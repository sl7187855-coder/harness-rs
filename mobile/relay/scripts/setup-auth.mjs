#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const secretDir = path.resolve(process.env.THOMAS_MOBILE_SECRET_DIR || path.join(root, "secrets"));
const authFile = path.resolve(process.env.THOMAS_PUBLIC_AUTH_FILE || path.join(secretDir, "public-auth.json"));
const reset = process.argv.includes("--reset");

async function ask(prompt) {
  const { createInterface } = await import("node:readline/promises");
  const io = createInterface({ input: process.stdin, output: process.stdout });
  try { return (await io.question(prompt)).trim(); }
  finally { io.close(); }
}

function askHidden(prompt) {
  return new Promise((resolve, reject) => {
    if (!process.stdin.isTTY || typeof process.stdin.setRawMode !== "function") {
      reject(new Error("Run this command from an interactive terminal so the password can be entered without echo."));
      return;
    }
    const input = process.stdin;
    let value = "";
    process.stdout.write(prompt);
    input.setEncoding("utf8");
    input.setRawMode(true);
    input.resume();
    const finish = (err) => {
      input.removeListener("data", onData);
      input.setRawMode(false);
      input.pause();
      process.stdout.write("\n");
      if (err) reject(err);
      else resolve(value);
    };
    const onData = chunk => {
      for (const char of chunk) {
        if (char === "\u0003") return finish(new Error("Cancelled."));
        if (char === "\r" || char === "\n") return finish();
        if (char === "\u007f" || char === "\b") value = value.slice(0, -1);
        else if (char >= " ") value += char;
      }
    };
    input.on("data", onData);
  });
}

function scryptHash(password, salt) {
  return crypto.scryptSync(password, salt, 32).toString("hex");
}

try {
  fs.mkdirSync(secretDir, { recursive: true, mode: 0o700 });
  fs.chmodSync(secretDir, 0o700);
  if (fs.existsSync(authFile) && !reset) {
    throw new Error("An account file already exists. Re-run with --reset only when you intend to replace the login.");
  }
  const username = await ask("New mobile-agent username: ");
  if (!username || username.length > 80) throw new Error("Username must contain 1–80 characters.");
  const password = await askHidden("Password (not echoed): ");
  if (password.length < 14) throw new Error("Use a password with at least 14 characters.");
  const repeat = await askHidden("Repeat password (not echoed): ");
  if (password !== repeat) throw new Error("Passwords did not match.");

  const salt = crypto.randomBytes(16);
  const record = {
    username,
    salt: salt.toString("base64"),
    hash: scryptHash(password, salt),
    createdAt: Date.now()
  };
  const temporary = authFile + ".tmp-" + crypto.randomUUID();
  fs.mkdirSync(path.dirname(authFile), { recursive: true, mode: 0o700 });
  fs.writeFileSync(temporary, JSON.stringify(record, null, 2) + "\n", { mode: 0o600, flag: "wx" });
  fs.chmodSync(temporary, 0o600);
  fs.renameSync(temporary, authFile);
  console.log("MOBILE_AUTH_CREATED");
  console.log("Account file:", authFile);
  console.log("File permissions set to 0600. Keep this file outside Git and back it up privately.");
} catch (error) {
  console.error("setup-auth:", error.message);
  process.exitCode = 1;
}
