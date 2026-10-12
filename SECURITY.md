# Security policy

Harness-RS executes tools and processes on behalf of a user. Treat vulnerabilities involving command execution, permission decisions, workspace boundaries, provider credentials, remote exposure, hooks/plugins, MCP/LSP, and tool-result injection as high priority.

## Reporting a vulnerability

**Do not open a public issue for an exploitable vulnerability.** Use GitHub's private vulnerability reporting / Security Advisories for this repository when enabled. If private reporting is not available yet, contact the repository owner through GitHub privately and share only the minimum information needed to reproduce the issue.

Please include the affected version or commit, platform/kernel, reproduction steps, expected and actual behavior, and any relevant sanitized logs. Do not attach provider keys, live access tokens, production database files, audit logs with session metadata, or private network addresses.

## Security boundaries and limitations

- Keep the HTTP execution gateway bound to loopback or a trusted private network. Gateway login/session-cookie authentication is still an open task.
- The approval and policy layers are not substitutes for running with least privilege.
- Linux Landlock/seccomp setup is designed to fail closed. Do not add an unsandboxed fallback to bypass an unavailable kernel feature.
- Tool output is treated as untrusted data and passed through credential redaction; neither this nor pattern-based secret detection is perfect.
- Hash-chained audit logs are tamper-evident when validated but are not signed against an attacker who can rewrite the whole file.
- MCP/LSP server executables and skills/hooks may be untrusted code. Review their origin, configuration, permissions, and update process.
- Do not claim production readiness or complete tool parity while the gaps in docs/PROJECT_STATUS.md remain open.

## Credential hygiene

Use private environment variables or a private provider configuration. Do not put credentials in Git, issue reports, screenshots, examples, CI logs, test fixtures that are intended to resemble live credentials, or shell command history. Synthetic test tokens must be clearly labeled as fake.
