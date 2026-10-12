# Architecture

## Process model

The main binary starts one Rust application process and two loopback listeners:

- Main HTTP/SSE gateway: HARNESS_BIND, default 127.0.0.1:1347.
- Compatibility bridge: HARNESS_BRIDGE_BIND, default 127.0.0.1:3196.

The gateway serves the selected HUD HTML file and exposes the API-compatible chat, status, settings, tools, history, memory, tasks and related integration routes. The bridge exposes a narrower compatibility surface for the companion HUD. A reverse proxy or private overlay network may be used by an operator, but the agent API should not be exposed to the public internet until gateway authentication is implemented and reviewed.

## Agent turn lifecycle

1. Validate the user request and resolve the selected provider profile/model.
2. Load session history, project instruction files, configured skills/hooks and bounded context.
3. Build the active tool list. ToolSearch can load additional catalog entries for the current session.
4. Send the request to the configured model over the provider streaming API.
5. Validate and dispatch any requested tool calls. Side-effecting tools pass through plan-mode checks, hooks, declarative policy and approval requirements before execution.
6. Run supported commands/children under the Linux sandbox. Tool outputs are bounded and sanitized before the model sees them.
7. Save the result/state, emit a final event, and record bounded audit metadata.

Cancellation and deadlines must stop work cooperatively. A permission request is a real pending interaction resolved by the HUD; a sentence from the model asking for permission is not itself an approval event.

## Tool and permission layers

- The tool catalog defines stable identifiers, JSON schemas, grouping and availability state.
- ToolSearch loads ready entries on demand; deferred tools are not executed.
- Execution modes are plan, ask, auto-edit and auto.
- Explicit deny takes precedence over ask/allow. Invalid policies fail closed for side-effecting tools.
- Hooks can observe or update bounded input, add context, allow, ask or deny. Security-event errors fail closed.
- Linux subprocess sandboxing uses Landlock/seccomp for supported paths and has no automatic unsandboxed fallback.
- File-tool snapshots and /rewind provide bounded rollback for supported direct file operations. Edits done indirectly by Bash or an external process are not automatically rewindable.

## Context, skills and memory

Project instruction files such as AGENTS.md, PERSONALIDADE.md and CLAUDE.md are loaded according to the current loader behavior. Local SKILL.md content is treated as untrusted instructions and must not elevate tool permissions. Context compaction is bounded to avoid losing recent turns and tool-result pairing.

SQLite stores session messages and the supported memory, task and schedule records. Full-text search uses SQLite FTS. Memory-write filtering rejects detected credentials/secrets, but no detector can recognize every proprietary format; secrets do not belong in prompts or memory.

## Provider layer

Providers are configured through the private runtime data store or environment variables. The application supports OpenAI-compatible streaming and a separate adapter for Anthropic Messages. Retry/backoff, provider-specific failure classification, configured same-provider fallback and per-profile concurrency limits reduce avoidable failures. They do not guarantee model quality or prevent upstream rate limits. Token usage/cost is only priced when the provider reports usage and valid per-model price data is configured.

No API key is stored in the repository. Mock tests use synthetic credentials only.

## Audit and untrusted output

Each supported tool dispatch records a bounded start/finish event in a local append-only JSONL audit file. Record hashes form a SHA-256 chain that is verified on startup. Audit storage or integrity failure blocks supported tool dispatch. The log intentionally excludes prompts, full tool arguments, provider messages and credentials.

Tool outputs have common credential patterns redacted and are wrapped as untrusted data before model reinjection. This reduces risk but is defense in depth, not proof that every input source or secret format is safe.

## Extension protocols

- Skills: local SKILL.md files.
- Hooks: bounded commands with JSON input/output; they execute in a read-only sandbox for supported hooks.
- MCP: bounded stdio resource discovery/read subset with short-lived child processes; remote HTTP, prompts, sampling, elicitation and full server-tool lifecycle are not claimed.
- LSP: JSON-RPC over stdio for selected definition/reference/diagnostic methods. An actual server must be configured explicitly.
- TypeScript/Cordis plugins: the complete Node host/lifecycle remains future work.
- ACP, full WASM support and advanced memory are roadmap items, not current claims.

## Data and trust boundaries

The workspace is the primary boundary for file tools. HARNESS_HOME and HARNESS_DATA hold private settings and state. Provider credentials and user-specific provider profiles must stay outside Git. Public tunnel exposure, unreviewed hook/plugin commands, symlink traversal and network access from subprocesses are treated as security-sensitive.
