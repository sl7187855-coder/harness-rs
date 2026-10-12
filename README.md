# Harness-RS

**A resource-conscious coding-agent runtime in Rust, inspired by Cordis concepts.** Harness-RS combines a tool-using agent loop, an HTTP/SSE compatibility API, persistent SQLite state, skills, hooks, policy-based approvals, Linux sandboxing, and a HUD client.

> **Status: early alpha / active community development.** This repository is useful for experimentation and contributions, not a promise of drop-in Codex CLI or Claude Code parity. The tool catalog contains 50 names, 48 marked available for dispatch and 2 deferred; availability in the catalog is not the same as independent conformance certification.

**Português:** Este projeto nasceu para criar um harness de agentes de código em Rust que consiga funcionar com poucos recursos. Estamos trabalhando em uma máquina Linux x86-64 bastante limitada: compilações completas demoram, memória é apertada e alguns testes longos são difíceis de executar localmente. Precisamos de contribuidores para revisar segurança, testar em outras máquinas, melhorar documentação, ampliar os testes e implementar as partes que ainda faltam.

**Help wanted:** Rust, Linux sandboxing, provider/API compatibility, browser/HUD testing, MCP/LSP, documentation, performance, and security review. Start with [CONTRIBUTING.md](CONTRIBUTING.md) and [the project status](docs/PROJECT_STATUS.md).

## Goals

- Keep the harness runtime and its child processes within a target budget of **1.5 GiB RAM** on constrained hosts. This is a design/acceptance target, not a guarantee for arbitrary plugins, language servers, or user commands.
- Keep the core in Rust and implement the useful lifecycle ideas of Cordis (context, fibers/lifecycle, services, events) through the Rust runtime. The npm/TypeScript Cordis plugin host is a future item, not claimed as complete.
- Provide a compatible HTTP and Server-Sent Events (SSE) gateway for a companion HUD.
- Support tool discovery, skills, hooks, context files, SQLite memory, task/schedule persistence, and explicit permission decisions.
- Fail closed when sandbox setup, permission policy, or audit integrity checks fail.
- Make missing functionality visible instead of silently claiming it works.

## Current capability snapshot

| Area | State |
|---|---|
| Rust agent loop and provider streaming | Implemented; provider-specific differences still need coverage |
| HUD-compatible HTTP/SSE gateway | Implemented; gateway login/session-cookie authentication remains open |
| Tool catalog | 50 unique names: 48 available for dispatch, 2 deferred |
| File tools | Read, Write, Edit, NotebookEdit, ApplyPatch, Glob, Grep; automatic rewind snapshots cover harness file tools |
| Execution safety | Approval modes, declarative policy, hooks, Landlock/seccomp sandbox on Linux |
| Context and skills | Loads supported project instruction files and local SKILL.md files |
| Persistent state | SQLite messages, memory search/save/forget, tasks, schedules |
| Audit | Append-only JSONL records with a SHA-256 hash chain; dispatch fails closed when audit is unhealthy |
| MCP | Bounded stdio resource list/read subset; no claim of full MCP protocol coverage |
| LSP | JSON-RPC stdio adapter; verified with mock servers, not a bundled language-server fleet |
| TypeScript Cordis plugin lifecycle | Not implemented end to end |
| PowerShell | Deferred on the Linux host |
| RemoteTrigger | Deferred until an authenticated connector and routine allowlist exist |

Read [PROJECT_STATUS.md](docs/PROJECT_STATUS.md) for known gaps and [CONFORMANCE.md](docs/CONFORMANCE.md) for the evidence and limits of the tests.

## Architecture at a glance

~~~mermaid
flowchart TD
  HUD["HUD client"] <-->|"HTTP + SSE"| GW["Rust gateway / compatibility bridge"]
  GW <--> AG["Agent loop and provider adapter"]
  AG --> DISC["Tool discovery and schemas"]
  AG --> CTX["Context, skills, hooks"]
  AG --> POL["Permission policy and approval gate"]
  POL --> SAN["Sandboxed tool runtime"]
  SAN --> FS["Workspace files / Bash / MCP / LSP"]
  AG <--> DB["SQLite messages, memory, tasks, schedules"]
  SAN --> AUD["Hash-chained audit trail"]
  AG <-->|"API-compatible provider"| MODEL["Configured remote model"]
~~~

The local API should remain bound to loopback or a trusted private network until explicit gateway authentication has been implemented and independently reviewed. Do not expose the execution API publicly or use a public tunnel as an authentication substitute.

## Requirements

- Linux is the primary supported execution and sandbox target.
- Rust 1.88+ (edition 2024).
- A C toolchain and the platform dependencies required by Cargo crates.
- Node.js is required only for the inline-JavaScript syntax gate for the bundled HUD.
- A configured API-compatible model provider is needed for live model calls. No provider key is included in this repository.

## Build and test

Run from the repository root:

