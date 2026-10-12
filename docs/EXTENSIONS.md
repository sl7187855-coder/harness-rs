# Runtime controls, MCP, LSP, hooks and policy

## Execution modes

The effective mode is chosen from a valid HARNESS_EXECUTION_MODE override when set, otherwise settings.json.execution_mode, otherwise ask. The only allowed values are:

- plan: read-only; every side-effecting tool is rejected even if a model requests it.
- ask: writes and non-read-only shell commands require approval.
- auto-edit: file edits can run automatically; non-read-only shell commands and privileged actions still require approval.
- auto: removes the standard write/shell confirmation requirement, but hard safety checks, denied policy rules, tool validation and the Landlock/seccomp sandbox still apply.

The HUD settings API returns execution_mode and rejects unrecognized values with HTTP 400. Do not use auto on an untrusted workspace. The runit service should not hard-code HARNESS_EXECUTION_MODE=ask if changes through the HUD are intended to persist; absence of the variable defaults to ask.

## Declarative policy

Copy policy.example.toml to $HARNESS_HOME/policy.toml for trusted user rules or to <workspace>/.harness/policy.toml for project rules. Matching uses glob patterns. Explicit deny wins over ask and allow. A project-controlled allow on side-effecting tools is demoted to ask, so checking out a hostile repository cannot auto-authorize commands. Malformed policy fails closed for side-effecting tools.

## Hooks

Copy hooks.example.toml and replace the placeholder scripts with reviewed hooks. Hook commands receive a single JSON object on stdin (session_id, hook_event_name, tool_name, tool_input, cwd, and source metadata). They may write one JSON object to stdout containing:
- decision: allow, ask, or deny;
- reason: explanatory text;
- additionalContext: up to 8 KiB of extra context;
- updatedInput: bounded replacement object for user_prompt_submit and pre_tool_use.

Supported events: session_start, session_end, user_prompt_submit, pre_tool_use, permission_request, post_tool_use, pre_compact, post_compact, subagent_start, subagent_stop, memory_write, stop. Commands run with a cleared environment under the read-only sandbox, without network syscalls, with an operational bound capped at ten seconds, 64 KiB output limits and process-group termination on timeout. Errors from security events fail closed; observational hook errors are logged and ignored. Treat hook JSON, additionalContext and all returned content as untrusted data, never as policy or system instructions.

## MCP resources

The stdio resources adapter reads $HARNESS_DATA/mcp-servers.json; a schema example is mcp-servers.example.json. Each configured server needs a unique name, an absolute command path, optional bounded args, and enabled. The adapter launches one short-lived child per tool call, performs MCP initialize, sends notifications/initialized, and calls resources/list or resources/read. It never invokes a shell, clears the child environment, caps messages at 1 MiB and startup at eight seconds, and denies network syscalls using seccomp. Resource results are bounded to 20,000 text characters per item; binary blobs are not returned to the model. With no configuration, the tools explicitly report that zero servers are configured.

This version intentionally implements the stdio resources subset. MCP remote HTTP, prompt templates, server tools, sampling, elicitation and durable server processes are not claimed as implemented.

## LSP

The LSP adapter reads $HARNESS_DATA/lsp-servers.json; use lsp-servers.example.json as a template. Each configured server maps a unique name, an absolute executable path, bounded argument list, and the file extensions it owns. The adapter speaks LSP JSON-RPC over stdio using Content-Length framing, initializes a workspace, opens a UTF-8 source file, then calls textDocument/definition, textDocument/references, or textDocument/diagnostic. Lines and characters are zero-based. It refuses files outside the workspace, private/hidden files and files larger than 512 KiB; response frames are capped at 1 MiB and output at 64 KiB. The language server is launched in read-only Landlock/seccomp mode with network access blocked.

No language server is bundled or assumed. The tool returns guidance when no enabled server is configured for that extension. The unit suite uses a local mock LSP server; an actual installed server must be tested separately before claiming working diagnostics for that language.

## Sandboxing

Bash foreground/background commands, configured hooks, MCP servers and LSP servers use Landlock plus seccomp on Linux. Setup errors fail closed; there is no automatic unsandboxed fallback. Writes are allowed only within the active workspace and a private per-execution temporary directory. Network socket operations, ptrace, mount/namespace changes, kernel module loading and selected high-risk syscalls are denied. Resource limits are applied before the child executes the requested command. This is an OS-level layer and does not replace least privilege, approval prompts or careful configuration of trusted local executables.
