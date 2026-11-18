#!/usr/bin/env python3
"""Throughput of opencode runs over a time window.

Reads opencode's SQLite database read-only (no server needed, safe while
opencode or warp-agent is running).

Two rates, because they answer different questions:

  turn tok/s    generated tokens / (time.completed - time.created) for each
                assistant message. End-to-end: includes time to first token
                and any tool call run inside that message. This is the
                reliable number.
  stream tok/s  generated tokens / sum of the text and reasoning part
                durations. A ceiling, not a measurement: a non-streamed
                response records a near-zero part window, which inflates it.

"Generated" is output + reasoning tokens. opencode records them separately
and its total is input + cache.read + output + reasoning.

    scripts/dev/opencode-tok-s.py --days 7
    scripts/dev/opencode-tok-s.py --since 2026-11-01 --until 2026-11-07
    scripts/dev/opencode-tok-s.py --days 30 --by model
"""
from __future__ import annotations

import argparse
import json
import os
import sqlite3
import sys
from datetime import datetime, timedelta, timezone

DEFAULT_DB = os.path.expanduser("~/.local/share/opencode/opencode.db")


def parse_when(value: str) -> datetime:
    """Accept a date, a minute, or a full ISO timestamp, in local time."""
    text = value.strip().replace(" ", "T")
    for fmt in ("%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d"):
        try:
            return datetime.strptime(text, fmt).astimezone()
        except ValueError:
            pass
    parsed = datetime.fromisoformat(text)
    return parsed if parsed.tzinfo else parsed.astimezone()


def window(args: argparse.Namespace) -> tuple[datetime, datetime, str]:
    until = parse_when(args.until) if args.until else datetime.now().astimezone()
    if args.since:
        return parse_when(args.since), until, "requested window"
    if args.days is not None:
        return until - timedelta(days=args.days), until, f"last {args.days:g} days"
    return datetime(1970, 1, 1, tzinfo=timezone.utc), until, "all time"


def rows_in(con: sqlite3.Connection, since: datetime, until: datetime) -> list[dict]:
    raw = con.execute(
        """
        select m.id, m.session_id, m.data,
          (select min(json_extract(p.data, '$.time.start')) from part p
             where p.message_id = m.id
               and json_extract(p.data, '$.type') in ('text', 'reasoning')
               and json_extract(p.data, '$.time.start') is not null),
          (select max(json_extract(p.data, '$.time.end')) from part p
             where p.message_id = m.id
               and json_extract(p.data, '$.type') in ('text', 'reasoning')
               and json_extract(p.data, '$.time.end') is not null),
          (select sum(json_extract(p.data, '$.time.end')
                      - json_extract(p.data, '$.time.start')) from part p
             where p.message_id = m.id
               and json_extract(p.data, '$.type') in ('text', 'reasoning')
               and json_extract(p.data, '$.time.start') is not null
               and json_extract(p.data, '$.time.end') is not null),
          (select count(*) from part p where p.message_id = m.id
             and json_extract(p.data, '$.type') in ('text', 'reasoning')),
          (select count(*) from part p where p.message_id = m.id
             and json_extract(p.data, '$.type') in ('text', 'reasoning')
             and (json_extract(p.data, '$.time.start') is null
                  or json_extract(p.data, '$.time.end') is null)),
          (select count(*) from part p where p.message_id = m.id
             and json_extract(p.data, '$.type') = 'tool')
        from message m
        where m.time_created >= ? and m.time_created < ?
          and json_extract(m.data, '$.role') = 'assistant'
        order by m.time_created
        """,
        (int(since.timestamp() * 1000), int(until.timestamp() * 1000)),
    ).fetchall()

    out = []
    for mid, sid, blob, first, last, streamed, n_parts, n_untimed, n_tools in raw:
        data = json.loads(blob)
        stamp = data.get("time") or {}
        toks = data.get("tokens") or {}
        created, completed = stamp.get("created"), stamp.get("completed")
        generated = (toks.get("output") or 0) + (toks.get("reasoning") or 0)
        turn_s = ((completed - created) / 1000.0
                  if created and completed and completed >= created else None)
        decode_s = (streamed / 1000.0
                    if streamed and n_parts and not n_untimed and streamed > 0 else None)
        out.append({
            "id": mid,
            "session": sid,
            "model": f"{data.get('providerID', '?')}/{data.get('modelID', '?')}",
            "output": toks.get("output") or 0,
            "reasoning": toks.get("reasoning") or 0,
            "input": toks.get("input") or 0,
            "cache_read": (toks.get("cache") or {}).get("read") or 0,
            "generated": generated,
            "cost": data.get("cost") or 0.0,
            "turn_s": turn_s,
            "decode_s": decode_s,
            "tools": n_tools or 0,
            "first": first,
            "created": created,
            "completed": completed,
        })
    return out


