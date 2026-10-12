const https=require("https");
const http=require("http");
const fs=require("fs");
const path=require("path");
const crypto=require("crypto");
const WebSocket=require("ws");

const ROOT=path.resolve(__dirname,"..");
const SECRET_DIR=path.resolve(process.env.THOMAS_MOBILE_SECRET_DIR||path.join(ROOT,"secrets"));
const TOKEN_FILE=path.join(SECRET_DIR,"agent.token");
const AUTH_FILE=path.resolve(process.env.THOMAS_PUBLIC_AUTH_FILE||path.join(SECRET_DIR,"public-auth.json"));
const SESSION_FILE=path.resolve(process.env.THOMAS_PUBLIC_SESSION_FILE||path.join(SECRET_DIR,"public-sessions.json"));
const TLS_KEY=process.env.THOMAS_PUBLIC_TLS_KEY||path.join(SECRET_DIR,"tls","server.key");
const TLS_CERT=process.env.THOMAS_PUBLIC_TLS_CERT||path.join(SECRET_DIR,"tls","server.crt");
const BIND=process.env.THOMAS_PUBLIC_WS_BIND||"127.0.0.1";
const PORT=Number(process.env.THOMAS_PUBLIC_WS_PORT||3197);
const UPSTREAM=process.env.THOMAS_MOBILE_UPSTREAM||"ws://127.0.0.1:3194/agent";
const SESSION_TTL_MS=30*24*60*60*1000;

