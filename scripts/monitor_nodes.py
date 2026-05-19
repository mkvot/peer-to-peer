#!/usr/bin/env python3

import argparse
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent / "scenarios"))

from lib import (
    BOLD,
    DIM,
    GREEN,
    PORT_COLORS,
    RESET,
    TIP_COLORS,
    color,
    fmt_attempts,
    http_json,
    short_hash,
)


LOG_PATH = Path(__file__).with_suffix(".log")


def parse_ports(value):
    ports = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            start, end = part.split("-", 1)
            ports.extend(range(int(start), int(end) + 1))
        else:
            ports.append(int(part))
    return ports


def snapshot(port, host):
    status_code, status = http_json(port, "GET", "/status", timeout=0.7, host=host)
    mining_code, mining = http_json(port, "GET", "/mining/status", timeout=0.7, host=host)
    events_code, events = http_json(port, "GET", "/events", timeout=0.7, host=host)
    if status_code != 200 or not isinstance(status, dict):
        return {
            "online": False,
            "height": None,
            "tip": None,
            "mempool": None,
            "orphans": None,
            "mining": False,
            "attempts": 0,
            "last_mined": None,
            "events": [],
            "last_event": "offline",
        }
    if mining_code != 200 or not isinstance(mining, dict):
        mining = {}
    if events_code != 200 or not isinstance(events, list):
        events = []
    last_event = events[-1]["message"][:20] if events else "-"
    return {
        "online": True,
        "height": status.get("height", 0),
        "tip": status.get("tip"),
        "mempool": status.get("mempool", 0),
        "orphans": status.get("orphans", 0),
        "mining": bool(mining.get("active", False)),
        "attempts": mining.get("attempts", 0),
        "last_mined": mining.get("last_mined_hash"),
        "events": events,
        "last_event": last_event,
    }


def render(args, snapshots, timeline, color_enabled):
    if sys.stdout.isatty():
        print("\033[2J\033[H", end="")
    online = [snap for snap in snapshots.values() if snap["online"]]
    heights = [snap["height"] for snap in online]
    tips = sorted({snap["tip"] for snap in online if snap["tip"]})
    mempool = sum(snap["mempool"] or 0 for snap in online)
    orphans = sum(snap["orphans"] or 0 for snap in online)
    mining = sum(1 for snap in online if snap["mining"])
    height_range = f"{min(heights)}..{max(heights)}" if heights else "-"
    print(color("node monitor", BOLD, color_enabled))
    print(
        f"online {len(online)}/{len(snapshots)} | heights {height_range} | "
        f"distinct tips {len(tips)} | mempool {mempool} | orphans {orphans} | mining {mining}"
    )
    print()

    tip_colors = {
        tip: TIP_COLORS[index % len(TIP_COLORS)] if color_enabled else ""
        for index, tip in enumerate(tips)
    }
    visible = list(snapshots.keys())[: args.show]

    if args.style == "box":
        line = "+------+-----+--------+----------+-----+------+--------+----------+------------+----------------------+"
        print(line)
        print("| node | on  | height | tip      | mem | orph | mining | attempts | last mined | last event           |")
        print(line)
    elif args.style == "grid":
        print("node   on   height  tip       mem  orph  mining  attempts  last mined  last event")
        print("-----  ---  ------  --------  ---  ----  ------  --------  ----------  --------------------")
    else:
        print("node online height tip      mining event")
        print("---- ------ ------ -------- ------ ----------------")

    for port in visible:
        snap = snapshots[port]
        node = color(f"{port:<4}", PORT_COLORS[port % len(PORT_COLORS)], color_enabled)
        online_text = "yes" if snap["online"] else "no"
        online_cell = color(f"{online_text:<3}", GREEN if snap["online"] else DIM, color_enabled)
        height = "-" if not snap["online"] else str(snap["height"])
        tip = short_hash(snap["tip"])
        tip = color(f"{tip:<8}", tip_colors.get(snap["tip"], DIM), color_enabled)
        mempool_cell = "-" if not snap["online"] else str(snap["mempool"])
        orphan_cell = "-" if not snap["online"] else str(snap["orphans"])
        mining_text = "yes" if snap["mining"] else "no"
        mining_cell = color(f"{mining_text:<6}", GREEN if snap["mining"] else DIM, color_enabled)
        attempts = fmt_attempts(snap["attempts"])
        last_mined = short_hash(snap["last_mined"])
        event = snap["last_event"]
        if args.style == "box":
            print(
                f"| {node} | {online_cell} | {height:>6} | {tip} | {mempool_cell:>3} | "
                f"{orphan_cell:>4} | {mining_cell} | {attempts:>8} | {last_mined:<10} | {event:<20} |"
            )
        elif args.style == "grid":
            print(
                f"{node}  {online_cell} {height:>7}  {tip} {mempool_cell:>3}  "
                f"{orphan_cell:>4}  {mining_cell}  {attempts:>8}  {last_mined:<10}  {event}"
            )
        else:
            print(f"{node} {online_cell} {height:>6} {tip} {mining_cell} {event}")
    if args.style == "box":
        print(line)
    if len(snapshots) > len(visible):
        print(f"... showing {len(visible)} of {len(snapshots)} nodes")

    print()
    print("timeline")
    print("--------")
    for elapsed, port, kind, message in timeline[-12:]:
        print(f"{elapsed:05.1f} [{kind:<6}] {port:<5} {message}")


def write_timeline(timeline):
    LOG_PATH.write_text(
        "\n".join(f"{elapsed:05.1f} [{kind:<6}] {port} {message}" for elapsed, port, kind, message in timeline)
        + ("\n" if timeline else ""),
        encoding="utf-8",
    )


def main():
    parser = argparse.ArgumentParser(description="Observe running peer-to-peer ledger nodes.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--ports", default="9000-9009")
    parser.add_argument("--interval", type=float, default=2.0)
    parser.add_argument("--fast", action="store_true")
    parser.add_argument("--show", type=int, default=10)
    parser.add_argument("--style", choices=["box", "grid", "minimal"], default="box")
    parser.add_argument("--no-color", action="store_true")
    args = parser.parse_args()
    if args.fast:
        args.interval = 0.5

    ports = parse_ports(args.ports)
    timeline = []
    seen = set()
    started = time.time()
    color_enabled = not args.no_color

    try:
        while True:
            snapshots = {port: snapshot(port, args.host) for port in ports}
            for port, snap in snapshots.items():
                for event in snap.get("events", []):
                    key = (port, event.get("time_ms"), event.get("kind"), event.get("message"))
                    if key in seen:
                        continue
                    seen.add(key)
                    timeline.append(
                        (
                            max(0.0, event.get("time_ms", 0) / 1000.0 - started),
                            port,
                            event.get("kind", "event"),
                            event.get("message", ""),
                        )
                    )
            render(args, snapshots, timeline, color_enabled)
            write_timeline(timeline)
            time.sleep(args.interval)
    except KeyboardInterrupt:
        write_timeline(timeline)
        print()


if __name__ == "__main__":
    main()
