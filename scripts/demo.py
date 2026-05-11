#!/usr/bin/env python3
"""
Usage:
    python3 scripts/demo.py                         # baseline + converge + bad actor + no-quorum + leader failure
    python3 scripts/demo.py --step                  # ask before/after major actions in every selected scenario
    python3 scripts/demo.py --scenario converge     # only one scenario
    python3 scripts/demo.py --scenario bad-actor    # invalid /inv is rejected
    python3 scripts/demo.py --scenario no-quorum    # consensus enabled, but no quorum
    python3 scripts/demo.py --scenario scale        # 30-node consensus scaling demo
    python3 scripts/demo.py --include-scale --step  # default scenarios plus scale, with confirmations
    python3 scripts/demo.py --output demo.txt       # write the terminal log to a file
    python3 scripts/demo.py --scenario converge --hold
    python3 scripts/demo.py --no-pause              # fast non-interactive run
    python3 scripts/demo.py ./target/release/peer-to-peer --scenario leader-failure
"""

import argparse
import json
import os
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlencode


DEFAULT_BINARY = "./target/debug/peer-to-peer"
BASE_PORT = 11000
TIMEOUT = 3

processes = []
log_lines = []
demo_state = {
    "scenario": "",
    "host": "127.0.0.1",
    "base_port": BASE_PORT,
    "ports": [],
    "target_ports": [],
    "updated_at": 0.0,
}
demo_state_lock = threading.Lock()
demo_control_server = None


# -- logging ------------------------------------------------------------------


def log(msg, color=None):
    codes = {
        "red": "\033[91m",
        "green": "\033[92m",
        "yellow": "\033[93m",
        "blue": "\033[94m",
        "cyan": "\033[96m",
        "reset": "\033[0m",
    }
    if color:
        print(f"{codes[color]}{msg}{codes['reset']}")
    else:
        print(msg)

    clean = msg
    for code in codes.values():
        clean = clean.replace(code, "")
    log_lines.append(clean)


def section(title, subtitle=""):
    log(f"\n{'═' * 64}", "blue")
    log(title, "blue")
    if subtitle:
        log(f"  {subtitle}", "blue")
    log(f"{'═' * 64}", "blue")


def default_results_file():
    return f"demo_results_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt"


def save_results(path=None):
    if not path:
        print("\nResults not written. Use --output <path> or --save-results to save the demo log.")
        return

    directory = os.path.dirname(path)
    if directory:
        os.makedirs(directory, exist_ok=True)

    with open(path, "w") as file:
        file.write("\n".join(log_lines))
    print(f"\nResults saved to {path}")


def pause_if_enabled(enabled, message="Press Enter to continue..."):
    if enabled:
        try:
            input(f"\n  {message}")
        except EOFError:
            log(f"\n  {message} [stdin unavailable; continuing]", "yellow")


class DemoStateHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.split("?", 1)[0] != "/state":
            self.send_response(404)
            self.end_headers()
            return

        with demo_state_lock:
            payload = json.dumps(demo_state).encode()

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, _format, *_args):
        return


def start_demo_control_server(port):
    server = ThreadingHTTPServer(("127.0.0.1", port), DemoStateHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def stop_demo_control_server():
    global demo_control_server
    if demo_control_server is not None:
        demo_control_server.shutdown()
        demo_control_server.server_close()
        demo_control_server = None


# -- helpers ------------------------------------------------------------------


def addr(port: int) -> str:
    return f"127.0.0.1:{port}"


def request(method: str, port: int, path: str, body=None):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"

    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT) as response:
        raw = response.read().decode()
        return response.status, json.loads(raw) if raw else None


def request_allow_error(method: str, port: int, path: str, body=None):
    try:
        return request(method, port, path, body)
    except urllib.error.HTTPError as error:
        raw = error.read().decode()
        return error.code, json.loads(raw) if raw else None


