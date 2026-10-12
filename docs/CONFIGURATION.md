# Configuration

Harness-RS reads configuration from environment variables and a private data directory. It does not automatically load a .env file.

## Paths and listeners

| Variable | Default | Purpose |
|---|---|---|
| HARNESS_HOME | /var/lib/harness-rs | Private runtime root |
| HARNESS_WORKSPACE | HARNESS_HOME/workspace | Workspace accessible to the agent |
| HARNESS_DATA | HARNESS_HOME/data | Private database, settings and audit data |
| HARNESS_SKILLS | HARNESS_HOME/skills | Local SKILL.md files |
| HARNESS_PLUGINS | HARNESS_HOME/plugins | Declarative plugin directory |
| HARNESS_HUD_FILE | web/thomas-hud.html | HUD HTML path; set it to the bundled web/thomas-hud.html in a local checkout |
| HARNESS_BIND | 127.0.0.1:1347 | Main HTTP/SSE API bind address |
| HARNESS_BRIDGE_BIND | 127.0.0.1:3196 | Compatibility bridge bind address |
| HARNESS_API_BASE_URL | https://api.deepseek.com/v1 | API-compatible provider base URL |
| HARNESS_API_KEY | empty | Provider key; configure privately, never commit it |
| HARNESS_MODEL | deepseek-chat | Requested model name |
| HARNESS_MAX_TOOL_OUTPUT | 24000 | Maximum tool output characters |
| HARNESS_MAX_STEPS | 40 | Upper bound for agent steps |
| HARNESS_MAX_PARALLEL_TOOLS | 2 | Concurrent tool limit |
| HARNESS_EXECUTION_MODE | uses private settings, default ask | Optional validated mode override |

## Run with an isolated workspace

~~~sh
mkdir -p .local-home/data .local-workspace
export HARNESS_HOME="$PWD/.local-home"
export HARNESS_DATA="$PWD/.local-home/data"
export HARNESS_WORKSPACE="$PWD/.local-workspace"
export HARNESS_HUD_FILE="$PWD/web/thomas-hud.html"
export HARNESS_BIND="127.0.0.1:1347"
export HARNESS_BRIDGE_BIND="127.0.0.1:3196"
export HARNESS_EXECUTION_MODE="ask"
cargo run --release
~~~

Never point HARNESS_WORKSPACE at your home directory or a production checkout for an initial test.

## Provider credentials

Set HARNESS_API_BASE_URL, HARNESS_API_KEY and HARNESS_MODEL in the private process environment if you choose environment configuration. Do not put real values in this example, a shell script, a committed settings file, a bug report, or a screenshot. The HUD may also support managing provider profiles in the private runtime data store; those files must remain untracked.

A configured provider is required for model-backed turns. Unit and mock tests should work without a paid provider. Real-provider E2E tests require an explicitly configured disposable staging service and must not default to production.

## Execution modes

- plan: read-only; side-effecting tools are rejected.
- ask: writes and non-read-only shell commands require approval.
- auto-edit: file edits may run without the standard confirmation; other sensitive actions still have checks.
- auto: removes standard write/shell confirmation requirements but keeps hard validation, policy denials and the sandbox.

Use ask for new installations and untrusted workspaces. Do not treat a project-controlled policy file as permission to weaken a broader deny rule.
