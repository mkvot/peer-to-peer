#!/usr/bin/env python3

import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import threading
import time
from urllib import error, request


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_INTERVAL = 2.0
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
GREEN = "\033[32m"
RED = "\033[31m"
YELLOW = "\033[33m"
PURPLE = "\033[35m"
CYAN = "\033[36m"
TIP_COLORS = ["\033[36m", "\033[33m", "\033[35m", "\033[32m", "\033[34m", "\033[31m"]
PORT_COLORS = ["\033[38;5;39m", "\033[38;5;75m", "\033[38;5;111m", "\033[38;5;147m", "\033[38;5;183m"]


def common_parser(description, default_difficulty=4):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("--binary", default="./target/debug/peer-to-peer")
    parser.add_argument("--base-port", type=int, default=9000)
    parser.add_argument("--data-dir", default="./ledger_runs")
    parser.add_argument("--difficulty", type=int, default=default_difficulty)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--output")
    parser.add_argument("--long", action="store_true")
    parser.add_argument("--no-color", action="store_true")
    parser.add_argument("--table", choices=["compact", "monitor"], default="compact")
    parser.add_argument("--style", choices=["box", "grid", "minimal"], default="box")
    parser.add_argument("--timeline-lines", type=int, default=12)
    return parser


def resolve_binary(path):
    binary = Path(path)
    if not binary.is_absolute():
        binary = ROOT / binary
    return binary


def address(port):
    return f"127.0.0.1:{port}"


def short_hash(value):
    if not value:
        return "-"
    return str(value)[:8]


def color(text, code, enabled):
    return f"{code}{text}{RESET}" if enabled else text


def fmt_attempts(value):
    if not value:
        return "-"
    value = int(value)
    if value >= 1_000_000:
        return f"{value / 1_000_000:.1f}m"
    if value >= 1_000:
        return f"{value // 1_000}k"
    return str(value)


def http_json(port, method, path, payload=None, timeout=1.5, host="127.0.0.1"):
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload, separators=(",", ":")).encode()
        headers["Content-Type"] = "application/json"
    req = request.Request(
        f"http://{host}:{port}{path}",
        data=data,
        method=method,
        headers=headers,
    )
    try:
        with request.urlopen(req, timeout=timeout) as response:
            body = response.read().decode()
            return response.status, json.loads(body) if body else None
    except error.HTTPError as exc:
        body = exc.read().decode()
        try:
            parsed = json.loads(body) if body else None
        except json.JSONDecodeError:
            parsed = body
        return exc.code, parsed
    except (error.URLError, TimeoutError, ConnectionError, OSError):
        return 0, None