fs.mkdirSync(SECRET_DIR,{recursive:true,mode:0o700});
function loadInternalToken(){
  try {
    fs.writeFileSync(TOKEN_FILE,crypto.randomBytes(32).toString("hex")+"\n",{mode:0o600,flag:"wx"});
  } catch(e) { if(e.code!=="EEXIST") throw e; }
  fs.chmodSync(TOKEN_FILE,0o600);
  const token=fs.readFileSync(TOKEN_FILE,"utf8").trim();
  if(!token) throw new Error("Mobile relay token is empty; refusing to start.");
  return token;
}
const INTERNAL_TOKEN=loadInternalToken();
function scryptHash(password,salt){return crypto.scryptSync(String(password),salt,32).toString("hex")}
function loadAuth(){if(!fs.existsSync(AUTH_FILE))return null;try{return JSON.parse(fs.readFileSync(AUTH_FILE,"utf8"))}catch{return null}}
function saveAuth(r){fs.writeFileSync(AUTH_FILE,JSON.stringify(r,null,2),{mode:0o600})}
function verifyPassword(record,password){
  if(!record?.salt||!record?.hash)return false;
  try{const a=Buffer.from(scryptHash(password,Buffer.from(record.salt,"base64")),"hex"),b=Buffer.from(record.hash,"hex");return a.length===b.length&&crypto.timingSafeEqual(a,b)}catch{return false}
}
let AUTH=loadAuth();
const SESSIONS=new Map();
function persistSessions(){try{fs.writeFileSync(SESSION_FILE,JSON.stringify([...SESSIONS.entries()]),{mode:0o600})}catch(e){console.error("[session] persist failed",e.message)}}
function loadSessions(){try{if(!fs.existsSync(SESSION_FILE))return;const data=JSON.parse(fs.readFileSync(SESSION_FILE,"utf8"));if(Array.isArray(data))for(const item of data){if(!Array.isArray(item)||item.length!==2)continue;const [token,s]=item;if(!token||!s||s.expiresAt<=Date.now())continue;SESSIONS.set(String(token),{username:String(s.username||""),createdAt:Number(s.createdAt)||Date.now(),expiresAt:Number(s.expiresAt)||0})}}catch(e){console.error("[session] load failed",e.message)}}
loadSessions();
function cleanup(){const now=Date.now();let changed=false;for(const [t,s] of SESSIONS)if(s.expiresAt<=now){SESSIONS.delete(t);changed=true}if(changed)persistSessions()}
setInterval(cleanup,3600000).unref();
function newSession(username){const token=crypto.randomBytes(32).toString("base64url");SESSIONS.set(token,{username,createdAt:Date.now(),expiresAt:Date.now()+SESSION_TTL_MS});persistSessions();return token}
function sessionFromToken(token){const t=String(token||"").trim(),s=SESSIONS.get(t);if(!s||s.expiresAt<=Date.now()){SESSIONS.delete(t);return null}return{token:t,...s}}
function sessionFromRequest(req){const m=String(req.headers.authorization||"").match(/^Bearer\s+(.+)$/i);return sessionFromToken(m?.[1])}
function sendJson(res,status,value){const body=JSON.stringify(value);res.writeHead(status,{"content-type":"application/json; charset=utf-8","cache-control":"no-store"});res.end(body)}
function readBody(req){return new Promise((resolve,reject)=>{let d="";req.on("data",c=>{d+=c;if(d.length>1048576){reject(new Error("body too large"));req.destroy()}});req.on("end",()=>{try{resolve(d?JSON.parse(d):{})}catch{reject(new Error("invalid JSON"))}});req.on("error",reject)})}
function send(ws,msg){if(ws.readyState===WebSocket.OPEN)ws.send(JSON.stringify(msg))}
const pending=new Map(),devices=new Map();
function socketPeer(ws){return String(ws?._socket?.remoteAddress||"unknown").replace(/^::ffff:/,"")}
function requestDevice(deviceId,action,params,timeoutMs=20000){
  const entry=devices.get(deviceId);if(!entry)throw new Error("device not connected");
  const requestId=crypto.randomUUID();
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{pending.delete(requestId);reject(new Error("mobile action timed out: "+action))},Math.max(1000,Math.min(timeoutMs,120000)));
    pending.set(requestId,{resolve,reject,timer,deviceId,action});
    send(entry.ws,{type:"command",requestId,action,params:params||{}});
  });
}
function handleAgentMessage(ws,raw,req){
  let msg;try{msg=JSON.parse(raw.toString())}catch{send(ws,{type:"error",code:"BAD_JSON"});return}
  if(msg.type==="register"){
    const session=sessionFromToken(msg.token);
    if(!session){send(ws,{type:"error",code:"AUTH_FAILED"});ws.close(1008,"login required");return}
    if(!msg.deviceId){send(ws,{type:"error",code:"DEVICE_ID_REQUIRED"});ws.close(1008,"device id required");return}
    const reportedDeviceId=String(msg.deviceId);
    for(const [id,e] of devices.entries())if(e.reportedDeviceId===reportedDeviceId&&e.ws!==ws){try{e.ws.close(4001,"replaced")}catch{};devices.delete(id)}
    const entry={ws,deviceId:reportedDeviceId,reportedDeviceId,username:session.username,peer:socketPeer(ws),name:String(msg.name||reportedDeviceId),version:String(msg.version||"unknown"),capabilities:msg.capabilities||[],connectedAt:new Date().toISOString()};
    devices.set(entry.deviceId,entry);ws.__deviceId=entry.deviceId;ws.__registered=true;ws.__username=session.username;
    send(ws,{type:"registered",deviceId:entry.deviceId,user:session.username});return
  }
  if(!ws.__registered){send(ws,{type:"error",code:"NOT_REGISTERED"});return}
  if(msg.type==="result"&&msg.requestId){const call=pending.get(msg.requestId);if(!call)return;clearTimeout(call.timer);pending.delete(msg.requestId);msg.ok===false?call.reject(new Error(msg.error||"mobile action failed")):call.resolve(msg.result);return}
  if(msg.type==="event")console.log("[agent-event]",ws.__deviceId,JSON.stringify(msg.event||{}));
}
const wss=new WebSocket.WebSocketServer({noServer:true});
wss.on("connection",(ws,req)=>{
  const authTimer=setTimeout(()=>{if(!ws.__registered)ws.close(1008,"registration timeout")},10000);
  ws.on("message",raw=>handleAgentMessage(ws,raw,req));
  ws.on("close",()=>{clearTimeout(authTimer);const id=ws.__deviceId,e=devices.get(id);if(id&&e?.ws===ws)devices.delete(id)});
  ws.on("error",e=>console.error("[agent] websocket error",e.message));
});
const server=https.createServer({key:fs.readFileSync(TLS_KEY),cert:fs.readFileSync(TLS_CERT),minVersion:"TLSv1.2"},async(req,res)=>{
  const u=new URL(req.url||"/","https://"+(req.headers.host||"thomas.local"));
  if(req.method==="GET"&&u.pathname==="/health")return sendJson(res,200,{ok:true,service:"thomas-mobile-public",tls:true,login:!!AUTH,devices:devices.size});
  if(req.method==="GET"&&u.pathname==="/apk"){
    const apk=String(process.env.THOMAS_MOBILE_APK_PATH||"").trim();
    if(!apk)return sendJson(res,404,{ok:false,error:"APK distribution is not configured. Set THOMAS_MOBILE_APK_PATH to a verified build artifact."});
    let stat;
    try{stat=fs.statSync(apk);}catch{return sendJson(res,404,{ok:false,error:"Configured mobile APK is missing."});}
    if(!stat.isFile()||stat.size<1024||stat.size>100*1024*1024)return sendJson(res,404,{ok:false,error:"Configured mobile APK is not a valid-sized file."});
    res.writeHead(200,{"content-type":"application/vnd.android.package-archive","content-length":stat.size,"content-disposition":"attachment; filename=Thomas-Mobile-Agent.apk","cache-control":"no-store"});
    return fs.createReadStream(apk).pipe(res);
  }
  if(req.method==="POST"&&u.pathname==="/login"){
    try{const body=await readBody(req);if(!AUTH)return sendJson(res,503,{ok:false,error:"auth not initialized"});const username=String(body.username||"").trim(),password=String(body.password||"");if(username!==AUTH.username||!verifyPassword(AUTH,password))return sendJson(res,401,{ok:false,error:"Usuário ou senha inválidos."});const sessionToken=newSession(username);return sendJson(res,200,{ok:true,username,sessionToken,expiresAt:Date.now()+SESSION_TTL_MS})}catch(e){return sendJson(res,400,{ok:false,error:e.message})}
  }
  if(req.method==="POST"&&u.pathname==="/logout"){const s=sessionFromRequest(req);if(s){SESSIONS.delete(s.token);persistSessions()}return sendJson(res,200,{ok:true})}
  if(req.method==="POST"&&u.pathname==="/change-password"){
    const s=sessionFromRequest(req);if(!s)return sendJson(res,401,{ok:false,error:"login required"});
    try{const body=await readBody(req),password=String(body.password||"");if(password.length<8)return sendJson(res,400,{ok:false,error:"A senha precisa ter pelo menos 8 caracteres."});const salt=crypto.randomBytes(16);AUTH={username:s.username,salt:salt.toString("base64"),hash:scryptHash(password,salt),updatedAt:Date.now()};saveAuth(AUTH);SESSIONS.clear();persistSessions();return sendJson(res,200,{ok:true,message:"Senha alterada. Faça login novamente."})}catch(e){return sendJson(res,400,{ok:false,error:e.message})}
  }
  return sendJson(res,404,{ok:false,error:"not found"});
});
server.on("upgrade",(req,socket,head)=>{
  const u=new URL(req.url||"/","https://"+(req.headers.host||"thomas.local"));
  if(u.pathname!=="/agent"&&u.pathname!=="/mobile/agent"){socket.destroy();return}
  wss.handleUpgrade(req,socket,head,ws=>wss.emit("connection",ws,req));
});
server.listen(PORT,BIND,()=>console.log("Thomas Mobile Public WSS listening on "+BIND+":"+PORT+"/agent"));

