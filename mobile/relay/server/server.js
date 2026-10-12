const http = require("http");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const os = require("os");
const WebSocket = require("ws");

const ROOT = path.resolve(__dirname, "..");
const SECRET_DIR = path.resolve(process.env.THOMAS_MOBILE_SECRET_DIR || path.join(ROOT, "secrets"));
const TOKEN_FILE = path.join(SECRET_DIR, "agent.token");
const WS_BIND_HOST = process.env.THOMAS_MOBILE_BIND_HOST || "127.0.0.1";
const WS_PUBLIC_HOST = process.env.THOMAS_MOBILE_PUBLIC_HOST || "";
const WS_PUBLIC_ENDPOINTS = String(process.env.THOMAS_MOBILE_PUBLIC_ENDPOINTS || "")
  .split(/[\s,;]+/).map(x => x.trim()).filter(Boolean);
const WS_PUBLIC_URL_FILE = process.env.THOMAS_MOBILE_PUBLIC_URL_FILE || "";
const WS_PORT = Number(process.env.THOMAS_MOBILE_PORT || 3194);
const API_HOST = process.env.THOMAS_MOBILE_API_HOST || "127.0.0.1";
const API_PORT = Number(process.env.THOMAS_MOBILE_API_PORT || 3195);
const pending = new Map();
const devices = new Map();

function loadToken() {
  fs.mkdirSync(SECRET_DIR, { recursive: true, mode: 0o700 });
  try {
    fs.writeFileSync(TOKEN_FILE, crypto.randomBytes(32).toString("hex") + "\n", {
      mode: 0o600, flag: "wx"
    });
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
  }
  fs.chmodSync(TOKEN_FILE, 0o600);
  const token = fs.readFileSync(TOKEN_FILE, "utf8").trim();
  if (!token) throw new Error("Mobile relay token is empty; refusing to start.");
  return token;
}

const TOKEN = loadToken();

function isPrivateIpv4(ip) {
  return /^(10|192\.168)\./.test(ip) || /^172\.(1[6-9]|2\d|3[0-1])\./.test(ip);
}
function isTailscaleIpv4(ip) {
  return /^100\.(6[4-9]|[7-9]\d|1[01]\d)\./.test(ip);
}
function networkHosts() {
  const found = [];
  for (const values of Object.values(os.networkInterfaces())) {
    for (const info of values || []) {
      if (info.family !== "IPv4" || info.internal) continue;
      const ip = String(info.address || "");
      if (!ip || found.includes(ip)) continue;
      found.push(ip);
    }
  }
  const lan = found.filter(isPrivateIpv4);
  const tailscale = found.filter(isTailscaleIpv4);
  const publicHosts = [...WS_PUBLIC_ENDPOINTS];
  try {
    if (WS_PUBLIC_URL_FILE && fs.existsSync(WS_PUBLIC_URL_FILE)) {
      let url = fs.readFileSync(WS_PUBLIC_URL_FILE, "utf8").trim();
      if (/^https:\/\//i.test(url)) url = "wss://" + url.slice(8);
      else if (/^http:\/\//i.test(url)) url = "ws://" + url.slice(7);
      if (url && !/[\\/]agent$/.test(url)) url += "/agent";
      if (url && !publicHosts.includes(url)) publicHosts.unshift(url);
    }
  } catch (_) {}
  const other = found.filter(ip => !lan.includes(ip) && !tailscale.includes(ip));
  return {
    primary: WS_PUBLIC_HOST || lan[0] || tailscale[0] || other[0] || "127.0.0.1",
    lan,
    tailscale,
    all: [...lan, ...tailscale, ...other],
    publicHosts,
  };
}

function json(res, status, value) {
  const body = JSON.stringify(value);
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store"
  });
  res.end(body);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = "";
    req.on("data", chunk => {
      data += chunk;
      if (data.length > 1024 * 1024) req.destroy(new Error("body too large"));
    });
    req.on("end", () => {
      try { resolve(data ? JSON.parse(data) : {}); }
      catch (err) { reject(new Error("invalid JSON")); }
    });
    req.on("error", reject);
  });
}

function send(ws, message) {
  if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(message));
}

function socketPeer(ws) {
  return String(ws?._socket?.remoteAddress || "unknown").replace(/^::ffff:/, "");
}

