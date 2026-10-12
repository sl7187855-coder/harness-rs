#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export CARGO_BUILD_JOBS=1

printf '%s\n' '== Rust formatting =='
if command -v rustfmt >/dev/null 2>&1; then cargo fmt --check; else printf '%s\n' 'SKIP: rustfmt is not installed on this host'; fi

printf '%s\n' '== Full Rust unit/integration/schema tests =='
cargo test --all-targets

printf '%s\n' '== Deterministic provider/tool E2E =='
if printenv HARNESS_BINARY >/dev/null 2>&1; then
  python3 tests/e2e/mock_api_test.py
else
  printf '%s\n' 'SKIP: set HARNESS_BINARY to a built executable to run isolated API/tool-call E2E'
fi

if [ "$(printenv HARNESS_RUN_REAL_E2E 2>/dev/null || true)" = "1" ]; then
  printf '%s\n' '== Real-provider E2E against an explicit staging URL =='
  : "${HARNESS_E2E_BASE_URL:?set HARNESS_E2E_BASE_URL to a disposable staging server, never production}"
  python3 tests/e2e/run.py
  python3 tests/e2e/provider_concurrency.py
  python3 tests/e2e/routes.py
fi
