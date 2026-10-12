# Project status and roadmap

Last source-tree status documented: 2026-10-12. This is a community snapshot, not a guarantee about the current state of every deployment.

## What exists today

- Rust 2024 application, version 0.1.0, with a HTTP/SSE HUD gateway and a compatibility bridge.
- 50 unique tool definitions. 48 are marked available for dispatch; PowerShell and RemoteTrigger are deferred. “Available” does not mean every tool has passed an independent semantic conformance test.
- Provider streaming, limited same-provider failover, per-profile concurrency controls, retry/backoff handling, and token/cost metrics when valid usage and pricing data are supplied.
- Tool schemas, ToolSearch, file tools, Bash, agents/workflows, tasks/todos, plans/worktrees, persistent schedules, web tools, skills, MCP/LSP adapters, interaction/deliverable tools, and memory tools.
- Context loading from supported project instruction files and a bounded context-compaction/hook pipeline.
- Declarative policy and execution modes: plan, ask, auto-edit, auto.
- Linux Landlock/seccomp sandboxing for supported subprocess paths; setup failures should fail closed.
- SQLite-backed messages, memory, tasks, and schedules.
- Append-only JSONL audit metadata with SHA-256 chaining and startup verification.
- Automatic pre-write snapshots for supported harness file tools and explicit preview/confirmation for local rewind.
- A bundled companion HUD client under web/ and GitHub Actions CI.

## Last documented test evidence

The working source snapshot had passed 60 Rust unit tests plus 3 tool-schema tests, formatting and script checks. The deterministic mock suite was recorded at 33 tool probes and 85 end-to-end checks on a later source snapshot. Staging runs also recorded 6 successful sequential real-model turns and 6 successful concurrent turns under a configured concurrency limit of 2.

These are historical results from the source snapshot and staging environment described by the maintainers; they are not a guarantee that every tool is independently conformant, that every provider works, or that a new checkout is production-ready. The first public CI run is the canonical check for the exact published commit.

## Known gaps that need contributors

### Security and deployment
- Implement and independently review gateway authentication/session cookies before exposing the execution API beyond loopback/trusted private networking.
- Expand prompt-injection and secret-exfiltration tests beyond local tool output.
- Design bounded, reviewable audit-log rotation/continuation without weakening integrity checks.
- Review reverse-proxy/tunnel configuration; do not put the execution gateway on a public unauthenticated endpoint.

### Tool and provider conformance
- WebSearch has been dispatched in a real-model run, but positive useful search results remain unverified on the tested environment.
- Successful public HTTPS WebFetch needs an independent positive test.
- Valid child-originated SubagentHandback needs a positive test.
- Several tools have dispatch/mock evidence but still need independent positive and negative semantic conformance tests.
- Provider-specific tool calling, streaming finalization and rate-limit behavior need broader replay and contract tests.

### Extensions and memory
- MCP currently covers a bounded stdio resource list/read subset; it is not full MCP.
- LSP was tested against a mock server; real installed language servers need integration tests.
- The full optional Node/TypeScript host and Cordis plugin lifecycle are not complete.
- Advanced memory graph/reflection/consolidation/embeddings and ACP still need implementation and evidence.

### Long-running quality
- A 200-turn / 40-tool-call workload and an 8-hour memory/RSS soak test remain open.
- Performance, token efficiency, cold-start time and resource use need repeatable benchmarks.
- PowerShell is deferred on the Linux host.
- RemoteTrigger is deferred pending an authenticated connector and an explicit allowlist.
- The earlier 1.5 GiB target must be measured with documented workloads; do not generalize one health reading to all processes and plugins.

## Contributor priority queue

1. Gateway auth and end-to-end tests for approval/session lifecycle.
2. Close the positive WebSearch, WebFetch and child handback test gaps.
3. Add per-tool conformance fixtures and provider contract recordings using synthetic credentials.
4. Validate MCP/LSP against real local servers.
5. Build the optional TypeScript/Cordis host with a strict resource lifecycle and explicit permissions.
6. Add long-run benchmarks/soak tests and publish the raw, sanitized methodology.
7. Improve portability and documentation for clean deployments on low-RAM x86-64 Linux.

## Hardware and maintenance context

The project is being developed on a constrained Linux x86-64 machine with an Atom-class CPU and a tight RAM budget. Full optimized Rust builds can take many minutes and memory pressure limits how many checks can run at once. Contributions from people with faster hardware, different kernels, and more time for long-run testing are particularly valuable. No project secrets or private device addresses are needed to contribute.


## Thomas Mobile Agent public package — 2026-10-12

The public source snapshot now includes the Android app, the private relay, the optional TLS/login gateway, the MCP stdio adapter, the CLI, protocol/configuration guidance, and a versioned test APK.

- Android source build passed with the Gradle wrapper: package com.thomas.mobileagent, manifest versionName=0.5.3, versionCode=12, min SDK 26, target/compile SDK 35, Java 17 source compatibility.
- The checked-in APK is a debug-signed developer artifact, not a production/release-key build. Manifest and APK v2 signature were verified; SHA-256 is recorded in mobile/releases/Thomas-Mobile-Agent-0.5.3-debug.apk.sha256.
- The app has no preselected server URL or username, rejects login unless a valid wss:// endpoint is entered, removes legacy local/tunnel endpoints from saved settings, and requests the optional runtime-permission bundle only after the user explicitly taps the permissions action.
- The Android build does not embed a live server certificate. System CA verification is the default. The relay's default bind addresses are loopback; the APK endpoint returns 404 unless the operator explicitly configures a verified artifact path.
- Relay runtime token creation uses a random 256-bit token with file mode 0600. The bootstrap command prompts for the relay username and password without echo and stores a salted scrypt hash in a private auth file. Tokens, sessions, TLS keys/certs, auth files, APK build intermediates and node_modules remain excluded from Git.
- Source syntax passed node --check on the relay, gateway, MCP adapter and auth bootstrap. Full runtime/on-device conformance has not yet been completed because the phone app was uninstalled; the publication includes reproducible build/test instructions, not a claim that the handset has been retested.
- The public profile README is published from the repository named exactly `sl7187855-coder`; the profile Bio and pinned repository settings are managed on the GitHub account. The profile README and repository README include the low-resource hardware context and contributor callouts.


## Maintainer hardware and community plan — 2026-10-12

The maintainer machine was inspected directly: Positivo Informática EC10IS1, Intel Atom N2600 @ 1.60 GHz, 2 physical cores/4 threads, approximately 2 GiB RAM detected by Linux, antiX/Debian 13, x86-64. The BIOS date field reports `04/11/2014`; this is a firmware date and not a verified manufacture date.

The maintainer intends to continue publishing updates, but the old low-resource host makes broad device coverage, long soak tests and some builds expensive. The public README now identifies the goals of Thomas as a persistent personal agent for long-running tasks, tool use, memory, lessons from outcomes and feedback. These are goals; automatic model-weight training is not claimed.

The profile README and project README are public. `CONTRIBUTING.md` provides a Quick Start, test commands, security rules and review expectations. Beginner-sized issues use the `good first issue` label, and the contributor graph is linked for public recognition.