function publicDevice(entry) {
  return {
    deviceId: entry.deviceId,
    reportedDeviceId: entry.reportedDeviceId,
    name: entry.name,
    version: entry.version,
    capabilities: entry.capabilities,
    connectedAt: entry.connectedAt,
    peer: entry.peer || ""
  };
}
function requestDevice(deviceId, action, params, timeoutMs = 20000) {
  const entry = devices.get(deviceId);
  if (!entry) throw new Error(`device not connected: ${deviceId}`);
  if (!entry.ws || entry.ws.readyState !== WebSocket.OPEN) {
    if (devices.get(deviceId)?.ws === entry.ws) devices.delete(deviceId);
    throw new Error(`device socket is not open: ${deviceId}`);
  }
  const requestId = crypto.randomUUID();
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(requestId);
      reject(new Error(`mobile action timed out: ${action}`));
    }, Math.max(1000, Math.min(timeoutMs, 120000)));
    pending.set(requestId, { resolve, reject, timer, deviceId, action });
    send(entry.ws, {
      type: "command",
      requestId,
      action,
      params: params || {}
    });
  });
}

function handleAgentMessage(ws, raw) {
  let msg;
  try { msg = JSON.parse(raw.toString()); }
  catch { send(ws, { type: "error", code: "BAD_JSON" }); return; }

  if (msg.type === "register") {
    const peer = socketPeer(ws);
    if (msg.token !== TOKEN) {
      console.warn(`[agent] auth failed peer=${peer}`);
      send(ws, { type: "error", code: "AUTH_FAILED" });
      ws.close(1008, "authentication failed");
      return;
    }
    if (!msg.deviceId) {
      console.warn(`[agent] device id missing peer=${peer}`);
      send(ws, { type: "error", code: "DEVICE_ID_REQUIRED" });
      ws.close(1008, "device id required");
      return;
    }

    const reportedDeviceId = String(msg.deviceId);
    const deviceId = reportedDeviceId;

    // One physical phone = one canonical live socket, regardless of whether
    // it reaches the server through LAN, Tailscale, Funnel or a local proxy.
    // Replacing every older socket prevents stale entries from receiving
    // commands and eliminates ghost devices for the same physical phone.
    for (const [existingId, existing] of devices.entries()) {
      if (existing.reportedDeviceId === reportedDeviceId && existing.ws !== ws) {
        try { existing.ws.close(4001, "replaced"); } catch {}
        devices.delete(existingId);
      }
    }

    const entry = {
      ws,
      deviceId,
      reportedDeviceId,
      peerKey: peer.replace(/[^0-9A-Za-z_.:-]/g, "_") || "peer",
      peer,
      name: String(msg.name || reportedDeviceId),
      version: String(msg.version || "unknown"),
      capabilities: msg.capabilities || [],
      connectedAt: new Date().toISOString()
    };
    devices.set(entry.deviceId, entry);
    ws.__deviceId = entry.deviceId;
    ws.__registered = true;
    send(ws, {
      type: "registered",
      deviceId: entry.deviceId,
      reportedDeviceId: entry.reportedDeviceId
    });
    console.log(`[agent] connected ${entry.deviceId} reported=${entry.reportedDeviceId} peer=${peer}`);
    return;
  }

  if (!ws.__registered) {
    send(ws, { type: "error", code: "NOT_REGISTERED" });
    return;
  }

  if (msg.type === "result" && msg.requestId) {
    const call = pending.get(msg.requestId);
    if (!call) return;
    clearTimeout(call.timer);
    pending.delete(msg.requestId);
    if (msg.ok === false) call.reject(new Error(msg.error || "mobile action failed"));
    else call.resolve(msg.result);
    return;
  }

  if (msg.type === "event") {
    console.log(`[agent-event] ${ws.__deviceId} ${JSON.stringify(msg.event || {})}`);
  }
}

const wss = new WebSocket.WebSocketServer({ noServer: true });
wss.on("connection", (ws) => {
  const authTimer = setTimeout(() => {
    if (!ws.__registered) ws.close(1008, "registration timeout");
  }, 10000);

  ws.on("message", raw => handleAgentMessage(ws, raw));
  ws.on("close", () => {
    clearTimeout(authTimer);
    const deviceId = ws.__deviceId;
    if (!deviceId) return;
    const entry = devices.get(deviceId);
    if (entry?.ws === ws) {
      devices.delete(deviceId);
      console.log(`[agent] disconnected ${deviceId} reported=${entry.reportedDeviceId || deviceId}`);
    }
  });
  ws.on("error", err => console.error("[agent] websocket error", err.message));
});

