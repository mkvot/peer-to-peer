#!/usr/bin/env python3
"""
Prax2 experiment harness.

Usage:
    python3 test_prax2.py --divergence
    python3 test_prax2.py ./target/release/peer-to-peer --divergence
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


def wait_for_node(port: int, timeout=8):
    deadline = time.time() + timeout
    while time.time() < deadline:
        if status(port):
            return
        time.sleep(0.2)
    raise RuntimeError(f"node {port} did not start")


def wait_for_all(ports, timeout=8):
    for port in ports:
        wait_for_node(port, timeout=timeout)


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
        start_node(binary, ports[0], env_overrides=env)
        wait_for_node(ports[0])

        for port in ports[1:]:
            start_node(binary, port, peers=[ports[0]], env_overrides=env)
            wait_for_node(port)

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


def parse_args():
    parser = argparse.ArgumentParser(description="Run Prax2 experiments")
    parser.add_argument("binary", nargs="?", default=DEFAULT_BINARY)
    parser.add_argument("--base-port", type=int, default=DEFAULT_BASE_PORT)
    parser.add_argument("--divergence", action="store_true")
    return parser.parse_args()


def main():
    args = parse_args()
    if not os.path.exists(args.binary):
        raise SystemExit(f"binary not found: {args.binary}; run cargo build first")

    selected = args.divergence
    if args.divergence or not selected:
        test_divergence(args.binary, args.base_port)
        save_results("divergence")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        stop_all()
        sys.exit(130)
