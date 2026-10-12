# Test and conformance status

This file records test evidence and known limitations. Catalog state alone is never treated as proof of semantic conformance.

## Source snapshot results

The latest recorded source-tree run before this public packaging step reported:

- Rust: 60 unit tests and 3 schema-conformance tests passed.
- Formatting and static script checks passed.
- Deterministic mock provider/tool suite: 33 tool probes and 85 end-to-end checks passed.
- A separate isolated staging run recorded 6 sequential real-model turns and 6 concurrent real-model turns, with model concurrency bounded at 2.
- Audit-chain tests cover integrity validation, tampering detection, concurrent appends and safe failure when audit storage is unhealthy.
- Adversarial local tool-output tests cover credential redaction, explicit untrusted-data marking, and refusal to treat malicious file content as policy.

The maintainer machine's exact deployment paths, provider profile IDs, private URLs, staging logs, audit records and binary hashes are intentionally excluded. The first GitHub CI run should establish fresh evidence for the public commit.

## Tool catalogue

There are 50 unique tool names: 48 are currently marked available for dispatch, and 2 are deferred. The catalog contains:

- Files: Read, Write, Edit, NotebookEdit, Glob, Grep.
- Execution: Bash, PowerShell (deferred on Linux), Monitor.
- Agents: Agent, SendMessage, ListAgents, SubagentHandback, Workflow.
- Tasks: TaskCreate, TaskGet, TaskList, TaskUpdate, TaskOutput, TaskStop, TodoWrite.
- Planning/worktrees: EnterPlanMode, ExitPlanMode, EnterWorktree, ExitWorktree.
- Schedules: CronCreate, CronDelete, CronList, ScheduleWakeup, RemoteTrigger (deferred until an authenticated connector exists).
- Web: WebFetch, WebSearch.
- Extensions/MCP: Skill, ToolSearch, ListMcpResourcesTool, ReadMcpResourceTool, WaitForMcpServers.
- Code: LSP, ApplyPatch.
- Interaction: AskUserQuestion, PushNotification, SendUserFile, Artifact, ReportFindings, SendFeedback, ShareOnboardingGuide, EndConversation.
- Memory: MemorySearch, MemorySave, MemoryForget.

## Independently tracked gaps

- WebSearch: dispatch was observed, but positive useful results were not validated in the tested environment.
- WebFetch: unsafe targets are rejected; successful public HTTPS content needs a separate positive test.
- SubagentHandback: invalid parent-context handback is rejected; a valid child-originated handback needs positive coverage.
- MCP/LSP: local mocks pass, but no production server integration is bundled or assumed.
- Gateway authentication/session cookies remain open.
- Per-tool conformance is incomplete; tool availability is not the same as proven compatibility with another product's complete schema or behavior.
- Long-run 200-turn/40-call and 8-hour soak tests remain open.
- PowerShell and RemoteTrigger remain deferred.

## How to update this file

Only change an item to passed when an automated test actually runs and succeeds. Link the named test, report the command and environment, and distinguish synthetic-provider coverage from a real-provider staging test. Never add raw secrets or private deployment paths to test evidence.


## Thomas Mobile Agent 0.5.3 package acceptance — 2026-10-12

- Android Gradle build: ./gradlew --no-daemon --max-workers=1 assembleDebug passed using Java 21 on the build machine.
- APK manifest verified: package com.thomas.mobileagent, versionName 0.5.3, versionCode 12.
- Android APK signature verified with the SDK's apksigner. The artifact is debug-signed and is not claimed to be a Play Store/production-signed release.
- APK SHA-256: 625a5a030981a61ca4a23503c3741b9e5cd0d2399950beeb1ea2e3a02af3333b.
- Privacy scan of the APK confirmed the old transient development URL, baked-in relay IP, predictable example token and developer source path are absent. A real server address, username, provider API key, relay token, TLS certificate or password is not embedded.
- Relay, public TLS gateway, MCP adapter and interactive auth setup passed node --check; npm ci installed the locked dependencies. A full on-device install, authenticated connection and action-by-action Android conformance session is still pending because the maintainer handset app was uninstalled. Do not mark mobile remote actions independently verified based on the APK build alone.
