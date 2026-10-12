#!/usr/bin/env python3
"""Credential-free route and boundary smoke tests for the local Harness API."""
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

BASE = os.environ.get("HARNESS_E2E_BASE_URL", "http://127.0.0.1:1347").rstrip("/")


def fetch(path, data=None, method=None, timeout=12):
    body = None if data is None else json.dumps(data).encode()
    headers = {"Cache-Control": "no-cache"}
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(BASE + path, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, response.headers.get("Content-Type", ""), response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers.get("Content-Type", ""), error.read()


def check(label, ok):
    if not ok:
        raise AssertionError(label)
    print("PASS", label)


def main():
    status, _, raw = fetch("/api/health")
    health = json.loads(raw)
    check("health reports release API and memory health fields", status == 200 and health.get("ok") is True and "degraded" in health.get("memory", {}))
    check("health reports a healthy audit chain", status == 200 and health.get("audit", {}).get("healthy") is True and isinstance(health.get("audit", {}).get("records"), int))

    status, _, raw = fetch("/api/harness/context")
    context = json.loads(raw)
    check("context metrics route responds", status == 200 and context.get("ok") is True and "sections" in context)

    # /rewind is a local command and must not invoke the configured model.
    status, _, raw = fetch("/api/harness/metrics")
    rewind_metrics_before = json.loads(raw).get("model_usage")
    status, _, raw = fetch("/api/health")
    rewind_audit_before = json.loads(raw).get("audit", {}).get("records")
    status, content_type, raw = fetch(
        "/api/dsh-chat",
        {"message": "/rewind list", "session_id": "rewind-route-smoke", "history_id": "rewind-route-smoke", "run_id": "rewind-route-smoke"},
        timeout=8,
    )
    body = raw.decode("utf-8", errors="replace")
    check(
        "/rewind list returns a completed SSE command response without model output",
        status == 200 and "text/event-stream" in content_type.lower()
        and "rewind_command" in body and "turn_end" in body
        and ("snapshot" in body.lower() or "snapshots" in body.lower()),
    )
    status, _, raw = fetch("/api/harness/metrics")
    rewind_metrics_after = json.loads(raw).get("model_usage")
    check(
        "/rewind list does not increase model usage",
        status == 200 and rewind_metrics_before == rewind_metrics_after,
    )
    status, _, raw = fetch("/api/health")
    rewind_audit_after = json.loads(raw).get("audit", {}).get("records")
    check(
        "/rewind list writes a start and completion to the audit chain",
        status == 200 and rewind_audit_before is not None and rewind_audit_after == rewind_audit_before + 2,
    )

    status, _, raw = fetch("/api/settings")
    before_settings = json.loads(raw)
    before_mode = before_settings.get("execution_mode")
    status, _, raw = fetch("/api/settings", {"execution_mode": "unsafe-invalid-mode"}, "POST")
    check("settings API rejects an invalid execution mode", status == 400 and b"execution_mode" in raw)
    status, _, raw = fetch("/api/settings")
    after_settings = json.loads(raw)
    check("invalid execution mode leaves persisted settings unchanged", status == 200 and after_settings.get("execution_mode") == before_mode)

    status, _, raw = fetch("/api/dsh-status?session_id=route-smoke-missing&run_id=route-smoke-missing")
    snapshot = json.loads(raw)
    check("dsh-status route resolves an unknown run without leaking contents", status == 200 and snapshot.get("active") is False)

    status, _, raw = fetch("/api/mobile/frame")
    check("mobile frame rejects missing device id", status == 400 and b"deviceId" in raw)

    for route, label in (
        ("/api/research-image?url=" + urllib.parse.quote("http://127.0.0.1/"), "image research blocks loopback SSRF"),
        ("/api/research-media?url=" + urllib.parse.quote("http://127.0.0.1/"), "media research blocks loopback SSRF"),
    ):
        status, _, raw = fetch(route)
        check(label, status == 502 and b"bloqueado" in raw)

    hostile = "/api/workspace/terminal/raw/" + urllib.parse.quote("../etc/passwd", safe="")
    status, _, raw = fetch(hostile)
    check("terminal raw blocks workspace traversal", status in (400, 403, 404) and b"root:" not in raw)

    status, _, raw = fetch("/api/browser/frame")
    check("browser frame route is implemented (upstream may be offline)", status != 404)
    status, _, raw = fetch("/frame")
    check("direct /frame alias is implemented (upstream may be offline)", status != 404)

    status, _, raw = fetch("/api/voice/config")
    check("voice config GET route is implemented (upstream may be offline)", status != 404)
    status, _, raw = fetch("/api/voice/voices")
    check("voice list route is implemented (upstream may be offline)", status != 404)
    status, _, raw = fetch("/voice")
    check("voice root route is implemented (upstream may be offline)", status != 404)
    status, _, raw = fetch("/voice/api/voice/config")
    check("voice path proxy is implemented (upstream may be offline)", status != 404)

    # Use oversized requests: handlers must reject them before forwarding,
    # so a real voice backend cannot have its settings changed by this smoke test.
    status, _, raw = fetch("/api/voice/config", {"probe": "x" * (2 * 1024 * 1024)}, "POST")
    check("voice config POST is rejected before upstream side effects", status == 413)
    status, _, raw = fetch("/api/voice/speak", {"text": "x" * (2 * 1024 * 1024)}, "POST")
    check("voice speak POST is rejected before upstream side effects", status == 413)

    status, content_type, raw = fetch("/api/mobile/apk")
    if status == 404:
        check("mobile APK route reports its explicit missing-artifact result", b"APK" in raw)
    else:
        check("mobile APK route responds", status == 200 and "application/vnd.android.package-archive" in content_type.lower() and raw.startswith(b"PK"))

    status, _, raw = fetch("/api/research-images?query=")
    check("research image-search route validates input before network access", status == 400 and b"query" in raw)
    status, _, raw = fetch("/api/research-media?url=" + urllib.parse.quote("http://127.0.0.1/"))
    check("research media route blocks loopback SSRF before network access", status == 502 and b"bloqueado" in raw)

    print("ROUTE_RESULT=25_CHECKS_PASS")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"FAIL {type(error).__name__}: {error}", file=sys.stderr)
        sys.exit(1)
