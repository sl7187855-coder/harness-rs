## What changed

## Why

## Tests performed

Include commands and results. Clearly label tests as unit, deterministic mock, staging real-provider, or manual UI tests.

## Security and resource considerations

- [ ] No credentials, private IPs/URLs, local databases, logs or binaries added
- [ ] Permission and sandbox checks were preserved
- [ ] New side effects have explicit policy/approval coverage
- [ ] Tool/schema changes have tests and docs
- [ ] Known gaps are documented; compatibility is not overclaimed

## Checklist

- [ ] cargo fmt --check
- [ ] cargo test --locked --all-targets
- [ ] Relevant script/HUD checks
- [ ] Docs updated