def pct(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    values = sorted(values)
    return values[max(0, min(len(values) - 1, round(p / 100 * (len(values) - 1))))]


def rate(rows: list[dict], key: str) -> tuple[float, float, int]:
    """(generated, seconds, rows counted) over rows that have `key`."""
    usable = [r for r in rows if r[key]]
    return (sum(r["generated"] for r in usable),
            sum(r[key] for r in usable),
            len(usable))


def report(rows: list[dict], label: str, since: datetime, until: datetime, by: str | None):
    gen, turn_s, timed = rate(rows, "turn_s")
    dec, dec_s, decoded = rate(rows, "decode_s")
    output = sum(r["output"] for r in rows)
    wh = (until - since).total_seconds() / 3600

    print(f"\n=== {label} ===")
    print(f"window            {since:%Y-%m-%d %H:%M} .. {until:%Y-%m-%d %H:%M}"
          f"  ({wh:.1f} h)")
    print(f"assistant turns   {len(rows)}  ({timed} timed, {decoded} with part timing)")
    print(f"generated tokens  {gen:,}  (output {output:,} + reasoning {gen - output:,})")
    print(f"input tokens      {sum(r['input'] for r in rows):,}"
          f"  cache read {sum(r['cache_read'] for r in rows):,}")
    print(f"cost              ${sum(r['cost'] for r in rows):.2f}")
    if turn_s:
        print(f"turn time         {turn_s / 60:.1f} min busy -> {turn_s / 3600:.2f} h")
        print(f"turn tok/s        {gen / turn_s:6.1f} generated   {output / turn_s:6.1f} output only")
        print(f"stream tok/s      {dec / dec_s:6.1f} generated   {output / dec_s:6.1f} output only"
              if dec_s else "stream tok/s      (no part timing in window)")
        speeds = sorted(r["generated"] / r["turn_s"] for r in rows if r["turn_s"])
        print(f"per-turn tok/s    median {pct(speeds, 50):.1f}  p90 {pct(speeds, 90):.1f}"
              f"  max {speeds[-1]:.1f}")
    wall = (until - since).total_seconds() or 1
    print(f"wall tok/s        {gen / wall:6.2f} generated  (idle gaps included)")

    if by:
        groups: dict[str, list[dict]] = {}
        for r in rows:
            groups.setdefault(r[by], []).append(r)
        head = (f"{'group':34} {'turns':>6} {'gen tok':>10} {'busy s':>8} "
                f"{'turn/s':>7} {'strm/s':>7} {'cost':>7}")
        print(f"\n{head}")
        for key, group in sorted(groups.items(), key=lambda kv: -sum(r["generated"] for r in kv[1])):
            g = sum(r["generated"] for r in group)
            t = sum(r["turn_s"] for r in group if r["turn_s"])
            d = sum(r["decode_s"] for r in group if r["decode_s"])
            print(f"{str(key)[:34]:34} {len(group):6} {g:10,} {t:8.0f} "
                  f"{(g / t if t else 0):7.1f} {(g / d if d else 0):7.1f} "
                  f"{sum(r['cost'] for r in group):7.2f}")

    return {
        "label": label,
        "turns": len(rows),
        "generated_tokens": gen,
        "output_tokens": output,
        "turn_seconds": turn_s,
        "turn_tokens_per_second": gen / turn_s if turn_s else None,
        "decode_seconds": dec_s,
        "decode_tokens_per_second": dec / dec_s if dec_s else None,
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--db", default=os.environ.get("OPENCODE_DB", DEFAULT_DB))
    ap.add_argument("--days", type=float, help="last N days, counted from --until")
    ap.add_argument("--since", help="start: 2026-11-06, 2026-11-06T14:00, or ISO")
    ap.add_argument("--until", help="end, default now")
    ap.add_argument("--by", choices=["model", "session"], help="per-group breakdown")
    ap.add_argument("--json", action="store_true", help="also print the summary as JSON")
    args = ap.parse_args()

    path = os.path.expanduser(args.db)
    if not os.path.exists(path):
        sys.exit(f"no opencode database at {path}")
    since, until, label = window(args)
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    try:
        rows = rows_in(con, since, until)
    finally:
        con.close()

    if label == "all time" and rows:
        first = min(r["created"] for r in rows if r["created"])
        since = datetime.fromtimestamp(first / 1000).astimezone()

    summary = report(rows, label, since, until, args.by)
    if args.json:
        print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