def status(port: int):
    try:
        _, payload = request("GET", port, "/ledger/status")
        return payload
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return None


def ledger(port: int):
    try:
        _, payload = request("GET", port, "/ledger")
        return payload
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return None


def post_tx(port: int, body: str):
    code, payload = request("POST", port, "/tx", {"body": body})
    if code != 200:
        raise RuntimeError(f"POST /tx on {port} returned {code}: {payload}")
    return payload["tx"]


def post_inv_raw(port: int, body):
    return request_allow_error("POST", port, "/inv", body)


def start_node(binary: str, port: int, peers=None, data_dir="/tmp/p2p-demo", consensus=True):
    if peers is None:
        peers = []

    cmd = [
        binary,
        str(port),
        "--data-dir",
        data_dir,
        "--round-secs",
        "2",
        "--no-forward-inv",
    ]
    if consensus:
        cmd.append("--consensus")
    else:
        cmd.append("--no-consensus")

    for peer in peers:
        cmd.extend(["--peer", addr(peer)])

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    processes.append((proc, port))
    return proc


def stop_node_by_port(port: int):
    for proc, proc_port in processes[:]:
        if proc_port != port:
            continue
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
        processes.remove((proc, proc_port))
        log(f"  killed node :{port}", "red")
        return
    raise RuntimeError(f"node {port} is not managed by this demo")


def stop_all():
    for proc, _ in processes[:]:
        proc.terminate()
    for proc, _ in processes[:]:
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()
    processes.clear()


def wait_for_node(port: int, timeout=8) -> bool:
    start = time.time()
    while time.time() - start < timeout:
        if status(port):
            return True
        time.sleep(0.2)
    return False


def wait_for_all(ports: list, timeout=10):
    for port in ports:
        if not wait_for_node(port, timeout):
            raise RuntimeError(f"node :{port} did not start")


def start_cluster(binary: str, ports: list, data_dir: str, consensus=True):
    start_node(binary, ports[0], data_dir=data_dir, consensus=consensus)
    wait_for_node(ports[0])
    for port in ports[1:]:
        start_node(binary, port, peers=[ports[0]], data_dir=data_dir, consensus=consensus)
        wait_for_node(port)
        time.sleep(0.2)
    wait_for_all(ports)


def short_hash(value: str) -> str:
    if not value:
        return ""
    return value[:12]


def snapshot(ports: list, label: str):
    log(f"\n  [{label}]", "cyan")
    rows = {}
    for port in ports:
        stat = status(port)
        entries = ledger(port)
        if not stat:
            log(f"  :{port}  OFFLINE", "red")
            continue
        rows[port] = {"status": stat, "ledger": entries or []}
        peers = [peer.split(":")[-1] for peer in stat["peers"]]
        mode = "consensus" if stat["consensus_enabled"] else "direct"
        bodies = [tx["body"] for tx in rows[port]["ledger"]]
        log(
            f"  :{port}  mode={mode:<9} peers={peers} "
            f"ledger={stat['ledger_len']} mempool={stat['mempool_count']} "
            f"round={stat['next_round']} commits={stat['commit_count']} "
            f"hash={short_hash(stat['ledger_hash'])} txs={bodies}"
        )
    return rows


def snapshot_summary(ports: list, label: str, sample_limit=10):
    rows = snapshot_data(ports)
    online_ports = sorted(rows)
    hashes = {row["status"]["ledger_hash"] for row in rows.values()}
    lengths = [row["status"]["ledger_len"] for row in rows.values()]
    mempools = [row["status"]["mempool_count"] for row in rows.values()]

    log(f"\n  [{label}]", "cyan")
    log(
        f"  online={len(rows)}/{len(ports)} unique_hashes={len(hashes)} "
        f"ledger_min={min(lengths) if lengths else 0} ledger_max={max(lengths) if lengths else 0} "
        f"mempool_total={sum(mempools)}"
    )

    sample = online_ports
    if len(sample) > sample_limit:
        head = sample_limit // 2
        tail = sample_limit - head
        sample = online_ports[:head] + online_ports[-tail:]

    for port in sample:
        stat = rows[port]["status"]
        log(
            f"  :{port} ledger={stat['ledger_len']} mempool={stat['mempool_count']} "
            f"round={stat['next_round']} commits={stat['commit_count']} "
            f"hash={short_hash(stat['ledger_hash'])}"
        )

    omitted = len(online_ports) - len(sample)
    if omitted > 0:
        log(f"  ... {omitted} online nodes omitted from sample")

    return rows