const agentServer = http.createServer();
agentServer.on("upgrade", (req, socket, head) => {
  const url = new URL(req.url, `http://${req.headers.host || "localhost"}`);
  if (url.pathname !== "/agent") {
    socket.destroy();
    return;
  }
  wss.handleUpgrade(req, socket, head, ws => wss.emit("connection", ws, req));
});
async function commandApi(req, res) {
  const url = new URL(req.url, `http://${req.headers.host}`);
  if (req.method === "GET" && url.pathname === "/health") {
    return json(res, 200, { ok: true, service: "thomas-mobile-agent", devices: devices.size });
  }
  if (req.method === "GET" && url.pathname === "/devices") {
    const latestByPhone = new Map();
    for (const entry of devices.values()) {
      const key = entry.reportedDeviceId || entry.deviceId;
      const previous = latestByPhone.get(key);
      if (!previous || String(entry.connectedAt) > String(previous.connectedAt)) latestByPhone.set(key, entry);
    }
    return json(res, 200, { devices: [...latestByPhone.values()].map(publicDevice) });
  }
  if (req.method === "GET" && url.pathname === "/frame") {
    try {
      const deviceId = url.searchParams.get("deviceId");
      if (!deviceId) return json(res, 400, { ok: false, error: "deviceId is required" });
      const result = await requestDevice(deviceId, "screenshot", {});
      const base64 = String(result?.data || result?.base64 || "");
      if (!base64) return json(res, 502, { ok: false, error: "device returned no screenshot" });
      const mime = String(result?.mime || "image/jpeg");
      const data = Buffer.from(base64, "base64");
      res.writeHead(200, {
        "Content-Type": mime,
        "Content-Length": data.length,
        "Cache-Control": "no-store",
        "X-Mobile-Device": deviceId
      });
      return res.end(data);
    } catch (err) {
      return json(res, 502, { ok: false, error: err.message });
    }
  }
  if (req.method === "GET" && url.pathname === "/pairing") {
    const hosts = networkHosts();
    const wsUrls = [
      ...hosts.publicHosts.map(x => /^wss?:\/\//.test(x) ? x : "ws://" + x + ":" + WS_PORT + "/agent"),
      ...hosts.lan.map(x => "ws://" + x + ":" + WS_PORT + "/agent"),
      ...hosts.tailscale.map(x => "ws://" + x + ":" + WS_PORT + "/agent")
    ];
    return json(res, 200, {
      host: hosts.primary,
      hosts: hosts.all,
      lanHosts: hosts.lan,
      tailscaleHosts: hosts.tailscale,
      publicHosts: hosts.publicHosts,
      wsUrls,
      port: WS_PORT,
      path: "/agent",
      token: TOKEN
    });
  }
  if (req.method === "POST" && url.pathname === "/command-all") {
    try {
      const body = await readBody(req);
      const ids = Array.isArray(body.deviceIds) && body.deviceIds.length ? body.deviceIds.map(String) : [...devices.keys()];
      if (!body.action) return json(res, 400, { ok: false, error: "action is required" });
      const uniqueIds = [...new Set(ids)];
      const results = await Promise.all(uniqueIds.map(async deviceId => {
        try { const result = await requestDevice(deviceId, body.action, body.params || {}, body.timeoutMs); return { deviceId, ok: true, result }; }
        catch (err) { return { deviceId, ok: false, error: err.message }; }
      }));
      return json(res, 200, { ok: results.every(x => x.ok), action: String(body.action), count: results.length, results });
    } catch (err) { return json(res, 400, { ok: false, error: err.message }); }
  }

  if (req.method === "POST" && url.pathname === "/command") {
    try {
      const body = await readBody(req);
      if (!body.deviceId || !body.action) return json(res, 400, { ok: false, error: "deviceId and action are required" });
      const result = await requestDevice(body.deviceId, body.action, body.params || {}, body.timeoutMs);
      return json(res, 200, { ok: true, result });
    } catch (err) {
      return json(res, 502, { ok: false, error: err.message });
    }
  }
  return json(res, 404, { ok: false, error: "not found" });
}

const apiServer = http.createServer((req, res) => {
  commandApi(req, res).catch(err => json(res, 500, { ok: false, error: err.message }));
});
agentServer.listen(WS_PORT, WS_BIND_HOST, () => {
  const hosts = networkHosts();
  console.log(`Thomas Mobile Agent WS listening on ${WS_BIND_HOST}:${WS_PORT}/agent`);
  console.log("Thomas Mobile Agent endpoints: " + hosts.all.map(host => "ws://" + host + ":" + WS_PORT + "/agent").join(", "));
});
apiServer.listen(API_PORT, API_HOST, () => {
  console.log(`Thomas Mobile Agent API listening on ${API_HOST}:${API_PORT}`);
});

function shutdown() {
  for (const entry of devices.values()) entry.ws.close(1001, "server shutdown");
  for (const call of pending.values()) {
    clearTimeout(call.timer);
    call.reject(new Error("server shutting down"));
  }
  pending.clear();
  agentServer.close();
  apiServer.close();
}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