class Scenario:
    def __init__(self, name, description, args, script_file):
        self.name = name
        self.description = description
        self.args = args
        self.script_path = Path(script_file).resolve()
        self.log_path = self.script_path.with_suffix(".log")
        self.binary = resolve_binary(args.binary)
        self.data_dir = Path(args.data_dir)
        if not self.data_dir.is_absolute():
            self.data_dir = ROOT / self.data_dir
        self.data_dir = self.data_dir / name
        self.color_enabled = not args.no_color
        self.ports = []
        self.processes = {}
        self.mine_threads = []
        self.timeline = []
        self.seen_events = set()
        self.started_at = time.time()
        self.output_lines = []
        self._cleaned = False

    def prepare(self):
        if not self.args.no_build:
            subprocess.run(["cargo", "build"], cwd=ROOT, check=True)
        if self.data_dir.exists():
            shutil.rmtree(self.data_dir)
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.log_path.write_text("", encoding="utf-8")
        self.note("start", f"{self.name}: {self.description}")

    def print_plan(self, ports, extra=None):
        self.ports = ports
        print(f"{self.name} - {self.description}")
        print(f"ports: {', '.join(str(port) for port in ports)}")
        print(f"data: {self.data_dir}")
        print(f"difficulty: {self.args.difficulty}")
        if extra:
            print(extra)
        wait_for_enter("Press Enter to start...")

    def start_nodes(self, ports, peer_map=None, timeout=20):
        self.ports = ports
        for port in ports:
            peers = []
            if peer_map is not None:
                peers = peer_map.get(port, [])
            cmd = [
                str(self.binary),
                str(port),
                "--data-dir",
                str(self.data_dir),
                "--difficulty",
                str(self.args.difficulty),
                "--bind-ip",
                "127.0.0.1",
            ]
            if peers:
                cmd.extend(["--peers", ",".join(address(peer) for peer in peers)])
            proc = subprocess.Popen(
                cmd,
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                text=True,
            )
            self.processes[port] = proc

        if not self.wait_online(ports, timeout=timeout):
            self.note("error", "not all nodes came online before timeout")
            raise RuntimeError("nodes did not start")

    def start_isolated(self, ports):
        self.start_nodes(ports, peer_map={port: [] for port in ports})

    def start_seeded(self, ports, timeout=20):
        first = ports[0]
        peer_map = {first: []}
        for port in ports[1:]:
            peer_map[port] = [first]
        self.start_nodes(ports, peer_map, timeout=timeout)

    def start_fully_connected(self, ports):
        peer_map = {port: [peer for peer in ports if peer != port] for port in ports}
        self.start_nodes(ports, peer_map)

    def wait_online(self, ports, timeout=20):
        deadline = time.time() + timeout
        while time.time() < deadline:
            statuses = self.statuses(ports)
            if all(statuses[port]["online"] for port in ports):
                return True
            time.sleep(0.3)
        return False

    def stop_nodes(self):
        if self._cleaned:
            return
        self._cleaned = True
        for proc in self.processes.values():
            if proc.poll() is None:
                proc.terminate()
        for proc in self.processes.values():
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
        self.write_logs()

    def install_signal_handlers(self):
        def handler(_signum, _frame):
            self.stop_nodes()
            raise SystemExit(130)

        signal.signal(signal.SIGINT, handler)
        signal.signal(signal.SIGTERM, handler)

    def get(self, port, path, timeout=1.5):
        return http_json(port, "GET", path, timeout=timeout)

    def post(self, port, path, payload, timeout=10):
        return http_json(port, "POST", path, payload, timeout=timeout)

    def wallet(self, port):
        status, body = self.get(port, "/wallet")
        if status != 200:
            raise RuntimeError(f"wallet request failed on {port}: {body}")
        return body["public_key"]

    def create_transaction(self, port, to, amount=1, memo="scenario"):
        status, body = self.post(
            port,
            "/transactions/create",
            {"to": to, "amount": amount, "memo": memo},
        )
        if status != 200:
            self.note("tx", f"{port} create transaction rejected: {body}")
            return None
        return body

    def mine_async(self, port, blocks=1, max_txs=50):
        print(f"[action] node {port} mining {blocks} block(s)")
        self.note("action", f"node {port} mining {blocks} block(s)")

        def run():
            status, body = self.post(
                port,
                "/mine",
                {"blocks": blocks, "max_txs": max_txs},
                timeout=max(30, blocks * 30),
            )
            if status != 200:
                self.note("mine", f"{port} mining request failed: {body}")

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        self.mine_threads.append(thread)

    def wait_for_mining_requests(self):
        for thread in self.mine_threads:
            thread.join(timeout=1)

    def block_peer(self, port, peer):
        self.post(port, "/debug/faults", {"block_peer": address(peer)})
        self.note("action", f"{port} blocked {peer}")

    def unblock_peer(self, port, peer):
        self.post(port, "/debug/faults", {"unblock_peer": address(peer)})
        self.note("action", f"{port} unblocked {peer}")

    def clear_blocked(self, port):
        self.post(port, "/debug/faults", {"clear_blocked_peers": True})
        self.note("action", f"{port} cleared blocked peers")

    def partition(self, left, right):
        for port in left:
            for peer in right:
                self.block_peer(port, peer)
        for port in right:
            for peer in left:
                self.block_peer(port, peer)

    def heal(self, ports):
        for port in ports:
            self.clear_blocked(port)

    def chain(self, port):
        status, body = self.get(port, "/chain")
        if status != 200:
            return []
        return body

    def fetch_block(self, port, block_hash):
        status, body = self.get(port, f"/blocks/{block_hash}")
        if status != 200:
            raise RuntimeError(f"block {block_hash} missing on {port}")
        return body

    def post_block(self, port, block):
        return self.post(port, "/blocks", block)

    def statuses(self, ports=None):
        ports = ports or self.ports

        def one(port):
            status_code, status = self.get(port, "/status", timeout=0.7)
            mining_code, mining = self.get(port, "/mining/status", timeout=0.7)
            events_code, events = self.get(port, "/events", timeout=0.7)
            online = status_code == 200 and isinstance(status, dict)
            if not online:
                return port, {
                    "online": False,
                    "height": None,
                    "tip": None,
                    "mempool": None,
                    "orphans": None,
                    "mining": False,
                    "attempts": 0,
                    "last_mined": None,
                    "last_event": "offline",
                }

            if mining_code != 200 or not isinstance(mining, dict):
                mining = {}
            if events_code == 200 and isinstance(events, list):
                self.ingest_events(port, events)
            last_event = self.last_event_for(port)

            return port, {
                "online": True,
                "height": status.get("height", 0),
                "tip": status.get("tip"),
                "mempool": status.get("mempool", 0),
                "orphans": status.get("orphans", 0),
                "mining": bool(mining.get("active", False)),
                "attempts": mining.get("attempts", 0),
                "last_mined": mining.get("last_mined_hash"),
                "last_event": last_event,
            }

        result = {}
        workers = min(max(len(ports), 1), 24)
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
            for port, snapshot in pool.map(one, ports):
                result[port] = snapshot
        return result

    def ingest_events(self, port, events):
        for event in events:
            key = (port, event.get("time_ms"), event.get("kind"), event.get("message"))
            if key in self.seen_events:
                continue
            self.seen_events.add(key)
            self.timeline.append(
                {
                    "elapsed": max(0.0, (event.get("time_ms", 0) / 1000.0) - self.started_at),
                    "port": port,
                    "kind": event.get("kind", "event"),
                    "message": event.get("message", ""),
                }
            )

    def last_event_for(self, port):
        for event in reversed(self.timeline):
            if event["port"] == port:
                return event["message"][:22]
        return "-"

    def note(self, kind, message, port=None):
        self.timeline.append(
            {
                "elapsed": max(0.0, time.time() - self.started_at),
                "port": port,
                "kind": kind,
                "message": message,
            }
        )

    def wait_until(self, predicate, timeout, phase, partition_map=None):
        deadline = time.time() + timeout
        while time.time() < deadline:
            snapshots = self.statuses()
            self.render(phase, snapshots, partition_map=partition_map)
            if predicate(snapshots):
                return True
            time.sleep(DEFAULT_INTERVAL)
        return False

    def run_for(self, seconds, phase, partition_map=None):
        deadline = time.time() + seconds
        snapshots = {}
        while time.time() < deadline:
            snapshots = self.statuses()
            self.render(phase, snapshots, partition_map=partition_map)
            time.sleep(DEFAULT_INTERVAL)
        return snapshots

    def render(self, phase, snapshots, partition_map=None):
        if sys.stdout.isatty():
            print("\033[2J\033[H", end="")
        title = color(f"{self.name} - {phase}", BOLD, self.color_enabled)
        print(title)
        print(f"ports: {self.ports[0] if self.ports else '-'}..{self.ports[-1] if self.ports else '-'} | data: {self.data_dir}")
        print()
        self.render_summary(snapshots)
        print()
        self.render_table(snapshots, partition_map or {})
        print()
        self.render_timeline()
        self.write_logs()

    def render_summary(self, snapshots):
        online = [snap for snap in snapshots.values() if snap["online"]]
        heights = [snap["height"] for snap in online if snap["height"] is not None]
        tips = {snap["tip"] for snap in online if snap["tip"]}
        mempool = sum(snap["mempool"] or 0 for snap in online)
        orphans = sum(snap["orphans"] or 0 for snap in online)
        mining = sum(1 for snap in online if snap["mining"])
        height_range = f"{min(heights)}..{max(heights)}" if heights else "-"
        print(
            f"t+{int(time.time() - self.started_at):02d}s | online {len(online)}/{len(snapshots)} "
            f"| tips {len(tips)} | heights {height_range} | mempool {mempool} | orphans {orphans} | mining {mining}"
        )

    def render_table(self, snapshots, partition_map):
        visible = self.ports[:10]
        tip_colors = self.tip_color_map(snapshots)
        if self.args.style == "box":
            line = "+------+----+--------+----------+-----+------+--------+----------+----------------------+"
            print(line)
            print("| node | pt | height | tip      | mem | orph | mining | attempts | last event           |")
            print(line)
        elif self.args.style == "grid":
            print("node   pt  height  tip       mem  orph  mining  attempts  last event")
            print("-----  --  ------  --------  ---  ----  ------  --------  --------------------")
        else:
            print("node height tip      mining event")
            print("---- ------ -------- ------ ----------------")

        for port in visible:
            snap = snapshots.get(port, {"online": False})
            part = partition_map.get(port, "-")
            node = color(f"{port:<4}", PORT_COLORS[port % len(PORT_COLORS)], self.color_enabled)
            height = "-" if not snap.get("online") else str(snap.get("height", 0))
            tip = short_hash(snap.get("tip"))
            tip = color(f"{tip:<8}", tip_colors.get(snap.get("tip"), DIM), self.color_enabled)
            mempool = "-" if not snap.get("online") else str(snap.get("mempool", 0))
            orphans = "-" if not snap.get("online") else str(snap.get("orphans", 0))
            mining = "yes" if snap.get("mining") else "no"
            mining = color(f"{mining:<6}", GREEN if snap.get("mining") else DIM, self.color_enabled)
            attempts = fmt_attempts(snap.get("attempts", 0))
            event = snap.get("last_event", "-")

            if self.args.style == "box":
                print(
                    f"| {node} | {part:<2} | {height:>6} | {tip} | {mempool:>3} | "
                    f"{orphans:>4} | {mining} | {attempts:>8} | {event:<20} |"
                )
            elif self.args.style == "grid":
                print(
                    f"{node}  {part:<2} {height:>7}  {tip} {mempool:>3}  "
                    f"{orphans:>4}  {mining}  {attempts:>8}  {event}"
                )
            else:
                print(f"{node} {height:>6} {tip} {mining} {event}")
        if self.args.style == "box":
            print(line)
        if len(self.ports) > len(visible):
            print(f"... showing {len(visible)} of {len(self.ports)} nodes")

    def render_timeline(self):
        print("timeline")
        print("--------")
        for event in self.timeline[-self.args.timeline_lines :]:
            port = "-" if event["port"] is None else str(event["port"])
            kind = f"{event['kind']:<6}"
            kind_color = {"mine": GREEN, "chain": PURPLE, "block": YELLOW, "error": RED}.get(
                event["kind"],
                DIM,
            )
            print(
                f"{event['elapsed']:05.1f} "
                f"[{color(kind, kind_color, self.color_enabled)}] "
                f"{port:<5} {event['message']}"
            )

    def tip_color_map(self, snapshots):
        tips = sorted({snap.get("tip") for snap in snapshots.values() if snap.get("tip")})
        return {
            tip: TIP_COLORS[index % len(TIP_COLORS)] if self.color_enabled else ""
            for index, tip in enumerate(tips)
        }

    def write_logs(self):
        lines = [
            f"{event['elapsed']:05.1f} [{event['kind']:<6}] "
            f"{'-' if event['port'] is None else event['port']} {event['message']}"
            for event in self.timeline
        ]
        self.log_path.write_text("\n".join(lines) + ("\n" if lines else ""), encoding="utf-8")
        if self.args.output:
            Path(self.args.output).write_text("\n".join(lines) + "\n", encoding="utf-8")

    def final(self, result, message):
        self.write_logs()
        print()
        print(f"RESULT: {result}")
        print(message)


def wait_for_enter(message):
    try:
        input(message)
    except EOFError:
        pass


def ports_from(base_port, count):
    return [base_port + offset for offset in range(count)]


def all_same_tip(snapshots):
    online = [snap for snap in snapshots.values() if snap["online"]]
    tips = {snap["tip"] for snap in online if snap["tip"]}
    return len(online) == len(snapshots) and len(tips) == 1


def distinct_tip_count(snapshots):
    return len({snap["tip"] for snap in snapshots.values() if snap.get("tip")})


def min_height(snapshots):
    heights = [snap["height"] for snap in snapshots.values() if snap.get("online")]
    return min(heights) if heights else 0


def max_height(snapshots):
    heights = [snap["height"] for snap in snapshots.values() if snap.get("online")]
    return max(heights) if heights else 0