def experiments_url(port: int, base_port: int, ports: list, target_ports=None):
    if target_ports is None:
        target_ports = ports

    query = {
        "host": "127.0.0.1",
        "basePort": str(base_port),
        "ports": ",".join(str(port) for port in ports),
        "targetPorts": ",".join(str(port) for port in target_ports),
    }
    if demo_control_server is not None:
        control_port = demo_control_server.server_address[1]
        query["demoState"] = f"http://127.0.0.1:{control_port}/state"

    return f"http://127.0.0.1:{port}/experiments?{urlencode(query)}"


def update_demo_state(label: str, base_port: int, ports: list, target_ports=None):
    if target_ports is None:
        target_ports = ports

    with demo_state_lock:
        demo_state.update(
            {
                "scenario": label,
                "host": "127.0.0.1",
                "base_port": base_port,
                "ports": ports,
                "target_ports": target_ports,
                "updated_at": time.time(),
            }
        )


def print_scan_hint(ports: list, label: str, base_port: int, target_ports=None):
    if target_ports is None:
        target_ports = ports
    update_demo_state(label, base_port, ports, target_ports)
    first, last = min(ports), max(ports)
    log("\n  HTML dashboard scan info", "yellow")
    log(f"  scenario: {label}")
    log(f"  open:     http://127.0.0.1:{ports[0]}/experiments")
    log(f"  node:     http://127.0.0.1:{ports[0]}/")
    log("  host:     127.0.0.1")
    log(f"  ports:    {first} - {last}")
    log(f"  exact:    {', '.join(str(port) for port in ports)}")
    if target_ports != ports:
        log(f"  targets:  {', '.join(str(port) for port in target_ports)}")
    if demo_control_server is not None:
        control_port = demo_control_server.server_address[1]
        log(f"  helper:   http://127.0.0.1:{control_port}/state")


def wait_same_ledger(ports: list, expected_len: int, timeout=35):
    start = time.time()
    last_rows = None
    while time.time() - start < timeout:
        rows = snapshot_data(ports)
        last_rows = rows
        if len(rows) == len(ports):
            hashes = {row["status"]["ledger_hash"] for row in rows.values()}
            ledgers = {json.dumps(row["ledger"], sort_keys=True) for row in rows.values()}
            lengths = {row["status"]["ledger_len"] for row in rows.values()}
            if len(hashes) == 1 and len(ledgers) == 1 and lengths == {expected_len}:
                return rows
        time.sleep(0.5)

    if last_rows is not None:
        log("\n  [last convergence snapshot]", "cyan")
        for port, row in last_rows.items():
            stat = row["status"]
            log(
                f"  :{port} ledger={stat['ledger_len']} mempool={stat['mempool_count']} "
                f"round={stat['next_round']} hash={short_hash(stat['ledger_hash'])}"
            )
    raise AssertionError("nodes did not converge before timeout")


def snapshot_data(ports: list):
    rows = {}
    for port in ports:
        stat = status(port)
        entries = ledger(port)
        if stat:
            rows[port] = {"status": stat, "ledger": entries or []}
    return rows


def expected_leader_from_status(stat):
    members = sorted(set([stat["addr"]] + stat["peers"]))
    return members[stat["next_round"] % len(members)]


def pause_before_stop(step: bool, hold: bool, message: str):
    pause_if_enabled(step or hold, message)


