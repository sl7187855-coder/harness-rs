# Contributing to Harness-RS

Thanks for considering a contribution. This project needs practical help: the maintainer currently works on a low-resource x86-64 Linux host, where optimized builds and long-running tests consume a disproportionate amount of time and memory. Reproducible tests and reports from other machines are especially valuable.

## Before opening a pull request

1. Read README.md, docs/PROJECT_STATUS.md, docs/CONFORMANCE.md, and docs/EXTENSIONS.md.
2. Search existing issues and pull requests so work is not duplicated.
3. For a behavior change, add or update a test. Prefer deterministic tests with synthetic providers and temporary workspaces.
4. Keep APIs and tool schemas backward-compatible unless the change explicitly documents a breaking change.
5. Do not weaken default permissions, sandbox boundaries, audit integrity, or fail-closed behavior to make a test pass.

## Development checks

Run what is available on your machine:

~~~sh
cargo fmt --check
CARGO_BUILD_JOBS=1 cargo test --locked --all-targets
CARGO_BUILD_JOBS=1 cargo clippy --locked --all-targets
python3 -m py_compile tests/e2e/*.py
bash -n scripts/deploy.sh
sh -n scripts/test.sh
python3 scripts/check_inline_js.py web/thomas-hud.html
~~~

If resources are limited, run focused Rust tests first and report the exact command and result. The CI workflow is the shared full check. Never point tests at a production service.

## Good areas for contributors

- **Security:** gateway authentication/session cookies, adversarial prompt-injection tests, private-file boundaries, safer error handling, audit rotation design.
- **Tool conformance:** positive and negative tests for every tool; WebSearch results; successful public HTTPS WebFetch; valid child-originated SubagentHandback.
- **Extensions:** real MCP/LSP integrations; a well-defined optional TypeScript/Cordis host; plugin lifecycle and permission review.
- **Memory and orchestration:** advanced memory/ACP work, context efficiency, cancellations, parallel agent lifecycle, long-run stability.
- **Performance:** benchmark provider-independent paths, quantify RAM under controlled workloads, improve compile time on weak CPUs.
- **Docs and portability:** installation paths, Linux distribution coverage, generic service packaging, clear examples.
- **Hardware testing:** build/test on other x86-64 machines, systems with different kernels, and low-RAM systems.

## Pull request expectations

- Explain the problem, the chosen approach, and important alternatives.
- Include tests, outputs, and relevant platform details.
- Identify whether a test used a mocked model, a real provider, or no model.
- Do not include API keys, access tokens, private hostnames/IPs, private workspace paths, personal logs, databases, or release binaries.
- Keep each PR focused. Split large refactors into reviewable steps.
- Update docs/CONFORMANCE.md only when the named test was actually run and passed.
- Do not claim full Claude Code/Codex compatibility merely because a tool name exists in the catalog.

By sending a contribution, you agree that it is submitted under the repository's Apache-2.0 license, subject to that license's terms.
