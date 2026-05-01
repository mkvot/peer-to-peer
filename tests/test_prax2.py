#!/usr/bin/env python3
"""
Prax2 experiment harness.

Usage:
    python3 tests/test_prax2.py --divergence
    python3 tests/test_prax2.py --converge
    python3 tests/test_prax2.py ./target/release/peer-to-peer --converge
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path


DEFAULT_BINARY = "./target/debug/peer-to-peer"
DEFAULT_BASE_PORT = 9500
TIMEOUT = 3

processes = []
peers_files = []
log_lines = []


def addr(port: int) -> str:
    return f"127.0.0.1:{port}"


def log(message: str):
    print(message)
    log_lines.append(message)


def save_results(label: str):
    results_dir = Path("prax2_results")
    results_dir.mkdir(exist_ok=True)
    path = results_dir / f"{label}_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt"
    path.write_text("\n".join(log_lines) + "\n")
    log(f"\nSaved results to {path}")


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
        payload = json.loads(raw) if raw else None
        return error.code, payload


def status(port: int):
    try:
        _, payload = request("GET", port, "/status")
        return payload
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return None


def ledger_status(port: int):
    _, payload = request("GET", port, "/ledger/status")
    return payload


def ledger(port: int):
    _, payload = request("GET", port, "/ledger")
    return payload


def post_tx(port: int, body: str):
    status_code, payload = request("POST", port, "/tx", {"body": body})
    if status_code != 200:
        raise RuntimeError(f"POST /tx on {port} returned {status_code}: {payload}")
    return payload["tx"]


def post_inv_raw(port: int, payload):
    return request_allow_error("POST", port, "/inv", payload)


def set_fault(port: int, payload):
    status_code, response = request("POST", port, "/debug/faults", payload)
    if status_code != 200:
        raise RuntimeError(f"POST /debug/faults on {port} returned {status_code}: {response}")
    return response


def wait_for_node(
    port: int,
    timeout=8,
    proc=None,
    expected_data_dir=None,
    expected_consensus=None,
):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if proc is not None and proc.poll() is not None:
            raise RuntimeError(f"node {port} exited early with code {proc.returncode}")

        stat = status(port)
        if stat:
            if expected_data_dir is not None:
                expected_node_dir = str(Path(expected_data_dir) / str(port))
                if stat.get("data_dir") != expected_node_dir:
                    time.sleep(0.2)
                    continue
            if expected_consensus is not None and stat.get("consensus_enabled") != expected_consensus:
                time.sleep(0.2)
                continue
            return
        time.sleep(0.2)
    raise RuntimeError(f"node {port} did not start")


def wait_for_all(ports, timeout=8, expected_data_dir=None, expected_consensus=None):
    for port in ports:
        wait_for_node(
            port,
            timeout=timeout,
            expected_data_dir=expected_data_dir,
            expected_consensus=expected_consensus,
        )


def start_node(binary: str, port: int, peers=None, env_overrides=None):
    if peers is None:
        peers = []
    if env_overrides is None:
        env_overrides = {}

    peers_file = tempfile.NamedTemporaryFile("w", delete=False, suffix=f"_{port}.json")
    json.dump([addr(peer) for peer in peers], peers_file)
    peers_file.close()
    peers_files.append(peers_file.name)

    env = os.environ.copy()
    env.update(env_overrides)

    proc = subprocess.Popen(
        [binary, str(port), peers_file.name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=env,
    )
    processes.append((proc, port))
    return proc


def stop_all():
    for proc, _ in processes:
        proc.terminate()

    for proc, _ in processes:
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()

    processes.clear()

    for path in peers_files:
        try:
            os.unlink(path)
        except OSError:
            pass
    peers_files.clear()


def snapshot_ledgers(ports, label: str):
    log(f"\n[{label}]")
    snapshots = {}
    for port in ports:
        stat = ledger_status(port)
        entries = ledger(port)
        snapshots[port] = {"status": stat, "ledger": entries}
        bodies = [tx["body"] for tx in entries]
        log(
            f":{port} len={stat['ledger_len']} hash={stat['ledger_hash']} "
            f"mempool={stat['mempool_count']} consensus={stat['consensus_enabled']} "
            f"forward_inv={stat['forward_inv_enabled']} txs={bodies}"
        )
    return snapshots


def start_consensus_cluster(binary: str, ports, data_dir: str, round_secs="2"):
    env = {
        "P2P_CONSENSUS": "1",
        "P2P_FORWARD_INV": "0",
        "P2P_DATA_DIR": data_dir,
        "P2P_ROUND_SECS": round_secs,
    }

    proc = start_node(binary, ports[0], env_overrides=env)
    wait_for_node(
        ports[0],
        proc=proc,
        expected_data_dir=data_dir,
        expected_consensus=True,
    )

    for port in ports[1:]:
        proc = start_node(binary, port, peers=[ports[0]], env_overrides=env)
        wait_for_node(
            port,
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        time.sleep(0.2)

    wait_for_all(ports, expected_data_dir=data_dir, expected_consensus=True)
    return env


def wait_same_ledger(ports, expected_len=None, timeout=35):
    deadline = time.time() + timeout
    last_snapshots = None

    while time.time() < deadline:
        try:
            snapshots = {
                port: {"status": ledger_status(port), "ledger": ledger(port)}
                for port in ports
            }
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
            time.sleep(0.5)
            continue

        last_snapshots = snapshots
        hashes = {snapshots[port]["status"]["ledger_hash"] for port in ports}
        ledgers = {json.dumps(snapshots[port]["ledger"], sort_keys=True) for port in ports}
        lengths = {snapshots[port]["status"]["ledger_len"] for port in ports}

        expected_reached = expected_len is None or lengths == {expected_len}
        if len(hashes) == 1 and len(ledgers) == 1 and expected_reached:
            return snapshots

        time.sleep(0.5)

    if last_snapshots is not None:
        log("\n[last convergence snapshot]")
        for port, snapshot in last_snapshots.items():
            stat = snapshot["status"]
            bodies = [tx["body"] for tx in snapshot["ledger"]]
            log(
                f":{port} len={stat['ledger_len']} hash={stat['ledger_hash']} "
                f"round={stat['next_round']} txs={bodies}"
            )

    raise AssertionError("nodes did not converge to the same ledger before timeout")


def test_divergence(binary: str, base_port: int):
    ports = [base_port + i for i in range(3)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-divergence-{run_id}"
    env = {
        "P2P_CONSENSUS": "0",
        "P2P_FORWARD_INV": "0",
        "P2P_DATA_DIR": data_dir,
        "P2P_ROUND_SECS": "3",
    }

    log("Prax2 no-consensus divergence experiment")
    log(f"binary={binary}")
    log(f"ports={ports}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, ports[0], env_overrides=env)
        wait_for_node(
            ports[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=False,
        )

        for port in ports[1:]:
            proc = start_node(binary, port, peers=[ports[0]], env_overrides=env)
            wait_for_node(
                port,
                proc=proc,
                expected_data_dir=data_dir,
                expected_consensus=False,
            )

        time.sleep(1)
        snapshot_ledgers(ports, "initial")

        for port in ports:
            tx = post_tx(port, f"divergence transaction from {port}")
            log(f"posted tx to :{port}: {tx['id']}")

        time.sleep(1)
        snapshots = snapshot_ledgers(ports, "after local transactions")
        hashes = {snapshots[port]["status"]["ledger_hash"] for port in ports}
        ledgers = {json.dumps(snapshots[port]["ledger"], sort_keys=True) for port in ports}

        passed = len(hashes) >= 2 and len(ledgers) >= 2
        log(f"\nunique ledger hashes={len(hashes)}")
        log(f"unique ledgers={len(ledgers)}")
        log(f"RESULT: {'PASSED' if passed else 'FAILED'}")

        if not passed:
            raise AssertionError("expected at least two nodes to have different ledgers")
    finally:
        stop_all()


def test_converge(binary: str, base_port: int):
    ports = [base_port + 100 + i for i in range(5)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-converge-{run_id}"
    env = {
        "P2P_CONSENSUS": "1",
        "P2P_FORWARD_INV": "0",
        "P2P_DATA_DIR": data_dir,
        "P2P_ROUND_SECS": "2",
    }

    log("Prax2 consensus convergence experiment")
    log(f"binary={binary}")
    log(f"ports={ports}")
    log(f"data_dir={data_dir}")

    try:
        start_consensus_cluster(binary, ports, data_dir)
        log("waiting for peer discovery and empty consensus rounds...")
        time.sleep(10)
        snapshot_ledgers(ports, "initial")

        for port in ports[:3]:
            tx = post_tx(port, f"convergence transaction from {port}")
            log(f"posted tx to :{port}: {tx['id']}")

        snapshots = wait_same_ledger(ports, expected_len=3, timeout=40)
        snapshot_ledgers(ports, "after convergence")

        hashes = {snapshots[port]["status"]["ledger_hash"] for port in ports}
        rounds = {port: snapshots[port]["status"]["next_round"] for port in ports}
        log(f"\nconverged ledger hash={next(iter(hashes))}")
        log(f"next_round_by_port={rounds}")
        log("RESULT: PASSED")
    finally:
        stop_all()


def test_invalid(binary: str, base_port: int):
    ports = [base_port + 200 + i for i in range(3)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-invalid-{run_id}"
    invalid_body = "invalid transaction should not commit"

    log("Prax2 invalid transaction experiment")
    log(f"binary={binary}")
    log(f"ports={ports}")
    log(f"data_dir={data_dir}")

    try:
        start_consensus_cluster(binary, ports, data_dir)
        time.sleep(6)
        snapshot_ledgers(ports, "initial")

        invalid_tx = {
            "id": "not-the-real-id",
            "origin": addr(ports[0]),
            "seq": 1,
            "body": invalid_body,
        }
        status_code, response = post_inv_raw(ports[1], invalid_tx)
        log(f"posted invalid /inv to :{ports[1]} status={status_code} response={response}")

        valid_tx = post_tx(ports[0], "valid transaction after invalid inv")
        log(f"posted valid tx to :{ports[0]}: {valid_tx['id']}")

        snapshots = wait_same_ledger(ports, expected_len=1, timeout=35)
        snapshot_ledgers(ports, "after consensus")

        for port, snapshot in snapshots.items():
            bodies = [tx["body"] for tx in snapshot["ledger"]]
            if invalid_body in bodies:
                raise AssertionError(f"invalid transaction committed on {port}")

        log("RESULT: PASSED")
    finally:
        stop_all()


def test_no_quorum(binary: str, base_port: int):
    port = base_port + 300
    phantom_ports = [port + i for i in range(1, 5)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-no-quorum-{run_id}"
    env = {
        "P2P_CONSENSUS": "1",
        "P2P_FORWARD_INV": "0",
        "P2P_DATA_DIR": data_dir,
        "P2P_ROUND_SECS": "2",
    }

    log("Prax2 no-quorum experiment")
    log(f"binary={binary}")
    log(f"port={port}")
    log(f"phantom_peers={phantom_ports}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, port, peers=phantom_ports, env_overrides=env)
        wait_for_node(
            port,
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )

        tx = post_tx(port, "transaction without reachable quorum")
        log(f"posted tx to :{port}: {tx['id']}")

        time.sleep(5)
        stat = ledger_status(port)
        entries = ledger(port)
        log(
            f":{port} len={stat['ledger_len']} hash={stat['ledger_hash']} "
            f"mempool={stat['mempool_count']} next_round={stat['next_round']} "
            f"peers={stat['peers']}"
        )

        if stat["ledger_len"] != 0 or entries:
            raise AssertionError("node committed despite lacking reachable quorum")
        if stat["mempool_count"] != 1:
            raise AssertionError("pending transaction left the mempool without a commit")

        log("RESULT: PASSED")
    finally:
        stop_all()


def test_partition(binary: str, base_port: int):
    group_a = [base_port + 400 + i for i in range(3)]
    group_b = [base_port + 410 + i for i in range(2)]
    ports = group_a + group_b
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-partition-{run_id}"
    env = {
        "P2P_CONSENSUS": "1",
        "P2P_FORWARD_INV": "0",
        "P2P_DATA_DIR": data_dir,
        "P2P_ROUND_SECS": "2",
    }

    log("Prax2 partition experiment")
    log(f"binary={binary}")
    log(f"group_a={group_a}")
    log(f"group_b={group_b}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, group_a[0], env_overrides=env)
        wait_for_node(
            group_a[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        for port in group_a[1:]:
            proc = start_node(binary, port, peers=[group_a[0]], env_overrides=env)
            wait_for_node(
                port,
                proc=proc,
                expected_data_dir=data_dir,
                expected_consensus=True,
            )

        proc = start_node(binary, group_b[0], env_overrides=env)
        wait_for_node(
            group_b[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        for port in group_b[1:]:
            proc = start_node(binary, port, peers=[group_b[0]], env_overrides=env)
            wait_for_node(
                port,
                proc=proc,
                expected_data_dir=data_dir,
                expected_consensus=True,
            )

        log("waiting for peer discovery inside each partition...")
        time.sleep(8)
        snapshot_ledgers(ports, "initial partitions")

        tx_a = post_tx(group_a[0], "partition A transaction")
        tx_b = post_tx(group_b[0], "partition B transaction")
        log(f"posted tx to partition A :{group_a[0]}: {tx_a['id']}")
        log(f"posted tx to partition B :{group_b[0]}: {tx_b['id']}")

        snapshots_a = wait_same_ledger(group_a, expected_len=1, timeout=30)
        snapshots_b = wait_same_ledger(group_b, expected_len=1, timeout=30)
        snapshot_ledgers(ports, "after partitioned consensus")

        hash_a = next(iter({snapshots_a[port]["status"]["ledger_hash"] for port in group_a}))
        hash_b = next(iter({snapshots_b[port]["status"]["ledger_hash"] for port in group_b}))
        log(f"\npartition_a_hash={hash_a}")
        log(f"partition_b_hash={hash_b}")

        if hash_a == hash_b:
            raise AssertionError("partitioned groups unexpectedly produced the same ledger hash")

        log("RESULT: PASSED")
    finally:
        stop_all()


def parse_args():
    parser = argparse.ArgumentParser(description="Run Prax2 experiments")
    parser.add_argument("binary", nargs="?", default=DEFAULT_BINARY)
    parser.add_argument("--base-port", type=int, default=DEFAULT_BASE_PORT)
    parser.add_argument("--divergence", action="store_true")
    parser.add_argument("--converge", action="store_true")
    parser.add_argument("--invalid", action="store_true")
    parser.add_argument("--no-quorum", action="store_true")
    parser.add_argument("--partition", action="store_true")
    return parser.parse_args()


def main():
    args = parse_args()
    if not os.path.exists(args.binary):
        raise SystemExit(f"binary not found: {args.binary}; run cargo build first")

    selected = (
        args.divergence
        or args.converge
        or args.invalid
        or args.no_quorum
        or args.partition
    )
    if args.divergence or not selected:
        test_divergence(args.binary, args.base_port)
        save_results("divergence")
    if args.converge:
        test_converge(args.binary, args.base_port)
        save_results("converge")
    if args.invalid:
        test_invalid(args.binary, args.base_port)
        save_results("invalid")
    if args.no_quorum:
        test_no_quorum(args.binary, args.base_port)
        save_results("no_quorum")
    if args.partition:
        test_partition(args.binary, args.base_port)
        save_results("partition")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        stop_all()
        sys.exit(130)