# -- demo scenarios ------------------------------------------------------------


def demo_divergence(binary: str, base_port: int, pause=False, step=False, hold=False):
    ports = [base_port + i for i in range(3)]
    data_dir = f"/tmp/p2p-demo-divergence-{datetime.now().strftime('%H%M%S')}"

    section("DEMO 1: Baseline divergence", "consensus disabled; direct-mode nodes commit different local ledgers")
    print_scan_hint(ports, "divergence", base_port)

    try:
        start_cluster(binary, ports, data_dir, consensus=False)
        time.sleep(1)
        snapshot(ports, "initial")
        pause_if_enabled(pause, "Open the dashboard with the scan range above, then press Enter...")

        log("\n  posting one local transaction to each node...")
        for port in ports:
            tx = post_tx(port, f"divergence tx from {port}")
            log(f"  posted to :{port}: {tx['id'][:12]}")

        pause_if_enabled(step, "Transactions are posted. Inspect the dashboard, then press Enter for the result...")
        time.sleep(1)
        rows = snapshot(ports, "after local transactions")
        hashes = {row["status"]["ledger_hash"] for row in rows.values()}
        passed = len(hashes) == len(ports)
        log(
            f"\n  RESULT: {'PASSED' if passed else 'FAILED'} "
            f"({len(hashes)} unique ledger hashes)",
            "green" if passed else "red",
        )

        pause_before_stop(step, hold, "Cluster is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


def demo_converge(binary: str, base_port: int, pause=False, step=False, hold=False):
    ports = [base_port + 100 + i for i in range(5)]
    data_dir = f"/tmp/p2p-demo-converge-{datetime.now().strftime('%H%M%S')}"

    section("DEMO 2: Consensus convergence", "five nodes agree on the same ordered ledger")
    print_scan_hint(ports, "converge", base_port, target_ports=ports[:3])

    try:
        start_cluster(binary, ports, data_dir, consensus=True)
        log("\n  waiting for peer discovery and empty rounds...")
        time.sleep(8)
        snapshot(ports, "initial")
        pause_if_enabled(pause, "Open the dashboard with the scan range above, then press Enter...")

        log("\n  posting transactions to three different nodes...")
        for port in ports[:3]:
            tx = post_tx(port, f"converge tx from {port}")
            log(f"  posted to :{port}: {tx['id'][:12]}")

        pause_if_enabled(step, "Transactions are pending. Watch consensus in the dashboard, then press Enter...")
        rows = wait_same_ledger(ports, expected_len=3, timeout=40)
        snapshot(ports, "after consensus")
        hashes = {row["status"]["ledger_hash"] for row in rows.values()}
        passed = len(hashes) == 1
        log(
            f"\n  RESULT: {'PASSED' if passed else 'FAILED'} "
            f"(ledger hash {short_hash(next(iter(hashes)))})",
            "green" if passed else "red",
        )

        pause_before_stop(step, hold, "Cluster is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


def demo_bad_actor(binary: str, base_port: int, pause=False, step=False, hold=False):
    ports = [base_port + 200 + i for i in range(3)]
    data_dir = f"/tmp/p2p-demo-bad-actor-{datetime.now().strftime('%H%M%S')}"
    invalid_body = "invalid transaction should not commit"

    section("DEMO 3: Bad actor - invalid transaction", "a peer sends a forged tx id; honest nodes reject it")
    print_scan_hint(ports, "bad actor", base_port, target_ports=[ports[0]])

    try:
        start_cluster(binary, ports, data_dir, consensus=True)
        log("\n  waiting for peer discovery and empty rounds...")
        time.sleep(6)
        snapshot(ports, "initial")
        pause_if_enabled(pause, "Open the dashboard with the scan range above, then press Enter...")

        invalid_tx = {
            "id": "not-the-real-id",
            "origin": addr(ports[0]),
            "seq": 1,
            "body": invalid_body,
        }
        code, payload = post_inv_raw(ports[1], invalid_tx)
        log(f"\n  bad actor sent invalid /inv to :{ports[1]} status={code} response={payload}", "yellow")
        pause_if_enabled(step, "Invalid tx was sent. Inspect mempools/ledgers, then press Enter to post a valid tx...")

        valid_tx = post_tx(ports[0], "valid transaction after bad actor")
        log(f"  posted valid tx to :{ports[0]}: {valid_tx['id'][:12]}")

        rows = wait_same_ledger(ports, expected_len=1, timeout=35)
        snapshot(ports, "after consensus")
        invalid_committed = any(
            invalid_body in [tx["body"] for tx in row["ledger"]]
            for row in rows.values()
        )
        passed = code == 400 and not invalid_committed
        log(
            f"\n  RESULT: {'PASSED' if passed else 'FAILED'} "
            f"(invalid_status={code}, invalid_committed={invalid_committed})",
            "green" if passed else "red",
        )

        pause_before_stop(step, hold, "Cluster is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


def demo_no_quorum(binary: str, base_port: int, pause=False, step=False, hold=False):
    port = base_port + 300
    phantom_ports = [port + i for i in range(1, 5)]
    data_dir = f"/tmp/p2p-demo-no-quorum-{datetime.now().strftime('%H%M%S')}"

    section("DEMO 4: Consensus failure - no quorum", "consensus enabled, but one real node cannot reach majority")
    print_scan_hint([port], "no quorum", base_port)
    log(f"  phantom peers: {', '.join(addr(peer) for peer in phantom_ports)}")

    try:
        start_node(binary, port, peers=phantom_ports, data_dir=data_dir, consensus=True)
        if not wait_for_node(port):
            raise RuntimeError(f"node :{port} did not start")

        time.sleep(1)
        snapshot([port], "initial")
        pause_if_enabled(pause, "Open the dashboard with the scan info above, then press Enter...")

        log("\n  posting one transaction to the consensus-enabled node...")
        tx = post_tx(port, "no quorum tx")
        log(f"  posted to :{port}: {tx['id'][:12]}")

        pause_if_enabled(step, "Transaction is pending without quorum. Inspect the dashboard, then press Enter...")
        log("  waiting 15 seconds so several consensus rounds can fail...")
        time.sleep(15)
        rows = snapshot([port], "after no-quorum wait")
        stat = rows[port]["status"]
        passed = (
            stat["consensus_enabled"]
            and stat["ledger_len"] == 0
            and stat["mempool_count"] == 1
            and stat["commit_count"] == 0
        )
        log(
            f"\n  RESULT: {'PASSED' if passed else 'FAILED'} "
            f"(consensus_enabled={stat['consensus_enabled']}, ledger={stat['ledger_len']}, "
            f"mempool={stat['mempool_count']}, commits={stat['commit_count']})",
            "green" if passed else "red",
        )

        pause_before_stop(step, hold, "No-quorum node is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


def demo_leader_failure(binary: str, base_port: int, pause=False, step=False, hold=False):
    ports = [base_port + 150 + i for i in range(5)]
    data_dir = f"/tmp/p2p-demo-leader-failure-{datetime.now().strftime('%H%M%S')}"

    section("DEMO 5: Leader failure", "consensus has quorum, but the current protocol stalls because there is no view-change")
    print_scan_hint(ports, "leader failure", base_port)

    try:
        start_cluster(binary, ports, data_dir, consensus=True)
        log("\n  waiting for peer discovery and stable round 0...")
        time.sleep(8)
        rows = snapshot(ports, "initial")
        pause_if_enabled(pause, "Open the dashboard with the scan range above, then press Enter...")

        leader_addr = expected_leader_from_status(rows[ports[0]]["status"])
        leader_port = int(leader_addr.rsplit(":", 1)[1])
        log(f"\n  expected leader for round 0: {leader_addr}", "yellow")
        pause_if_enabled(step, "Leader is identified. Press Enter to kill it...")
        log(f"  killing leader :{leader_port}...")
        stop_node_by_port(leader_port)
        survivors = [port for port in ports if port != leader_port]

        pause_if_enabled(step, "Leader is down. Inspect online nodes, then press Enter to post survivor txs...")
        log("  waiting longer than one peer-maintenance tick...")
        time.sleep(10)

        log("\n  posting transactions to surviving nodes...")
        for port in survivors[:3]:
            tx = post_tx(port, f"leader failure tx from {port}")
            log(f"  posted to survivor :{port}: {tx['id'][:12]}")

        pause_if_enabled(step, "Survivor txs are pending. Watch the stalled state, then press Enter...")
        time.sleep(18)
        rows = snapshot(survivors, "after leader failure")
        lengths = {row["status"]["ledger_len"] for row in rows.values()}
        mempools = {row["status"]["mempool_count"] for row in rows.values()}
        hashes = {row["status"]["ledger_hash"] for row in rows.values()}

        stalled = len(hashes) == 1 and lengths == {0} and max(mempools) > 0
        log(
            f"\n  RESULT: {'PASSED' if stalled else 'FAILED'} "
            f"(expected stalled; ledger lengths={lengths}, mempools={mempools})",
            "green" if stalled else "red",
        )

        pause_before_stop(step, hold, "Survivor cluster is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


def demo_scale(binary: str, base_port: int, node_count=30, tx_count=30, pause=False, step=False, hold=False):
    ports = [base_port + 500 + i for i in range(node_count)]
    data_dir = f"/tmp/p2p-demo-scale-{node_count}-{datetime.now().strftime('%H%M%S')}"

    section(
        f"DEMO 6: Scale - {node_count} consensus nodes",
        f"posts {tx_count} transactions and waits for one shared ledger",
    )
    print_scan_hint(ports, "scale", base_port)

    try:
        start_cluster(binary, ports, data_dir, consensus=True)
        log("\n  waiting for peer discovery and empty rounds...")
        time.sleep(10)
        snapshot_summary(ports, "initial scale snapshot")
        pause_if_enabled(pause, "Open the dashboard with the scan range above, then press Enter...")

        log(f"\n  posting {tx_count} transactions across {node_count} nodes...")
        for index in range(tx_count):
            port = ports[index % len(ports)]
            tx = post_tx(port, f"scale tx {index + 1} from {port}")
            if index < 5 or index >= tx_count - 3:
                log(f"  posted {index + 1:02d}/{tx_count} to :{port}: {tx['id'][:12]}")
        if tx_count > 8:
            log(f"  ... {tx_count - 8} posted tx lines omitted")

        pause_if_enabled(step, "Scale txs are posted. Watch mempools/rounds, then press Enter to wait for convergence...")
        rows = wait_same_ledger(ports, expected_len=tx_count, timeout=90)
        snapshot_summary(ports, "after scale consensus")
        hashes = {row["status"]["ledger_hash"] for row in rows.values()}
        lengths = {row["status"]["ledger_len"] for row in rows.values()}
        passed = len(hashes) == 1 and lengths == {tx_count}
        log(
            f"\n  RESULT: {'PASSED' if passed else 'FAILED'} "
            f"(nodes={node_count}, txs={tx_count}, unique_hashes={len(hashes)}, lengths={lengths})",
            "green" if passed else "red",
        )

        pause_before_stop(step, hold, "Scale cluster is still running. Press Enter to stop it...")
    finally:
        stop_all()
        time.sleep(1)


# -- main ----------------------------------------------------------------------


def parse_args():
    parser = argparse.ArgumentParser(description="Run a Prax2 defense demo")
    parser.add_argument("binary", nargs="?", default=DEFAULT_BINARY)
    parser.add_argument("--base-port", type=int, default=BASE_PORT)
    parser.add_argument(
        "--scenario",
        choices=["all", "divergence", "converge", "bad-actor", "no-quorum", "leader-failure", "scale"],
        default="all",
    )
    parser.add_argument(
        "--step",
        action="store_true",
        help="pause before/after major actions in every selected scenario",
    )
    parser.add_argument(
        "--no-pause",
        action="store_true",
        help="do not pause before scenario actions; useful for fast verification",
    )
    parser.add_argument(
        "--hold",
        action="store_true",
        help="keep each selected scenario running at the end until Enter",
    )
    parser.add_argument(
        "--include-scale",
        action="store_true",
        help="also run the scale scenario when --scenario all is selected",
    )
    parser.add_argument("--scale-nodes", type=int, default=30)
    parser.add_argument("--scale-txs", type=int, default=30)
    parser.add_argument(
        "--control-port",
        type=int,
        default=None,
        help="local helper port used by /experiments for automatic scan updates; default is base-port - 1",
    )
    parser.add_argument(
        "--no-control-server",
        action="store_true",
        help="disable the helper server that lets the browser auto-update scanned ports",
    )
    parser.add_argument(
        "--output",
        help="write the demo terminal log to this file; by default no file is written",
    )
    parser.add_argument(
        "--save-results",
        action="store_true",
        help="write the demo terminal log to a timestamped demo_results_*.txt file",
    )
    parser.add_argument("--no-build", action="store_true", help="skip cargo build")
    return parser.parse_args()


def main():
    global demo_control_server
    args = parse_args()
    if not args.no_build:
        log("building debug binary...", "cyan")
        subprocess.run(["cargo", "build"], check=True)

    if not os.path.exists(args.binary):
        raise SystemExit(f"binary not found: {args.binary}")
    if args.scale_nodes <= 0:
        raise SystemExit("--scale-nodes must be positive")
    if args.scale_txs <= 0:
        raise SystemExit("--scale-txs must be positive")

    output_path = args.output
    if output_path is None and args.save_results:
        output_path = default_results_file()

    if not args.no_control_server:
        control_port = args.control_port or max(1024, args.base_port - 1)
        try:
            demo_control_server = start_demo_control_server(control_port)
            log(f"browser demo-state helper: http://127.0.0.1:{control_port}/state", "cyan")
        except OSError as error:
            log(f"browser demo-state helper disabled: {error}", "yellow")

    pause = (not args.no_pause) or args.step

    def hold_for(name):
        return args.hold and args.scenario in ("all", name)

    try:
        if args.scenario in ("all", "divergence"):
            demo_divergence(
                args.binary,
                args.base_port,
                pause=pause,
                step=args.step,
                hold=hold_for("divergence"),
            )
        if args.scenario in ("all", "converge"):
            demo_converge(
                args.binary,
                args.base_port,
                pause=pause,
                step=args.step,
                hold=hold_for("converge"),
            )
        if args.scenario in ("all", "bad-actor"):
            demo_bad_actor(
                args.binary,
                args.base_port,
                pause=pause,
                step=args.step,
                hold=hold_for("bad-actor"),
            )
        if args.scenario in ("all", "no-quorum"):
            demo_no_quorum(
                args.binary,
                args.base_port,
                pause=pause,
                step=args.step,
                hold=hold_for("no-quorum"),
            )
        if args.scenario in ("all", "leader-failure"):
            demo_leader_failure(
                args.binary,
                args.base_port,
                pause=pause,
                step=args.step,
                hold=hold_for("leader-failure"),
            )
        if args.scenario == "scale" or (args.scenario == "all" and args.include_scale):
            demo_scale(
                args.binary,
                args.base_port,
                node_count=args.scale_nodes,
                tx_count=args.scale_txs,
                pause=pause,
                step=args.step,
                hold=hold_for("scale"),
            )
    finally:
        save_results(output_path)
        stop_demo_control_server()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        stop_all()
        sys.exit(130)
