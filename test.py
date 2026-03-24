#!/usr/bin/env python3
"""
P2P Network Test Suite
Usage:
    python3 test.py                     # run all tests
    python3 test.py --large             # 30-node propagation test
    python3 test.py --large2            # 40-node gradual test
    python3 test.py --scale             # scale limit test (finds max viable nodes)
    python3 test.py ./target/release/peer-to-peer --large
"""

import subprocess
import time
import json
import hashlib
import requests
import sys
import os
import signal
from datetime import datetime
from typing import Optional

BINARY = "./target/debug/peer-to-peer"
for arg in sys.argv[1:]:
    if not arg.startswith("--") and os.path.exists(arg):
        BINARY = arg

RUN_LARGE  = "large"  in sys.argv
RUN_LARGE2 = "large2" in sys.argv
RUN_SCALE  = "scale"  in sys.argv
BASE_PORT = 9000
TIMEOUT = 3
RESULTS_FILE = f"test_results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt"

processes = []
log_lines = []

# ── logging ────────────────────────────────────────────────────────────────────

def log(msg, color=None):
    codes = {"red": "\033[91m", "green": "\033[92m", "yellow": "\033[93m",
             "blue": "\033[94m", "cyan": "\033[96m", "reset": "\033[0m"}
    if color:
        print(f"{codes[color]}{msg}{codes['reset']}")
    else:
        print(msg)
    clean = msg
    for code in codes.values():
        clean = clean.replace(code, "")
    log_lines.append(clean)

def save_results():
    with open(RESULTS_FILE, "w") as f:
        f.write("\n".join(log_lines))
    print(f"\nResults saved to {RESULTS_FILE}")

# ── helpers ────────────────────────────────────────────────────────────────────

def sha256(content: str) -> str:
    return hashlib.sha256(content.encode()).hexdigest()

def addr(port: int) -> str:
    return f"127.0.0.1:{port}"

