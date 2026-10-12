# Thomas Mobile Agent relay

The relay connects the Android app to authorized local tooling. It has two distinct processes:

| Component | Default listener | Purpose |
|---|---|---|
| Private WebSocket agent relay | 127.0.0.1:3194 | Android agent registration/command/result protocol |
| Private HTTP command API | 127.0.0.1:3195 | Device listing, screenshots and commands for the local HUD/MCP |
| Optional TLS/WSS gateway | 127.0.0.1:3197 | HTTPS login, session authentication and /agent WebSocket upgrade |
| Optional local HUD bridge | 127.0.0.1:3198 | Private local integration with the public WSS registry |

All ports can be configured via environment variables. Only the TLS gateway should ever be considered for a remote connection, and only after you explicitly configure TLS, an account, firewall rules and rate limiting.

## Install dependencies

Requires Node.js 20+. In this directory:

~~~sh
npm ci
~~~

mcp-server.mjs uses the official @modelcontextprotocol/sdk, zod, and the relay uses ws.

## Private deployment setup

Place runtime state outside the checkout. Set the same THOMAS_MOBILE_SECRET_DIR for both server processes so they share the generated internal agent token. For example, choose a service-owned directory with restrictive permissions, not a tracked folder.

~~~sh
export THOMAS_MOBILE_SECRET_DIR="/var/lib/thomas-mobile/secrets"
export THOMAS_MOBILE_BIND_HOST="127.0.0.1"
export THOMAS_MOBILE_API_HOST="127.0.0.1"
export THOMAS_PUBLIC_WS_BIND="127.0.0.1"
export THOMAS_PUBLIC_TLS_KEY="/secure/path/server.key"
export THOMAS_PUBLIC_TLS_CERT="/secure/path/server.crt"
export THOMAS_MOBILE_APK_PATH="/secure/path/Thomas-Mobile-Agent-0.5.3-debug.apk"
npm run setup-auth
~~~

The auth setup command prompts for a username and password without echoing the password and writes a 0600 password-hash record to the selected private secret directory. It refuses to overwrite an existing record unless --reset is used.

Start the private relay:

~~~sh
npm start
~~~

In another service/terminal with the same THOMAS_MOBILE_SECRET_DIR:

~~~sh
npm run start:public
~~~

The public gateway uses TLS and requires login. Its bind address stays on loopback by default. For a deliberate remote deployment, configure the gateway to bind on the intended interface only after making sure the certificate is valid, authentication works, and an external firewall/reverse proxy limits access. Set THOMAS_PUBLIC_WS_BIND=0.0.0.0 only as an explicit operator decision; do not expose the private HTTP API port 3195.

The /apk endpoint is disabled unless THOMAS_MOBILE_APK_PATH points to a verified artifact. The code contains no developer-machine APK path. No server private key, device token, login database or active session is bundled.

## MCP adapter

Run npm run mcp as an MCP stdio server in an MCP-compatible client. Set THOMAS_MOBILE_API only to a private loopback command API you control. The adapter can discover phones, observe Android accessibility state, interact with visible UI, open apps/URLs/settings, read device/battery state, control audio/brightness and use clipboard/screenshot tools. These are high-impact capabilities: only connect it to devices you own or explicitly administer. MCP stdio transport inherits the user's process permissions, so configure the client to ask before side effects.

## Known limitations

- No automatic public endpoint, live account, password, internal token or TLS certificate is committed.
- The sample build APK is debug-signed and must not be treated as a production distribution.
- Mobile commands require a live paired app; relay health alone does not prove the phone is connected.
- Review all action allowlists and Android permission behavior before exposing the gateway.
- Test and inspect this code in a disposable environment before use with a personal device.
