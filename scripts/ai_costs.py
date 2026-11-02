#!/usr/bin/env python3
"""Summarise an episode's AI call log (Logs/ai-calls.jsonl) per task.

usage: python3 scripts/ai_costs.py <episode folder or ai-calls.jsonl> [--since ISO-TIME]

Prints calls, failures, seconds, input size and (Claude Code provider) tokens and
API-equivalent cost per task. Cached answers are not logged, so this counts real calls.
"""

import json
import sys
from collections import defaultdict
from pathlib import Path


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    p = Path(sys.argv[1])
    log = p / "Logs" / "ai-calls.jsonl" if p.is_dir() else p
    since = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else ""
    rows = defaultdict(lambda: defaultdict(float))
    for line in log.read_text().splitlines():
        d = json.loads(line)
        if d.get("ts", "") < since:
            continue
        r = rows[d["task"]]
        r["calls"] += 1
        r["failed"] += 0 if d.get("ok") else 1
        r["seconds"] += d.get("duration_ms", 0) / 1000
        r["input_chars"] += d.get("input_chars", 0)
        u = d.get("usage") or {}
        r["cost_usd"] += u.get("cost_usd") or 0
        t = u.get("usage") or {}
        r["tokens_in"] += (t.get("input_tokens") or 0) + (t.get("cache_creation_input_tokens") or 0) + (
            t.get("cache_read_input_tokens") or 0
        )
        r["tokens_out"] += t.get("output_tokens") or 0
    head = f"{'task':15} {'calls':>5} {'failed':>6} {'seconds':>8} {'in kB':>7} {'tok in':>8} {'tok out':>8} {'cost $':>7}"
    print(head)
    total = defaultdict(float)
    for task, r in sorted(rows.items()):
        for k, v in r.items():
            total[k] += v
        print(
            f"{task:15} {r['calls']:5.0f} {r['failed']:6.0f} {r['seconds']:8.0f} {r['input_chars'] / 1000:7.0f}"
            f" {r['tokens_in']:8.0f} {r['tokens_out']:8.0f} {r['cost_usd']:7.3f}"
        )
    print(
        f"{'total':15} {total['calls']:5.0f} {total['failed']:6.0f} {total['seconds']:8.0f} {total['input_chars'] / 1000:7.0f}"
        f" {total['tokens_in']:8.0f} {total['tokens_out']:8.0f} {total['cost_usd']:7.3f}"
    )


if __name__ == "__main__":
    main()
