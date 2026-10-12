# Implementation gaps (public summary)

This file is a short index for contributors. The full initial design is preserved in PROJECT_PLAN_ORIGINAL.md; verified behavior is tracked in PROJECT_STATUS.md and CONFORMANCE.md.

## Implemented core

- Rust agent loop and API-compatible HTTP/SSE gateway.
- 50-name tool catalog with on-demand ToolSearch.
- Provider streaming, retry/fallback/concurrency controls and usage metrics.
- Project context/persona loading, compaction, skills, lifecycle hooks, declarative policy and execution modes.
- Linux Landlock/seccomp sandbox for supported child-process paths, fail-closed setup, and explicit approval interactions.
- SQLite memory/tasks/schedules; bounded MCP stdio resources and LSP JSON-RPC adapters.
- Tool-result credential filtering and untrusted-data envelope.
- Hash-chained metadata-only audit log and supported file rewind snapshots.
- Companion HUD and deterministic mock tests.

## Open work

- Gateway authentication/session cookies and a security review of externally reachable routing.
- Positive real results for WebSearch, public HTTPS WebFetch and child-originated SubagentHandback.
- Independent per-tool conformance, wider provider contract tests and stronger streaming/finalization recovery.
- Full TypeScript/Cordis plugin host and real third-party plugin lifecycle.
- Real configured MCP and LSP server coverage.
- Advanced memory graph/reflection/consolidation/embeddings and ACP.
- 200-turn/40-tool-call workload, 8-hour memory/RSS soak, and reproducible performance/token benchmarks.
- PowerShell and RemoteTrigger are deferred, not silently treated as implemented.

Do not claim the whole roadmap or parity with Codex/Claude Code is complete. The current status file records what has and has not been independently verified.
