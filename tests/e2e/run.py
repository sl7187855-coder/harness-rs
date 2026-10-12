#!/usr/bin/env python3
"""Sequential, credential-safe smoke tests for the live Harness-RS loopback API.

No API key is read, printed, persisted or passed in the request body. The real
model is selected from the HUD settings and sent using the configured active
provider profile. Credentials are neither printed nor sent in the request body.
"""
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

BASE = os.environ.get("HARNESS_E2E_BASE_URL", "http://127.0.0.1:1347").rstrip("/")
TIMEOUT = 20


def request(path, data=None, method=None, timeout=TIMEOUT):
    body = None if data is None else json.dumps(data).encode("utf-8")
    headers = {"Accept": "application/json", "Cache-Control": "no-cache"}
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(
        BASE + path, data=body, headers=headers, method=method
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, response.headers, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def json_request(path, data=None, method=None, timeout=TIMEOUT):
    status, _, raw = request(path, data=data, method=method, timeout=timeout)
    try:
        result = json.loads(raw.decode("utf-8", "replace"))
    except json.JSONDecodeError as error:
        raise AssertionError(f"{path}: expected JSON (HTTP {status})") from error
    if status >= 400:
        raise AssertionError(f"{path}: HTTP {status}, API returned an error")
    return result


def passed(label):
    print(f"PASS {label}")


def main():
    health = json_request("/api/health")
    assert health.get("ok") is True and health.get("service") == "harness-rs"
    passed("health API responds")

    catalog = json_request("/api/harness/tools")
    tools = catalog.get("tools") or []
    names = [tool.get("name") for tool in tools]
    assert len(names) == len(set(names))
    assert {"Read", "Edit", "Glob", "Grep", "Bash", "ToolSearch",
            "MemorySearch", "MemorySave", "MemoryForget", "ApplyPatch"} <= set(names)
    assert len(names) == 50, f"expected 50 catalogue entries, received {len(names)}"
    passed("50 unique tools and four Harness extensions are discoverable")

    apis = json_request("/api/apis").get("apis") or []
    active = next((profile for profile in apis if profile.get("active")), None)
    assert active and active.get("has_api_key"), "no active API profile is configured"
    assert active.get("provider") == "openrouter"
    assert "api_key" not in active and "key" not in active
    passed("active OpenRouter profile is configured (credential masked)")

    settings = json_request("/api/settings")
    assert settings.get("thomasApiId") == active.get("id")
    assert settings.get("thomasProvider") == "openrouter"
    selected_model = settings.get("thomasModel") or settings.get("model")
    assert selected_model == "nvidia/nemotron-3-super-120b-a12b:free", f"unexpected HUD model selection: {selected_model!r}"
    passed("HUD selects the NVIDIA Nemotron free model with the active OpenRouter profile")

    marker = "harness-e2e-" + uuid.uuid4().hex
    saved = json_request("/api/memory", {
        "text": f"temporary {marker} integration memory",
        "kind": "test",
        "scope": "global",
        "importance": 0.1,
        "confidence": 1.0,
    }, method="POST")
    memory_id = saved.get("id")
    assert isinstance(memory_id, int), "memory save returned no id"
    try:
        # Search on the unique alphanumeric suffix so FTS5 punctuation tokenization
        # does not make this integration test provider/SQLite-version dependent.
        query = urllib.parse.urlencode({"q": marker.rsplit("-", 1)[-1], "scope": "global", "limit": 5})
        found = json_request("/api/memory?" + query)
        assert any(item.get("id") == memory_id for item in found.get("memories", []))
    finally:
        deleted = json_request(f"/api/memory/{memory_id}", method="DELETE")
        assert deleted.get("removed") is True
    passed("memory save, FTS search and deletion lifecycle works")

    status, headers, hud = request("/")
    assert status == 200 and "text/html" in headers.get("Content-Type", "")
    assert len(hud) > 10000
    passed("HUD HTML is served locally")

    prompt = (
        'This is a synthetic smoke test. Invoke ToolSearch exactly once with query '
        '"memory tools" and limit 5, then reply exactly HARNESSTEST-OK.'
    )
    payload = {
        "message": prompt,
        "session_id": "harness-e2e-" + uuid.uuid4().hex[:12],
        "history_id": "harness-e2e-" + uuid.uuid4().hex[:12],
        "run_id": "e2e-" + uuid.uuid4().hex,
        "provider": "openrouter",
        "api_id": active["id"],
        "model": selected_model,
        "effort": "off",
        "preset": "standard",
        "memory": False,
        "history": [],
        "attachments": [],
    }
    body = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        BASE + "/api/dsh-chat",
        data=body,
        headers={"Content-Type": "application/json", "Accept": "text/event-stream"},
        method="POST",
    )
    events = []
    with urllib.request.urlopen(req, timeout=90) as response:
        assert response.status == 200
        for raw_line in response:
            line = raw_line.decode("utf-8", "replace").strip()
            if not line.startswith("data:"):
                continue
            try:
                event = json.loads(line[5:].strip())
            except json.JSONDecodeError:
                continue
            events.append(event)
    event_types = {event.get("type") for event in events}
    header = next((event for event in events if event.get("type") == "request_header"), {})
    assert header.get("provider") == "openrouter" and header.get("model") == selected_model, "runtime did not use the exact HUD-selected provider/model"
    assert "tool_call" in event_types and "tool_result" in event_types, "model did not exercise tool call"
    assert "final" in event_types, "model did not finish the turn"
    final = next(event for event in reversed(events) if event.get("type") == "final")
    assert "HARNESSTEST-OK" in str(final.get("text", "")), "model final response did not match smoke token"
    passed("real NVIDIA free model, exact provider/model selection, SSE and ToolSearch call succeed")

    print("E2E_RESULT=6_PASS")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"FAIL {type(error).__name__}: {error}", file=sys.stderr)
        sys.exit(1)
