# Thomas Mobile Agent

Thomas Mobile Agent is the optional Android-side component of the Thomas/Harness ecosystem. It pairs an Android device with a self-hosted relay and allows the user to explicitly authorize accessibility control and screen capture.

## Components

- Android app (agent-android/): Java 17 / Android SDK project, package com.thomas.mobileagent. The public source targets version 0.5.3, versionCode 12, minimum API 26 and target API 35.
- Private relay (relay/server/server.js): WebSocket agent protocol on a loopback listener plus a loopback HTTP command API.
- TLS/login gateway (relay/server/public-ws-proxy.js): optional TLS WebSocket endpoint, user/password login, expiring bearer sessions, and a local bridge for the HUD. It binds to loopback by default; public binding must be opted into deliberately.
- MCP tools (relay/mcp-server.mjs): tools for listing paired devices, observing, tapping/swiping/typing, app launch, system navigation, screen capture, device information, battery/status, and clipboard operations.
- CLI (relay/bin/mobilectl): local administrative/diagnostic client.

The relay, TLS keys, login database, sessions, internal device token and provider/API credentials are runtime data. They are deliberately excluded from Git.

## What is reported by the Android app

The registration payload contains a device ID derived by Android, a human-readable manufacturer/model label, the app version, and a list of declared action capabilities. The relay associates commands with the currently registered device socket and returns bounded results keyed by request ID. A device registration is not proof that every permission-dependent capability is available: permission and device-policy checks remain on the handset.

The app requests a TLS WebSocket endpoint configured by the user. The expected WebSocket path is /agent; the gateway login endpoint is HTTPS at the same origin. Login produces a session token stored in the app's private SharedPreferences. Logout clears the local token and requests server-side session revocation.

## Security model

The Android app starts with no server URL and no username preselected. Enter a trusted wss:// endpoint and user credentials supplied by whoever operates the relay. Use a certificate issued by a public CA for internet deployments. Do not disable TLS validation or commit a live server certificate/private key.

Android accessibility and screen-capture permissions are sensitive. Enable them only on devices you own or have explicit permission to administer. Screen capture requires the Android system consent prompt. The app does not attempt to bypass lock-screen credentials, Android permission prompts, or device-owner restrictions.

The included APK is a debug-signed developer build for testing. It is not a Play Store release and is not signed with a production release key. Verify the published SHA-256 before installation.

## Build the Android app

See agent-android/README.md. A working Android SDK, platform 35, build-tools 35.0.0, Java 17 and network access to Gradle/Maven repositories are required.

## Run the relay

See relay/README.md. Keep both private listeners on loopback, set a private THOMAS_MOBILE_SECRET_DIR, create a user with the bootstrap script, and provide TLS key/certificate files outside the repository. Expose the TLS gateway only after configuring authentication, TLS, firewalling and rate limiting.

## Contributor opportunities

We need help with Android device/OS compatibility, reconnect/session lifecycle, protocol conformance, least-privilege permissions, mobile tests and installation documentation. See the root CONTRIBUTING.md and docs/PROJECT_STATUS.md.

Mobile Agent is part of the broader Harness-RS project, but its Android app and relay can be reviewed and tested independently.