def start_node(port: int, peers: list = []) -> subprocess.Popen:
    peers_file = f"/tmp/peers_{port}.json"
    with open(peers_file, "w") as f:
        json.dump([addr(p) for p in peers], f)
    proc = subprocess.Popen(
        [BINARY, str(port), peers_file],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    processes.append((proc, port))
    return proc

def stop_node(proc: subprocess.Popen, port: int):
    proc.terminate()
    processes[:] = [(p, po) for p, po in processes if p != proc]
    log(f"  killed node :{port}", "red")

def stop_all():
    for proc, port in processes[:]:
        proc.terminate()
    processes.clear()

def status(port: int) -> Optional[dict]:
    try:
        r = requests.get(f"http://127.0.0.1:{port}/status", timeout=TIMEOUT)
        return r.json() if r.status_code == 200 else None
    except:
        return None

def post_block(port: int, content: str) -> bool:
    h = sha256(content)
    try:
        r = requests.post(f"http://127.0.0.1:{port}/block",
                          json={"hash": h, "content": content}, timeout=TIMEOUT)
        return r.status_code == 200
    except:
        return False

def post_inv(port: int, content: str) -> bool:
    h = sha256(content)
    try:
        r = requests.post(f"http://127.0.0.1:{port}/inv",
                          json={"hash": h, "content": content}, timeout=TIMEOUT)
        return r.status_code == 200
    except:
        return False

def wait_for_node(port: int, timeout=5) -> bool:
    start = time.time()
    while time.time() - start < timeout:
        if status(port):
            return True
        time.sleep(0.2)
    return False

def wait_for_all(ports: list, timeout=10):
    for p in ports:
        wait_for_node(p, timeout)

def snapshot(ports: list, label: str):
    log(f"\n  [{label}]", "cyan")
    for port in ports:
        s = status(port)
        if s:
            peers = [p.split(":")[1] for p in s["peers"]]
            log(f"  :{port}  peers={peers}  blocks={s['block_count']}  txns={s['transaction_count']}")
        else:
            log(f"  :{port}  OFFLINE", "red")

def wait_propagation(ports: list, expected_blocks: int, timeout=20) -> bool:
    start = time.time()
    while time.time() - start < timeout:
        statuses = [status(p) for p in ports]
        online = [s for s in statuses if s is not None]
        if online and all(s["block_count"] >= expected_blocks for s in online):
            return True
        time.sleep(0.5)
    return False

def check_block(ports: list, content: str) -> dict:
    h = sha256(content)
    results = {}
    for port in ports:
        try:
            r = requests.get(f"http://127.0.0.1:{port}/getdata/{h}", timeout=TIMEOUT)
            results[port] = r.status_code == 200
        except:
            results[port] = False
    return results

def section(title, subtitle=""):
    log(f"\n{'═'*55}", "blue")
    log(f"{title}", "blue")
    if subtitle:
        log(f"  {subtitle}", "blue")
    log(f"{'═'*55}", "blue")

# ── tests ──────────────────────────────────────────────────────────────────────

def test_linear_chain():
    section("TEST 1: Linear chain propagation",
            "9000 ── 9001 ── 9002 ── 9003 ── 9004")

    ports = [BASE_PORT + i for i in range(5)]
    procs = []
    procs.append(start_node(ports[0]))
    time.sleep(0.3)
    for i in range(1, 5):
        procs.append(start_node(ports[i], peers=[ports[i-1]]))
        time.sleep(0.3)

    wait_for_all(ports)
    time.sleep(10)
    snapshot(ports, "initial")

    log("\n  posting block to :9000...")
    post_block(ports[0], "chain test block")
    log("  waiting for propagation (10s)...")
    wait_propagation(ports, 1, timeout=10)
    snapshot(ports, "after propagation")

    results = check_block(ports, "chain test block")
    for port, has in results.items():
        log(f"  :{port}: {'✓' if has else '✗'}", "green" if has else "red")

    passed = all(results.values())
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")
    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_star_topology():
    section("TEST 2: Star topology",
            "9011–9014 all connect to hub 9010")

    hub = BASE_PORT + 10
    spokes = [BASE_PORT + 11 + i for i in range(4)]
    ports = [hub] + spokes
    procs = []

    procs.append(start_node(hub))
    time.sleep(0.3)
    for s in spokes:
        procs.append(start_node(s, peers=[hub]))
        time.sleep(0.2)

    wait_for_all(ports)
    time.sleep(10)
    snapshot(ports, "initial")

    log("\n  posting block to spoke :9011...")
    post_block(spokes[0], "star block")
    log("  posting transaction to hub :9010...")
    post_inv(hub, "star transaction")
    log("  waiting for propagation (10s)...")
    wait_propagation(ports, 1, timeout=10)
    time.sleep(3)
    snapshot(ports, "after propagation")

    results = check_block(ports, "star block")
    for port, has in results.items():
        log(f"  :{port}: {'✓' if has else '✗'}", "green" if has else "red")

    passed = all(results.values())
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")
    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_pairs_merge():
    section("TEST 3: Isolated pairs that merge",
            "9020──9021  and  9022──9023  then bridged")

    a1, a2 = BASE_PORT + 20, BASE_PORT + 21
    b1, b2 = BASE_PORT + 22, BASE_PORT + 23
    all_ports = [a1, a2, b1, b2]
    procs = []

    procs.append(start_node(a1))
    time.sleep(0.2)
    procs.append(start_node(a2, peers=[a1]))
    time.sleep(0.2)
    procs.append(start_node(b1))
    time.sleep(0.2)
    procs.append(start_node(b2, peers=[b1]))
    time.sleep(0.2)

    wait_for_all(all_ports)
    time.sleep(10)
    snapshot(all_ports, "isolated pairs")

    log("\n  posting block to pair A (:9020)...")
    post_block(a1, "pair A block")
    log("  posting transaction to pair B (:9022)...")
    post_inv(b1, "pair B transaction")
    time.sleep(5)
    snapshot(all_ports, "before merge")

    log("\n  bridging: announcing :9021 to :9022...")
    try:
        requests.post(f"http://127.0.0.1:{a2}/peers/announce",
                      json={"address": addr(b1)}, timeout=TIMEOUT)
    except:
        pass

    log("  waiting for merge propagation (20s)...")
    time.sleep(20)
    snapshot(all_ports, "after merge")

    results = check_block(all_ports, "pair A block")
    for port, has in results.items():
        log(f"  :{port}: {'✓' if has else '✗'}", "green" if has else "red")

    passed = all(results.values())
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")
    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_node_failure():
    section("TEST 4: Node failure resilience",
            "6 nodes, kill 2, verify remaining 4 still propagate")

    ports = [BASE_PORT + 30 + i for i in range(6)]
    procs = []

    procs.append(start_node(ports[0]))
    time.sleep(0.3)
    for i in range(1, 6):
        procs.append(start_node(ports[i], peers=[ports[0]]))
        time.sleep(0.2)

    wait_for_all(ports)
    time.sleep(12)
    snapshot(ports, "initial mesh")

    proc_kill_1, port_kill_1 = procs[2], ports[2]
    proc_kill_2, port_kill_2 = procs[3], ports[3]

    log(f"\n  killing :{port_kill_1} and :{port_kill_2}...")
    stop_node(proc_kill_1, port_kill_1)
    stop_node(proc_kill_2, port_kill_2)

    alive = [p for p in ports if p not in [port_kill_1, port_kill_2]]
    log("  waiting for dead peers to be pruned (25s)...")
    time.sleep(25)
    snapshot(alive, "after kills")

    log("\n  posting block to surviving node...")
    post_block(alive[0], "resilience block")
    wait_propagation(alive, 1, timeout=10)
    snapshot(alive, "after block")

    results = check_block(alive, "resilience block")
    for port, has in results.items():
        log(f"  :{port}: {'✓' if has else '✗'}", "green" if has else "red")

    passed = all(results.values())
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")
    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_late_joiner():
    section("TEST 5: Late joiner block sync",
            "3 nodes share blocks, new node joins and must sync")

    ports = [BASE_PORT + 40, BASE_PORT + 41, BASE_PORT + 42]
    late = BASE_PORT + 43
    procs = []

    procs.append(start_node(ports[0]))
    time.sleep(0.3)
    for p in ports[1:]:
        procs.append(start_node(p, peers=[ports[0]]))
        time.sleep(0.2)

    wait_for_all(ports)
    time.sleep(10)

    log("  posting 3 blocks...")
    post_block(ports[0], "early block 1")
    time.sleep(1)
    post_block(ports[1], "early block 2")
    time.sleep(1)
    post_block(ports[2], "early block 3")

    wait_propagation(ports, 3, timeout=10)
    snapshot(ports, "before late joiner")

    log(f"\n  starting late joiner :{late}...")
    procs.append(start_node(late, peers=[ports[0]]))
    wait_for_node(late)

    log("  waiting for late joiner to sync (25s)...")
    time.sleep(25)
    snapshot(ports + [late], "after late joiner")

    contents = ["early block 1", "early block 2", "early block 3"]
    results = {c: check_block([late], c)[late] for c in contents}
    for content, has in results.items():
        log(f"  :{late} has '{content}': {'✓' if has else '✗'}",
            "green" if has else "red")

    passed = all(results.values())
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")
    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_large_network():
    section("TEST 6: 30-node propagation",
            "Watch live at monitor.html — set range 9050–9080")

    hub = BASE_PORT + 50
    nodes = [BASE_PORT + 51 + i for i in range(29)]
    all_ports = [hub] + nodes
    procs = []

    log(f"  starting hub :{hub}...")
    procs.append(start_node(hub))
    time.sleep(0.5)

    log(f"  starting 29 nodes...")
    for p in nodes:
        procs.append(start_node(p, peers=[hub]))
        time.sleep(0.1)

    log("  waiting for nodes to come up (10s)...")
    wait_for_all(all_ports, timeout=10)
    time.sleep(15)

    online_before = sum(1 for p in all_ports if status(p))
    log(f"  {online_before}/{len(all_ports)} nodes online")
    snapshot(all_ports, "initial — open monitor.html to watch")

    log("\n  posting block to hub...")
    t0 = time.time()
    post_block(hub, "large network block 1")

    log("  posting block to middle node...")
    post_block(nodes[14], "large network block 2")

    log("  posting transaction to last node...")
    post_inv(nodes[-1], "large network transaction")

    log("\n  waiting for full propagation (20s)...")
    log("  open monitor.html in browser, set range 9050–9080", "yellow")
    wait_propagation(all_ports, 2, timeout=20)
    t1 = time.time()

    snapshot(all_ports, "after propagation")

    results = check_block(all_ports, "large network block 1")
    reached = sum(1 for v in results.values() if v)
    online_after = sum(1 for p in all_ports if status(p))

    log(f"\n  block reached:   {reached}/{len(all_ports)} nodes")
    log(f"  online nodes:    {online_after}/{len(all_ports)}")
    log(f"  propagation time: ~{t1-t0:.1f}s")

    passed = reached >= len(all_ports) * 0.9  # pass if 90%+ got it
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")

    log("\n  keeping nodes alive 60s so you can view the monitor...")
    log("  Ctrl+C to stop early", "yellow")
    try:
        time.sleep(60)
    except KeyboardInterrupt:
        pass

    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_gradual_propagation():
    """
    40 nodes join one by one, each connecting to a random existing node.
    After settling: blocks/transactions sent from random nodes.
    After 10s more: random nodes start dying.
    Watch on monitor.html — set range 9100-9140.
    """
    import random

    section("TEST 7: Gradual propagation (40 nodes)",
            "nodes trickle in, blocks fly, then nodes die  |  monitor: 9100-9140")

    BASE = BASE_PORT + 100
    N = 40
    all_ports = [BASE + i for i in range(N)]
    procs = []
    alive_ports = []

    log(f"  starting {N} nodes one by one (~0.5s apart)...")
    log("  open monitor.html now, set range 9100-9140", "yellow")

    procs.append(start_node(all_ports[0]))
    alive_ports.append(all_ports[0])
    wait_for_node(all_ports[0])
    log(f"  :{all_ports[0]} started (bootstrap)")
    time.sleep(1)

    for i in range(1, N):
        port = all_ports[i]
        peer = random.choice(alive_ports)
        procs.append(start_node(port, peers=[peer]))
        alive_ports.append(port)
        log(f"  :{port} joined -> knows :{peer}")
        time.sleep(0.5)

    log(f"\n  all {N} nodes started, settling for 10s...")
    time.sleep(10)

    online = sum(1 for p in all_ports if status(p))
    log(f"  {online}/{N} nodes online after settle")
    snapshot(all_ports, "after settle")

    log("\n  -- phase 2: sending blocks & transactions --", "yellow")
    block_contents = []

    for i in range(5):
        port = random.choice(alive_ports)
        content = f"block-{i+1}-from-{port}"
        block_contents.append(content)
        ok = post_block(port, content)
        log(f"  block {i+1}/5 -> :{port}: {'ok' if ok else 'FAILED'}", "green" if ok else "red")
        time.sleep(1)

    for i in range(3):
        port = random.choice(alive_ports)
        content = f"txn-{i+1}-from-{port}"
        ok = post_inv(port, content)
        log(f"  txn {i+1}/3 -> :{port}: {'ok' if ok else 'FAILED'}", "green" if ok else "red")
        time.sleep(0.5)

    log("\n  waiting 10s for propagation...")
    time.sleep(10)
    snapshot(all_ports, "after blocks/txns")

    for content in block_contents[:2]:
        results = check_block(all_ports, content)
        reached = sum(1 for v in results.values() if v)
        log(f"  '{content}' reached {reached}/{N} nodes",
            "green" if reached == N else "yellow")

    log("\n  -- phase 3: killing 10 random nodes --", "yellow")
    killed = []

    for i in range(10):
        if not procs:
            break
        idx = random.randint(0, len(procs) - 1)
        proc = procs[idx]
        port = alive_ports[idx]
        stop_node(proc, port)
        procs.pop(idx)
        alive_ports.pop(idx)
        killed.append(port)
        log(f"  killed :{port} ({i+1}/10)")
        time.sleep(1.5)

    log(f"\n  {len(alive_ports)} nodes remaining, waiting 20s to heal...")
    time.sleep(20)

    remaining = [p for p in all_ports if p not in killed]
    snapshot(remaining, "surviving nodes after kills")

    if alive_ports:
        port = random.choice(alive_ports)
        content = "post-kill-verification-block"
        post_block(port, content)
        log(f"\n  posted verification block to :{port}, waiting 10s...")
        time.sleep(10)
        results = check_block(alive_ports, content)
        reached = sum(1 for v in results.values() if v)
        log(f"  verification block reached {reached}/{len(alive_ports)} survivors",
            "green" if reached == len(alive_ports) else "yellow")

    online_final = sum(1 for p in remaining if status(p))
    log(f"\n  final: {online_final}/{len(remaining)} expected nodes online")
    log(f"  killed ports: {killed}")

    passed = online_final >= len(remaining) * 0.8
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'}", "green" if passed else "red")

    log("\n  keeping alive 30s for monitor viewing...")
    log("  Ctrl+C to stop early", "yellow")
    try:
        time.sleep(30)
    except KeyboardInterrupt:
        pass

    for p in procs: p.terminate()
    time.sleep(1)
    return passed


def test_scale_limit():
    """
    Finds the practical node limit of your network on this machine.

    Strategy: spin up nodes in batches, each batch connecting to the hub.
    After each batch settles, post a block and measure:
      - how many nodes came online
      - propagation reach %
      - propagation time

    Stops when either:
      - a batch has >20% nodes fail to start, OR
      - propagation reach drops below 80%, OR
      - SCALE_MAX_NODES is reached

    Port range: 9200–9399  (monitor.html: set range 9200–9399)
    """
    section("TEST 8: Scale limit",
            "finds max viable nodes  |  monitor: 9200-9399")

    SCALE_BASE        = BASE_PORT + 200
    SCALE_MAX_NODES   = 300   # hard ceiling — raise if your machine is beefy
    BATCH_SIZE        = 25    # nodes added per round
    SETTLE_TIME       = 12    # seconds to let a batch gossip
    PROP_TIMEOUT      = 25    # seconds to wait for propagation per round
    STARTUP_TIMEOUT   = 8     # seconds to wait for a single node to answer

    log(f"  max nodes cap : {SCALE_MAX_NODES}", "yellow")
    log(f"  batch size    : {BATCH_SIZE}")
    log(f"  port range    : {SCALE_BASE}–{SCALE_BASE + SCALE_MAX_NODES - 1}")
    log("  open monitor.html, set range 9200-9399", "yellow")

    hub_port = SCALE_BASE
    hub_proc = start_node(hub_port)
    if not wait_for_node(hub_port, timeout=STARTUP_TIMEOUT):
        log("  hub failed to start — aborting", "red")
        hub_proc.terminate()
        return False

    log(f"  hub :{hub_port} online")

    all_procs  = [hub_proc]
    all_ports  = [hub_port]
    round_num  = 0
    last_good  = 0           # last N where propagation was healthy

    results_table = []       # list of dicts, one per round

    while len(all_ports) < SCALE_MAX_NODES:
        round_num += 1
        batch_start = len(all_ports)
        batch_ports = [SCALE_BASE + batch_start + i for i in range(BATCH_SIZE)
                       if batch_start + i < SCALE_MAX_NODES]
        if not batch_ports:
            break

        log(f"\n  ── round {round_num}: adding nodes "
            f"{batch_ports[0]}–{batch_ports[-1]} "
            f"(total will be {len(all_ports) + len(batch_ports)}) ──", "cyan")

        # start batch — each node points at hub
        for port in batch_ports:
            all_procs.append(start_node(port, peers=[hub_port]))
            all_ports.append(port)
            time.sleep(0.15)

        # wait for the batch to respond
        came_up = sum(1 for p in batch_ports if wait_for_node(p, timeout=STARTUP_TIMEOUT))
        online_total = sum(1 for p in all_ports if status(p))
        startup_rate = came_up / len(batch_ports)

        log(f"  batch startup : {came_up}/{len(batch_ports)} ({startup_rate*100:.0f}%)")
        log(f"  total online  : {online_total}/{len(all_ports)}")

        if startup_rate < 0.8:
            log(f"  startup rate dropped below 80% — this is the limit", "red")
            log(f"  practical limit: ~{last_good} nodes", "yellow")
            break

        # let gossip settle
        log(f"  settling for {SETTLE_TIME}s...")
        time.sleep(SETTLE_TIME)

        # post a block and time propagation
        content = f"scale-round-{round_num}-block"
        t0 = time.time()
        post_block(hub_port, content)
        propagated = wait_propagation(all_ports, round_num, timeout=PROP_TIMEOUT)
        t1 = time.time()
        prop_time = t1 - t0

        # measure actual reach
        reach_results = check_block(all_ports, content)
        reached = sum(1 for v in reach_results.values() if v)
        reach_pct = reached / len(all_ports) * 100

        log(f"  propagation   : {reached}/{len(all_ports)} nodes ({reach_pct:.0f}%) in {prop_time:.1f}s",
            "green" if reach_pct >= 80 else "red")

        results_table.append({
            "round":      round_num,
            "nodes":      len(all_ports),
            "online":     online_total,
            "reached":    reached,
            "reach_pct":  reach_pct,
            "prop_time":  prop_time,
        })

        if reach_pct >= 80:
            last_good = len(all_ports)
        else:
            log(f"  propagation reach dropped below 80% — this is the limit", "red")
            log(f"  practical limit: ~{last_good} nodes", "yellow")
            break

    # ── summary table ──────────────────────────────────────────────────────────
    log(f"\n{'─'*55}", "cyan")
    log(f"  {'nodes':>6}  {'online':>6}  {'reached':>8}  {'reach%':>7}  {'time':>6}", "cyan")
    log(f"{'─'*55}", "cyan")
    for r in results_table:
        color = "green" if r["reach_pct"] >= 80 else "red"
        log(f"  {r['nodes']:>6}  {r['online']:>6}  {r['reached']:>8}  "
            f"{r['reach_pct']:>6.0f}%  {r['prop_time']:>5.1f}s", color)
    log(f"{'─'*55}", "cyan")
    log(f"\n  practical limit: ~{last_good} nodes on this machine", "yellow")

    passed = last_good >= 20   # reasonable floor: at least 20 nodes should work
    log(f"\n  RESULT: {'PASSED' if passed else 'FAILED'} (limit >= 20 nodes)",
        "green" if passed else "red")

    log("\n  keeping nodes alive 30s — view monitor.html while they run...")
    log("  Ctrl+C to stop early", "yellow")
    try:
        time.sleep(30)
    except KeyboardInterrupt:
        pass

    stop_all()
    time.sleep(1)
    return passed


# ── main ───────────────────────────────────────────────────────────────────────

def cleanup():
    stop_all()
    for f in os.listdir("/tmp"):
        if f.startswith("peers_") and f.endswith(".json"):
            try:
                os.remove(f"/tmp/{f}")
            except:
                pass

def main():
    signal.signal(signal.SIGINT, lambda s, f: (cleanup(), save_results(), sys.exit(0)))

    if not os.path.exists(BINARY):
        log(f"Binary not found: {BINARY}", "red")
        log("Run: cargo build", "yellow")
        sys.exit(1)

    log("P2P Network Test Suite", "cyan")
    log(f"Binary:  {BINARY}", "cyan")
    log(f"Time:    {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}", "cyan")
    log(f"Output:  {RESULTS_FILE}", "cyan")

    if RUN_LARGE:
        tests = [("30-node network", test_large_network)]
    elif RUN_LARGE2:
        tests = [("Gradual propagation", test_gradual_propagation)]
    elif RUN_SCALE:
        tests = [("Scale limit", test_scale_limit)]
    else:
        tests = [
            ("Linear chain",  test_linear_chain),
            ("Star topology", test_star_topology),
            ("Pairs merge",   test_pairs_merge),
            ("Node failure",  test_node_failure),
            ("Late joiner",   test_late_joiner),
            ("30-node network", test_large_network),
            ("Gradual propagation", test_gradual_propagation),
            ("Scale limit", test_scale_limit),
        ]

    results = {}
    for name, fn in tests:
        try:
            results[name] = fn()
        except Exception as e:
            log(f"  ERROR in {name}: {e}", "red")
            results[name] = False
        finally:
            stop_all()
            time.sleep(2)

    log(f"\n{'═'*55}", "cyan")
    log("SUMMARY", "cyan")
    log(f"{'═'*55}", "cyan")
    for name, passed in results.items():
        log(f"  {'✓' if passed else '✗'}  {name}", "green" if passed else "red")

    total = sum(results.values())
    log(f"\n  {total}/{len(results)} tests passed",
        "green" if total == len(results) else "yellow")

    cleanup()
    save_results()


if __name__ == "__main__":
    main()
