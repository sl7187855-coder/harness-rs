#!/usr/bin/env bash
set -Eeuo pipefail

if ! printenv HARNESS_ALLOW_MAINTAINER_DEPLOY | grep -qx 1; then
  echo "Maintainer-host deployment script; review it and set HARNESS_ALLOW_MAINTAINER_DEPLOY=1 only when intentionally deploying to that host." >&2
  exit 64
fi

ROOT=/opt/harness-rs
LIVE=/etc/service/harness-rs
DATA=/var/lib/harness-rs/data
WORKSPACE=${HARNESS_DEPLOY_WORKSPACE:-/var/lib/harness-rs/workspace}
if [ "$#" -gt 0 ]; then VERSION="$1"; else VERSION=$(date -u +%Y%m%dT%H%M%SZ); fi
# This Atom is the deployment/build host. Only the exact local release artifact
# is accepted from target/; debug and other local build products remain forbidden.
if [ "$#" -gt 1 ]; then BINARY="$2"; else BINARY=""; fi
STAGE="/var/lib/harness-rs/stage/$VERSION"
RELEASE="$ROOT/releases/$VERSION"
PORT=1357
BRIDGE_PORT=1358
STAGE_LOG="/var/log/harness-rs/stage-$VERSION.log"
BASE="http://127.0.0.1:$PORT"
OLD_RUN=$(mktemp)
OLD_CURRENT=$(readlink "$ROOT/current" 2>/dev/null || true)
STAGE_PID=
CHANGED_RUN=0

cleanup() {
  status=$?
  if [ -n "$STAGE_PID" ]; then
    kill "$STAGE_PID" 2>/dev/null || true
    wait "$STAGE_PID" 2>/dev/null || true
  fi
  # Remove copied OpenRouter credentials even if a staging assertion fails.
  if [ -n "$STAGE" ]; then rm -f -- "$STAGE/data/providers.json"; fi
  if [ "$status" -ne 0 ] && [ "$CHANGED_RUN" -eq 1 ]; then
    echo "Deployment failed; restoring previous run script and current target" >&2
    cp -p "$OLD_RUN" "$LIVE/run"
    if [ -n "$OLD_CURRENT" ]; then ln -sfn "$OLD_CURRENT" "$ROOT/current"; else rm -f "$ROOT/current"; fi
    sv restart "$LIVE" || true
    for _ in $(seq 1 20); do
      curl -fsS --max-time 2 http://127.0.0.1:1347/api/health >/dev/null && break || sleep 1
    done
  fi
  # A successful deploy no longer needs the private staging DB or copied
  # provider credentials. Keep failed stages locked down for diagnosis.
  if [ "$status" -eq 0 ]; then
    rm -rf -- "$STAGE"
    # Stage-only acceptance must leave no duplicate release or private staging data.
    if [ "${HARNESS_DEPLOY_STAGE_ONLY:-0}" = "1" ]; then rm -rf -- "$RELEASE"; fi
  fi
  rm -f "$OLD_RUN"
}
trap cleanup EXIT

