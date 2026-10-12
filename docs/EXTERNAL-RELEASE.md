# Build artifacts and deployment

GitHub Actions is the recommended way to build the x86-64 baseline without exhausting a low-resource development machine. The workflow runs the locked Rust test suite, builds a release binary, validates scripts, checks the target CPU baseline, and uploads a short-lived build artifact.

## Build locally

~~~sh
cargo fmt --check
CARGO_BUILD_JOBS=1 cargo test --locked --all-targets
CARGO_BUILD_JOBS=1 cargo build --locked --release
~~~

Use one Cargo job on weak hardware. A release build may take many minutes. Do not copy a random local target directory into a live service without test gates, health checks and a rollback plan.

## Deployment script warning

The repository includes scripts/deploy.sh for the maintainer's original runit-based service layout. It is **not a universal installer** and is intentionally guarded. It exits unless HARNESS_ALLOW_MAINTAINER_DEPLOY=1 is explicitly present in the environment. Review the entire script before opting in; do not run it on a contributor machine or production host merely because it exists in the repository.

If you intentionally adapt it for a reviewed maintainer deployment, set HARNESS_DEPLOY_WORKSPACE to the intended workspace path and check every service/data path before enabling the guard. Do not assume these values match your host.

For a new setup, first run the binary with a disposable HARNESS_HOME, HARNESS_DATA and HARNESS_WORKSPACE, bind the API to 127.0.0.1, keep execution mode at ask, configure a provider privately, and verify the health endpoint and mocked test suite. Configure a system service only after security and route exposure have been reviewed.

The gateway authentication/session-cookie gap is still open. Do not expose the execution API on the public internet or through an unauthenticated tunnel. Mobile clients and external bridges require separate authentication and security review.

## Current limitation

This repository does not include a production provider credential or a pre-approved production deployment target. Real-provider E2E tests require an explicit disposable staging URL and private credentials; never point them at production.