~~~sh
cargo fmt --check
CARGO_BUILD_JOBS=1 cargo test --locked --all-targets
CARGO_BUILD_JOBS=1 cargo clippy --locked --all-targets
CARGO_BUILD_JOBS=1 cargo build --locked --release
python3 -m py_compile tests/e2e/*.py
bash -n scripts/deploy.sh
sh -n scripts/test.sh
python3 scripts/check_inline_js.py web/thomas-hud.html
~~~

The full release build may be slow on low-power hardware. Use one Cargo job on constrained machines. Do not run provider tests against production. The real-provider tests require an explicit disposable staging URL and credentials configured outside the repository.

The GitHub Actions workflow runs the core tests, creates a generic x86-64 baseline release artifact, validates the Python/shell scripts, and checks the bundled HUD's inline JavaScript syntax.

## Quick local start

Create an isolated working directory and run the binary bound to loopback:

~~~sh
mkdir -p .local-workspace .local-home/data
export HARNESS_HOME="$PWD/.local-home"
export HARNESS_DATA="$PWD/.local-home/data"
export HARNESS_WORKSPACE="$PWD/.local-workspace"
export HARNESS_BIND="127.0.0.1:1347"
export HARNESS_BRIDGE_BIND="127.0.0.1:3196"
export HARNESS_HUD_FILE="$PWD/web/thomas-hud.html"
export HARNESS_EXECUTION_MODE="ask"
cargo run --release
~~~

Then open the local HUD URL served by the gateway (normally http://127.0.0.1:1347/). Configure a provider through the local settings UI or set the provider environment variables in a private shell. This project does not load a .env file automatically. Never put keys in source code, screenshots, bug reports, command history, or committed settings.

To use environment credentials, set HARNESS_API_BASE_URL, HARNESS_API_KEY, and HARNESS_MODEL in your private environment before starting the server. Prefer the UI's private provider configuration where supported. The sample defaults are not a substitute for a configured model.

## Security and privacy

- Default execution mode is ask unless a valid administrator override is configured.
- Do not run this agent against an untrusted workspace with auto mode.
- Treat project instruction files, skills, hooks, MCP servers, LSP executables, and external tool results as inputs requiring review.
- Never commit .env files, provider profiles, databases, audit logs, production binaries, backups, or local deployment archives.
- The audit log intentionally records bounded metadata only; it is not a digital signature against an administrator who can rewrite the whole log.
- The sandbox is defense in depth, not a guarantee that every possible host setup is secure.
- Gateway authentication/cookies and additional prompt-injection/red-team coverage remain open tasks.

See [SECURITY.md](SECURITY.md) before reporting vulnerabilities.

## Thomas Mobile Agent (Android companion)

The repository now includes the optional Android agent, relay, local CLI and MCP adapter under `mobile/`. The Android source targets version **0.5.3** (versionCode 12); a debug-signed test APK is checked in under `mobile/releases/` after its manifest and SHA-256 are verified. The app ships with no preselected server URL or username. Read [the mobile overview](mobile/README.md), [Android build instructions](mobile/agent-android/README.md), and [relay security/setup](mobile/relay/README.md) before connecting a device.

**Security note:** Android accessibility and screen capture are powerful capabilities and require explicit on-device permission. Relay account files, device tokens, TLS private keys, session data and live provider/API keys are not included in the repository. The relay binds to loopback by default; do not expose it externally until TLS, authentication, firewalling and session handling are reviewed.

## Roadmap and limitations

The biggest open work is recorded in [PROJECT_STATUS.md](docs/PROJECT_STATUS.md): gateway authentication, independent positive tests for WebSearch and public HTTPS WebFetch, valid child-originated SubagentHandback, configured real MCP/LSP servers, a full TypeScript Cordis plugin host, advanced memory/ACP, per-tool conformance, long-run soak tests, performance benchmarks, and a review of externally reachable routing. PowerShell and RemoteTrigger remain deferred.

The [original full plan](docs/PROJECT_PLAN_ORIGINAL.md) is preserved as a historical design document. It explains the initial goals and phases; it is not a claim that all roadmap items are complete.

## Contributing

We need help from people with more powerful hardware and diverse Linux setups. Good first contributions include a test that does not need a paid API, improving a tool's negative/error-path coverage, trying the build on another x86-64 host, verifying docs, and implementing an explicitly listed gap. Please include reproduction steps, test output, architecture/OS details, and whether a real provider was involved. Redact tokens and private URLs.

Thanks for helping make a safer, more dependable agent runtime possible.


### Thomas Mobile Agent 0.5.3 artifact integrity

The debug-signed Android test APK at mobile/releases/Thomas-Mobile-Agent-0.5.3-debug.apk has SHA-256:

    625a5a030981a61ca4a23503c3741b9e5cd0d2399950beeb1ea2e3a02af3333b

Compare this value against the sidecar checksum before installing. This is a developer APK, not a production-signing-key release.