// Local HUD bridge: shares the exact same live device registry as the public WSS.
// It binds only to loopback, so the HUD does not need to reach the public TLS endpoint.
const LOCAL_API_HOST=process.env.THOMAS_PUBLIC_LOCAL_API_HOST||"127.0.0.1";
const LOCAL_API_PORT=Number(process.env.THOMAS_PUBLIC_LOCAL_API_PORT||3198);
const localApi=http.createServer(async(req,res)=>{
  const u=new URL(req.url||"/","http://"+(req.headers.host||"thomas.local"));
  if(req.method==="GET"&&u.pathname==="/health")return sendJson(res,200,{ok:true,service:"thomas-mobile-public-local",devices:devices.size});
  if(req.method==="GET"&&u.pathname==="/devices"){
    return sendJson(res,200,{devices:[...devices.values()].map(e=>({deviceId:e.deviceId,reportedDeviceId:e.reportedDeviceId,name:e.name,version:e.version,capabilities:e.capabilities,connectedAt:e.connectedAt,peer:e.peer||""}))});
  }
  if(req.method==="GET"&&u.pathname==="/frame"){
    try{
      const deviceId=u.searchParams.get("deviceId");
      if(!deviceId)return sendJson(res,400,{ok:false,error:"deviceId is required"});
      const result=await requestDevice(deviceId,"screenshot",{});
      const base64=String(result?.data||result?.base64||"");
      if(!base64)return sendJson(res,502,{ok:false,error:"device returned no screenshot"});
      const mime=String(result?.mime||"image/jpeg"),data=Buffer.from(base64,"base64");
      res.writeHead(200,{"Content-Type":mime,"Content-Length":data.length,"Cache-Control":"no-store","X-Mobile-Device":deviceId});
      return res.end(data);
    }catch(e){return sendJson(res,502,{ok:false,error:e.message});}
  }
  if(req.method==="POST"&&u.pathname==="/command"){
    try{
      const body=await readBody(req);
      if(!body.deviceId||!body.action)return sendJson(res,400,{ok:false,error:"deviceId and action are required"});
      const result=await requestDevice(String(body.deviceId),String(body.action),body.params||{},body.timeoutMs);
      return sendJson(res,200,{ok:true,result});
    }catch(e){return sendJson(res,502,{ok:false,error:e.message});}
  }
  if(req.method==="POST"&&u.pathname==="/command-all"){
    try{
      const body=await readBody(req);
      if(!body.action)return sendJson(res,400,{ok:false,error:"action is required"});
      const ids=Array.isArray(body.deviceIds)&&body.deviceIds.length?body.deviceIds.map(String):[...devices.keys()];
      const results=await Promise.all([...new Set(ids)].map(async deviceId=>{
        try{return {deviceId,ok:true,result:await requestDevice(deviceId,String(body.action),body.params||{},body.timeoutMs)}}
        catch(e){return {deviceId,ok:false,error:e.message}}
      }));
      return sendJson(res,200,{ok:results.every(x=>x.ok),action:String(body.action),count:results.length,results});
    }catch(e){return sendJson(res,400,{ok:false,error:e.message});}
  }
  return sendJson(res,404,{ok:false,error:"not found"});
});
localApi.listen(LOCAL_API_PORT,LOCAL_API_HOST,()=>console.log("Thomas Mobile HUD bridge listening on "+LOCAL_API_HOST+":"+LOCAL_API_PORT));
