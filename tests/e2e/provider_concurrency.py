"""Six concurrent real-provider turns against an explicitly selected staging API."""
import concurrent.futures
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid

BASE = os.environ.get("HARNESS_E2E_BASE_URL", "").rstrip("/")
if not BASE:
    raise SystemExit("Set HARNESS_E2E_BASE_URL to an isolated staging server; production is not an implicit target.")
TIMEOUT = 120


def request(path, payload=None, timeout=TIMEOUT):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    headers = {"Accept": "application/json", "Cache-Control": "no-cache"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(BASE + path, data=data, headers=headers,
                                 method="POST" if data is not None else "GET")
    with urllib.request.urlopen(req, timeout=timeout) as response:
        return response.read()


def get_json(path):
    return json.loads(request(path).decode("utf-8", "replace"))


def one_turn(index, profile_id, model):
    token = uuid.uuid4().hex
    session = f"harness-parallel-{index}-{token[:10]}"
    payload = {
        "message": "Synthetic reliability check. Do not call tools. Reply with exactly PARALLEL-HARNESS-OK.",
        "session_id": session,
        "history_id": session,
        "run_id": f"parallel-{token}",
        "provider": "openrouter",
        "api_id": profile_id,
        "model": model,
        "effort": "off",
        "preset": "standard",
        "memory": False,
        "history": [],
        "attachments": [],
        "max_steps": 3,
    }
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        BASE + "/api/dsh-chat", data=data,
        headers={"Content-Type": "application/json", "Accept": "text/event-stream"},
        method="POST",
    )
    events = []
    with urllib.request.urlopen(req, timeout=TIMEOUT) as response:
        for raw_line in response:
            line = raw_line.decode("utf-8", "replace").strip()
            if not line.startswith("data:"):
                continue
            try:
                event = json.loads(line[5:].strip())
            except json.JSONDecodeError:
                continue
            events.append(event)
    errors = [event for event in events if event.get("type") == "error"]
    credit_errors = [event for event in errors if event.get("code") == "PROVIDER_CREDITS"]
    if credit_errors:
        raise AssertionError(f"turn {index}: provider credits error")
    final = next((event for event in reversed(events) if event.get("type") == "final"), None)
    if final is None:
        detail = errors[-1].get("code", "missing final event") if errors else "missing final event"
        raise AssertionError(f"turn {index}: {detail}")
    text = final.get("text", "")
    if "PARALLEL-HARNESS-OK" not in text:
        raise AssertionError(f"turn {index}: unexpected model response")
    return index, True


def main():
    health = get_json("/api/health")
    if health.get("ok") is not True:
        raise AssertionError("staging health is not OK")
    apis = get_json("/api/apis").get("apis") or []
    active = next((profile for profile in apis if profile.get("active")), None)
    if not active or not active.get("has_api_key"):
        raise AssertionError("no active provider profile in staging")
    settings = get_json("/api/settings")
    if settings.get("thomasApiId") != active.get("id"):
        raise AssertionError("HUD profile and active provider profile differ")
    if settings.get("thomasProvider") != "openrouter":
        raise AssertionError("this staging test expects the already configured OpenRouter provider")
    model = settings.get("thomasModel") or settings.get("model")
    if not model:
        raise AssertionError("no selected model in staging settings")
    limit = settings.get("model_max_inflight", 2)
    try:
        limit = max(1, min(8, int(limit)))
    except (TypeError, ValueError):
        limit = 2

    errors = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        futures = [executor.submit(one_turn, index, active["id"], model) for index in range(1, 7)]
        pending = set(futures)
        observed_max = 0
        while pending:
            done, pending = concurrent.futures.wait(
                pending, timeout=0.15, return_when=concurrent.futures.FIRST_COMPLETED
            )
            for future in done:
                try:
                    index, _ = future.result()
                    print(f"PASS concurrent real-model turn {index}/6")
                except Exception as error:
                    errors.append(str(error))
            try:
                metrics = get_json("/api/harness/metrics")
                observed_max = max(observed_max, int(metrics.get("model_inflight", 0)))
            except Exception:
                pass
        metrics = get_json("/api/harness/metrics")
    if errors:
        for error in errors:
            print(f"FAIL {error}")
        raise SystemExit(1)
    if int(metrics.get("model_inflight", -1)) != 0:
        raise AssertionError("model_inflight did not return to zero")
    if observed_max > limit:
        raise AssertionError(f"observed model_inflight={observed_max}, configured per-profile ceiling={limit}")
    print(f"PASS six concurrent real-model turns without PROVIDER_CREDITS; max model_inflight={observed_max}, ceiling={limit}")
    print("REAL_PARALLEL_E2E_RESULT=6_PASS")


if __name__ == "__main__":
    main()
