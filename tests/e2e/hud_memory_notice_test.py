#!/usr/bin/env python3
"""Exercise the actual memory-degraded notice function with a small DOM stub."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HUD = Path(__file__).resolve().parents[2] / "web" / "thomas-hud.html"
html = HUD.read_text()
match = re.search(
    r"function updateMemoryDegradedNotice\(health\)\s*\{.*?\n\}",
    html,
    flags=re.DOTALL,
)
if not match:
    raise SystemExit("FAIL: updateMemoryDegradedNotice not found in HUD")

test = r"""
const assert = require("node:assert/strict");
const elements = {
  "#memoryDegradedNotice": { hidden: false, dataset: {} },
  "#memoryDegradedDetail": { textContent: "" }
};
function $(selector) { return elements[selector] || null; }
""" + match.group(0) + r"""

updateMemoryDegradedNotice({ok:true,memory:{degraded:false}});
assert.equal(elements["#memoryDegradedNotice"].hidden, true);
assert.equal(elements["#memoryDegradedNotice"].dataset.state, "healthy");

updateMemoryDegradedNotice({ok:true,memory:{degraded:true,degraded_reason:"SQLite unavailable"}});
assert.equal(elements["#memoryDegradedNotice"].hidden, false);
assert.equal(elements["#memoryDegradedNotice"].dataset.state, "degraded");
assert.match(elements["#memoryDegradedDetail"].textContent, /SQLite unavailable/);

updateMemoryDegradedNotice({ok:true,memory:{error:"database locked"}});
assert.equal(elements["#memoryDegradedNotice"].hidden, false);
assert.match(elements["#memoryDegradedDetail"].textContent, /database locked/);

updateMemoryDegradedNotice({ok:true,memory:{degraded:true,degraded_reason:"x".repeat(400)}});
assert.ok(elements["#memoryDegradedDetail"].textContent.length <= 240);
console.log("HUD_MEMORY_NOTICE_RESULT=4_PASS");
"""
with tempfile.NamedTemporaryFile("w", suffix=".cjs", encoding="utf-8", delete=False) as file:
    path = Path(file.name)
    file.write(test)
result = subprocess.run(["node", str(path)], text=True, capture_output=True)
path.unlink(missing_ok=True)
if result.returncode:
    sys.stderr.write(result.stdout)
    sys.stderr.write(result.stderr)
    raise SystemExit(result.returncode)
print(result.stdout.strip())