[ "$EUID" -eq 0 ] || { echo "must run as root" >&2; exit 2; }
[ -n "$BINARY" ] || BINARY="$ROOT/target/release/harness-rs"
BINARY=$(readlink -f "$BINARY")
case "$BINARY" in
  "$ROOT/target/release/harness-rs")
    [ -f "$BINARY" ] && [ -x "$BINARY" ] || { echo "local release binary missing or not executable: $BINARY" >&2; exit 2; }
    echo "PASS artifact gate: exact local release binary on Atom"
    ;;
  "$ROOT"/target/*)
    echo "refusing debug or non-release local target artifact: $BINARY" >&2; exit 2;;
  *)
    [ -f "$BINARY" ] && [ -x "$BINARY" ] || { echo "release binary not found or not executable: $BINARY" >&2; exit 2; }
    ;;
esac
case "$VERSION" in *[!A-Za-z0-9._-]*|'') echo "invalid version string" >&2; exit 2;; esac
[ "$VERSION" != "." ] && [ "$VERSION" != ".." ] || { echo "invalid version string" >&2; exit 2; }
[ "$(printf %s "$VERSION" | wc -c)" -le 80 ] || { echo "invalid version string" >&2; exit 2; }
[ -f "$DATA/providers.json" ] && [ -f "$DATA/settings.json" ] || { echo "provider profile/settings missing" >&2; exit 2; }
[ ! -e "$RELEASE" ] || { echo "release already exists: $RELEASE" >&2; exit 2; }

# Deployment is forbidden unless the latest source and checked-in schema fixture pass.
echo "== Full Rust test gate (one Cargo job) =="
if ! (cd "$ROOT" && CARGO_BUILD_JOBS=1 timeout 1800 cargo test --locked --all-targets) > /tmp/harness-release-test-gate.log 2>&1; then
  tail -n 220 /tmp/harness-release-test-gate.log >&2
  echo "test gate failed; the current production service and release pointer remain unchanged" >&2
  exit 1
fi
tail -n 45 /tmp/harness-release-test-gate.log

timeout 180 tar czf "/root/harness-rs-backup-predeploy-$VERSION.tar.gz" --exclude=target -C /opt harness-rs
install -d -o root -g root -m 755 "$ROOT/releases" "$RELEASE"
install -o root -g root -m 755 "$BINARY" "$RELEASE/harness-rs"
install -d -o nobody -g nogroup -m 700 "$STAGE/data" "$STAGE/data/mobile" "$STAGE/home"
install -o nobody -g nogroup -m 600 "$DATA/providers.json" "$STAGE/data/providers.json"
install -o nobody -g nogroup -m 600 "$DATA/settings.json" "$STAGE/data/settings.json"
if [ -f "$DATA/plugins-enabled.json" ]; then install -o nobody -g nogroup -m 600 "$DATA/plugins-enabled.json" "$STAGE/data/plugins-enabled.json"; fi
if [ -f "$DATA/mobile/ThomasMobileAgent.apk" ]; then install -o nobody -g nogroup -m 600 "$DATA/mobile/ThomasMobileAgent.apk" "$STAGE/data/mobile/ThomasMobileAgent.apk"; fi
chown nobody:nogroup "$STAGE"
chmod 700 "$STAGE"

# Use a fresh staging database. Never copy the production SQLite/WAL into this test instance.
export HARNESS_HOME="$STAGE/home"
export HARNESS_DATA="$STAGE/data"
export HARNESS_WORKSPACE="$WORKSPACE"
export HARNESS_SKILLS=/var/lib/harness-rs/skills
export HARNESS_PLUGINS=/var/lib/harness-rs/plugins
export HARNESS_HUD_FILE="$ROOT/web/thomas-hud.html"
export HARNESS_BIND="127.0.0.1:$PORT"
export HARNESS_BRIDGE_BIND="127.0.0.1:$BRIDGE_PORT"
export HARNESS_MAX_STEPS=20
export HARNESS_MAX_PARALLEL_TOOLS=1
export RUST_LOG=info

: > "$STAGE_LOG"
chown nobody:nogroup "$STAGE_LOG"
chmod 600 "$STAGE_LOG"
setpriv --reuid=nobody --regid=nogroup --clear-groups env \
  HARNESS_HOME="$HARNESS_HOME" HARNESS_DATA="$HARNESS_DATA" HARNESS_WORKSPACE="$HARNESS_WORKSPACE" \
  HARNESS_SKILLS="$HARNESS_SKILLS" HARNESS_PLUGINS="$HARNESS_PLUGINS" HARNESS_HUD_FILE="$HARNESS_HUD_FILE" \
  HARNESS_BIND="$HARNESS_BIND" HARNESS_BRIDGE_BIND="$HARNESS_BRIDGE_BIND" \
  HARNESS_MAX_STEPS=20 HARNESS_MAX_PARALLEL_TOOLS=1 RUST_LOG=info \
  "$RELEASE/harness-rs" >>"$STAGE_LOG" 2>&1 &
STAGE_PID=$!
ready=0
for _ in $(seq 1 20); do
  if curl -fsS --max-time 2 "$BASE/api/health" >/tmp/harness-deploy-health.json; then ready=1; break; fi
  if ! kill -0 "$STAGE_PID" 2>/dev/null; then cat "$STAGE_LOG" >&2; exit 1; fi
  sleep 1
done
[ "$ready" -eq 1 ] || { cat "$STAGE_LOG" >&2; echo "staging health never became ready" >&2; exit 1; }
python3 - <<'PY'
import json
d=json.load(open('/tmp/harness-deploy-health.json'))
assert d.get('ok') is True
assert d.get('version') == '0.1.0'
assert d.get('tool_count') == 50, d
assert d.get('tool_ready') == 48 and d.get('tool_deferred') == 2, d
assert d.get('ram',{}).get('within_budget') is True, d
assert d.get('old_dsh_disabled') is True, d
assert isinstance(d.get('memory',{}).get('degraded'), bool), d
audit=d.get('audit',{})
assert audit.get('healthy') is True, d
assert isinstance(audit.get('records'), int) and audit['records'] >= 0, d
print('PASS stage health: version, 50 tools, RAM, memory/audit status, legacy services')
PY
python3 "$ROOT/tests/e2e/hud_memory_notice_test.py"
HARNESS_E2E_BASE_URL="$BASE" python3 "$ROOT/tests/e2e/run.py"
HARNESS_E2E_BASE_URL="$BASE" python3 "$ROOT/tests/e2e/provider_concurrency.py"
HARNESS_E2E_BASE_URL="$BASE" python3 "$ROOT/tests/e2e/routes.py"
HARNESS_BINARY="$RELEASE/harness-rs" python3 "$ROOT/tests/e2e/mock_api_test.py"

echo "== Hash-chained audit acceptance =="
curl -fsS --max-time 3 "$BASE/api/health" >/tmp/harness-stage-post-e2e-health.json
python3 - "$STAGE/data/audit.log" /tmp/harness-stage-post-e2e-health.json <<'PY'
import hashlib
import json
import pathlib
import stat
import sys

path = pathlib.Path(sys.argv[1])
health = json.loads(pathlib.Path(sys.argv[2]).read_text())
audit = health.get("audit", {})
assert audit.get("healthy") is True, health
assert isinstance(audit.get("records"), int) and audit["records"] > 0, health
assert path.is_file() and not path.is_symlink(), "audit log missing or symlink"
assert stat.S_IMODE(path.stat().st_mode) == 0o600, "audit log mode is not 0600"
lines = path.read_text(encoding="utf-8").splitlines()
assert len(lines) == audit["records"], (len(lines), audit)
field_order = (
    "sequence", "timestamp_unix_ms", "event_id", "session_id", "run_id",
    "tool", "phase", "outcome", "error_code", "previous_hash",
)
previous = "0" * 64
for sequence, line in enumerate(lines, 1):
    record = json.loads(line)
    assert record["sequence"] == sequence, (sequence, record.get("sequence"))
    assert record["previous_hash"] == previous, sequence
    unsigned = {key: record[key] for key in field_order}
    canonical = json.dumps(unsigned, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    expected = hashlib.sha256(canonical).hexdigest()
    assert record.get("hash") == expected, sequence
    previous = expected
assert audit.get("last_hash") == previous, (audit.get("last_hash"), previous)
print(f"PASS audit acceptance: {len(lines)} records, hash chain intact, mode 0600, path audit.log")
PY

file "$RELEASE/harness-rs" | tee "$STAGE/release-file.txt"
# The generic x86-64 baseline is mandatory. Some dependencies (notably memchr)
# contain AVX2 routines behind runtime CPUID dispatch, so scanning for any AVX
# opcode is a false positive. The release is then exercised on this target CPU
# during staging; this Atom host reports no AVX flag.
grep -Fq 'target-cpu=x86-64' "$ROOT/.cargo/config.toml" || {
  echo "generic x86-64 target flag missing; refusing deploy" >&2; exit 1;
}
if ! grep -qm1 '^flags.* avx ' /proc/cpuinfo; then
  echo "PASS CPU baseline: staging runs on a host without AVX"
else
  echo "NOTE CPU baseline: host has AVX; runtime-dispatch safety needs a separate no-AVX smoke host"
fi

if [ "${HARNESS_DEPLOY_STAGE_ONLY:-0}" = "1" ]; then
  echo "STAGE_ONLY_OK version=$VERSION; production pointer and service were not changed"
  exit 0
fi

# The legacy Python HUD and Node DSH bridge used to bind the exact ports now
# owned by Harness-RS. Refuse promotion unless these replacement targets stay
# down, otherwise runit may race the Rust service and win the bind on restart.
for legacy in thomas-hud thomas-dsh-warm-bridge thomas-dsh-web; do
  status=$(sv status "/etc/service/$legacy" 2>&1 || true)
  case "$status" in
    down:*) echo "PASS legacy service is down: $legacy" ;;
    *) echo "refusing promotion: conflicting legacy service is not down: $legacy ($status)" >&2; exit 1 ;;
  esac
done

cp -p "$LIVE/run" "$OLD_RUN"
CHANGED_RUN=1
ln -sfn "$RELEASE" "$ROOT/current"
tmp_run=$(mktemp)
python3 - "$LIVE/run" "$tmp_run" <<'PY'
import pathlib,sys
source=pathlib.Path(sys.argv[1]).read_text()
debug_path='target/debug/harness-rs'
current_path='current/harness-rs'
debug_count=source.count(debug_path)
current_count=source.count(current_path)
assert debug_count + current_count == 1, "expected exactly one recognized Harness-RS binary path in the live runner"
if debug_count:
    source=source.replace(debug_path, current_path)
mode_line='export HARNESS_EXECUTION_MODE=ask'
assert source.count(mode_line) <= 1, "unexpected duplicate hard-coded ask mode in the live runner"
source='\n'.join(line for line in source.splitlines() if line.strip()!=mode_line)+'\n'
pathlib.Path(sys.argv[2]).write_text(source)
PY
chown root:root "$tmp_run"
chmod 755 "$tmp_run"
cat "$tmp_run" >"$LIVE/run"
rm -f "$tmp_run"
sv restart "$LIVE"

healthy=0
for _ in $(seq 1 20); do
  if curl -fsS --max-time 2 http://127.0.0.1:1347/api/health >/tmp/harness-live-health.json; then
    if python3 - <<'PY'
import json
d=json.load(open('/tmp/harness-live-health.json'))
assert d.get('ok') is True and d.get('build') == 'release', f"unexpected health service/build; keys={sorted(d)} service={d.get('service')!r} build={d.get('build')!r}"
assert d.get('tool_count') == 50 and d.get('tool_ready') == 48 and d.get('tool_deferred') == 2, f"tool catalogue mismatch: {d.get('tool_count')}/{d.get('tool_ready')}/{d.get('tool_deferred')}"
assert d.get('ram',{}).get('within_budget') is True, f"RAM budget gate failed: {d.get('ram')}"
assert d.get('old_dsh_disabled') is True, "legacy DSH status gate failed"
assert isinstance(d.get('memory',{}).get('degraded'), bool), "memory degradation field missing"
PY
    then healthy=1; break; fi
  fi
  sleep 1
done
[ "$healthy" -eq 1 ] || {
  echo "live health failed; current HTTP response and port owners follow (rollback will run)" >&2
  curl -sS --max-time 2 http://127.0.0.1:1347/api/health 2>/dev/null | python3 -c 'import json,sys; d=json.load(sys.stdin); print({"keys":sorted(d),"service":d.get("service"),"build":d.get("build"),"version":d.get("version")})' >&2 || true
  ss -ltnp 2>/dev/null | grep -E ':(1347|3196)\b' >&2 || true
  exit 1
}
echo "DEPLOY_OK version=$VERSION live=:1347 stage_port=$PORT"
