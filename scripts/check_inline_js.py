#!/usr/bin/env python3
"""Syntax-check inline JavaScript in a shipped HTML file using Node.js."""
from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check_inline_js.py PATH.html", file=sys.stderr)
        return 2
    node = shutil.which("node")
    if not node:
        print("Node.js is required for this check.", file=sys.stderr)
        return 2

    html_path = Path(sys.argv[1])
    try:
        source = html_path.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"cannot read {html_path}: {exc}", file=sys.stderr)
        return 2

    scripts = re.findall(
        r"<script\b(?![^>]*\bsrc\s*=)[^>]*>(.*?)</script\s*>",
        source,
        flags=re.IGNORECASE | re.DOTALL,
    )
    if not scripts:
        print(f"no inline scripts found in {html_path}", file=sys.stderr)
        return 1

    checked = 0
    with tempfile.TemporaryDirectory(prefix="harness-rs-js-check-") as tmp:
        temp_dir = Path(tmp)
        for index, script in enumerate(scripts, start=1):
            if not script.strip():
                continue
            candidate = temp_dir / f"inline-{index}.js"
            candidate.write_text(script, encoding="utf-8")
            result = subprocess.run(
                [node, "--check", str(candidate)],
                text=True,
                capture_output=True,
                check=False,
            )
            if result.returncode != 0:
                print(f"syntax error in inline script #{index} from {html_path}")
                if result.stderr:
                    print(result.stderr.rstrip(), file=sys.stderr)
                return result.returncode
            checked += 1

    print(f"INLINE_JS_SYNTAX_OK scripts={checked} file={html_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
