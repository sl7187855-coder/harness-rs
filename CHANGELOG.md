# Changelog

## Unreleased

- Add per-profile FIFO model request limits, configured 402 key failover and post-retry HTTP 429 profile failover while preserving `RATE_LIMIT` when every profile is limited; retain Retry-After-aware 429/5xx retries, stream timeouts, prompt-cache markers, token usage and price-table metrics. Synthetic E2E now proves the 429 failover and complete Read → explicitly approved Write → Read → final flow.
- Add persistent context compaction, project persona loading, runtime execution modes, declarative policy, and lifecycle hooks.
- Add Linux Landlock/seccomp sandboxing for Bash, hooks, MCP and LSP; fail closed when sandbox setup is unavailable.
- Add MemorySearch/MemorySave/MemoryForget, ApplyPatch, visible memory-degraded health, and bounded MCP stdio/LSP clients.
- Add the remaining HUD gateway handlers, schema fixture conformance, deterministic provider mock, route tests, release staging and rollback scripts.
- Add local `/rewind` preview/list/confirmed restore with session/workspace ownership, SHA-256 integrity checks, inverse snapshots for both present and absent files, and no model call.
- Add a durable append-only JSONL audit stream with per-event SHA-256 chaining, startup verification, synced start/finish records for tool dispatches and `/rewind`, metadata-only privacy, and fail-closed behavior when audit integrity/storage is unhealthy.
- Align the audit file with WP-24 at `$HARNESS_DATA/audit.log`; add staging checks for audit health, restrictive file mode, record counts and independent verification of the hash chain.
- Redact configured and pattern-matched credentials in tool results before model reinjection, wrap results in an explicit untrusted-data envelope, and add regression tests for API tokens, bearer headers, JWTs, private keys and ordinary-content preservation.
- Add a deterministic adversarial-file E2E: malicious Read output tries to override policy and exfiltrate provider credentials; the test checks system guidance, untrusted envelope, redaction and refusal.
- Restore HTTPS upstream for the public mobile-only Tailscale Funnel and accept the intended `/mobile/agent` WebSocket path; the Harness API remains tailnet-only.

## 0.1.0 — 2026-10-10 baseline

- Record the existing Rust + Cordis runtime and 46-tool catalog as found before the conformance work.

- Add the Thomas Mobile Agent Android source and self-hosted relay/MCP adapter, document the protocol and permission requirements, remove baked-in network defaults, replace the predictable relay token with random token generation, require explicit operator TLS/auth/APK paths, and include the verified 0.5.3 debug APK as a test artifact.
- Add a contributor-focused public profile README draft and instructions for enabling the GitHub account profile README.

- Add the Thomas Mobile Agent Android source and self-hosted relay/MCP adapter, document protocol and permission requirements, remove baked-in network defaults, replace the predictable relay token with random token generation, require explicit operator TLS/auth/APK paths, and include the verified 0.5.3 debug APK as a test artifact.
- Add a contributor-focused public profile README draft and instructions for enabling the GitHub account profile README.
