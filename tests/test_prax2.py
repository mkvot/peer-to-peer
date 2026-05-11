#!/usr/bin/env python3
"""
Prax2 experiment harness.

Usage:
    python3 tests/test_prax2.py --divergence
    python3 tests/test_prax2.py --converge
    python3 tests/test_prax2.py --leader-failure
    python3 tests/test_prax2.py --load
    python3 tests/test_prax2.py ./target/release/peer-to-peer --converge
"""

import argparse
import json
import os
import random
import subprocess
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path


DEFAULT_BINARY = "./target/debug/peer-to-peer"
DEFAULT_BASE_PORT = 9500
TIMEOUT = 3

processes = []
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


def start_node(binary: str, port: int, peers=None, args=None):
    if peers is None:
        peers = []
    if args is None:
        args = []

    cmd = [binary, str(port), *args]
    for peer in peers:
        cmd.extend(["--peer", addr(peer)])

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
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
        return

    raise RuntimeError(f"node {port} is not managed by this test run")


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


def expected_leader_from_status(stat):
    members = sorted(set([stat["addr"]] + stat["peers"]))
    if not members:
        return None
    return members[stat["next_round"] % len(members)]


def start_consensus_cluster(binary: str, ports, data_dir: str, round_secs="2"):
    args = [
        "--consensus",
        "--no-forward-inv",
        "--data-dir",
        data_dir,
        "--round-secs",
        round_secs,
    ]

    proc = start_node(binary, ports[0], args=args)
    wait_for_node(
        ports[0],
        proc=proc,
        expected_data_dir=data_dir,
        expected_consensus=True,
    )

    for port in ports[1:]:
        proc = start_node(binary, port, peers=[ports[0]], args=args)
        wait_for_node(
            port,
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        time.sleep(0.2)

    wait_for_all(ports, expected_data_dir=data_dir, expected_consensus=True)
    return args


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


def collect_ledger_snapshots(ports):
    snapshots = {}
    for port in ports:
        try:
            snapshots[port] = {"status": ledger_status(port), "ledger": ledger(port)}
        except (urllib.error.URLError, TimeoutError, json.JSONDecodeError, RuntimeError):
            snapshots[port] = {
                "status": {
                    "ledger_len": -1,
                    "ledger_hash": "unreachable",
                    "commit_count": -1,
                },
                "ledger": [],
            }
    return snapshots


def test_divergence(binary: str, base_port: int):
    ports = [base_port + i for i in range(3)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-divergence-{run_id}"
    args = [
        "--no-consensus",
        "--no-forward-inv",
        "--data-dir",
        data_dir,
        "--round-secs",
        "3",
    ]

    log("Prax2 no-consensus divergence experiment")
    log(f"binary={binary}")
    log(f"ports={ports}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, ports[0], args=args)
        wait_for_node(
            ports[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=False,
        )

        for port in ports[1:]:
            proc = start_node(binary, port, peers=[ports[0]], args=args)
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


def test_leader_failure(binary: str, base_port: int, expected_result: str):
    ports = [base_port + 150 + i for i in range(5)]
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    data_dir = f"/tmp/p2p-prax2-leader-failure-{run_id}"

    log("Prax2 leader-failure experiment")
    log(f"binary={binary}")
    log(f"ports={ports}")
    log(f"data_dir={data_dir}")

    try:
        start_consensus_cluster(binary, ports, data_dir)
        log("waiting for peer discovery and empty consensus rounds...")
        time.sleep(10)
        initial = snapshot_ledgers(ports, "initial")

        leaders = {
            port: expected_leader_from_status(initial[port]["status"])
            for port in ports
        }
        leader_addr = leaders[ports[0]]
        leader_port = int(leader_addr.rsplit(":", 1)[1])
        log(f"next_leader_by_port={leaders}")
        log(f"killing expected leader :{leader_port}")

        stop_node_by_port(leader_port)
        survivors = [port for port in ports if port != leader_port]

        log("waiting longer than one peer-maintenance tick after leader kill...")
        time.sleep(10)

        posted = []
        for port in survivors[:3]:
            tx = post_tx(port, f"leader failure tx from {port}")
            posted.append(tx["id"])
            log(f"posted tx to survivor :{port}: {tx['id']}")

        time.sleep(18)
        snapshots = snapshot_ledgers(survivors, "after leader failure")
        hashes = {snapshots[port]["status"]["ledger_hash"] for port in survivors}
        lengths = {snapshots[port]["status"]["ledger_len"] for port in survivors}
        rounds = {port: snapshots[port]["status"]["next_round"] for port in survivors}
        commit_counts = {port: snapshots[port]["status"]["commit_count"] for port in survivors}
        mempools = {port: snapshots[port]["status"]["mempool_count"] for port in survivors}

        recovered = len(hashes) == 1 and min(lengths) >= len(posted)
        if recovered:
            result = "recovered"
        elif max(lengths) == 0 and max(mempools.values()) > 0:
            result = "stalled"
        else:
            result = "partial_progress"

        log(f"\nleader_failure_result={result}")
        log(f"survivor_hashes={hashes}")
        log(f"survivor_lengths={lengths}")
        log(f"survivor_next_rounds={rounds}")
        log(f"survivor_commit_counts={commit_counts}")
        log(f"survivor_mempools={mempools}")

        if expected_result != "any" and result != expected_result:
            raise AssertionError(
                f"expected leader failure result {expected_result}, got {result}"
            )

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
    args = [
        "--consensus",
        "--no-forward-inv",
        "--data-dir",
        data_dir,
        "--round-secs",
        "2",
    ]

    log("Prax2 no-quorum experiment")
    log(f"binary={binary}")
    log(f"port={port}")
    log(f"phantom_peers={phantom_ports}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, port, peers=phantom_ports, args=args)
        wait_for_node(
            port,
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )

        tx = post_tx(port, "transaction without reachable quorum")
        log(f"posted tx to :{port}: {tx['id']}")

        log("waiting longer than one peer-maintenance tick...")
        time.sleep(15)
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
    args = [
        "--consensus",
        "--no-forward-inv",
        "--data-dir",
        data_dir,
        "--round-secs",
        "2",
    ]

    log("Prax2 partition experiment")
    log(f"binary={binary}")
    log(f"group_a={group_a}")
    log(f"group_b={group_b}")
    log(f"data_dir={data_dir}")

    try:
        proc = start_node(binary, group_a[0], args=args)
        wait_for_node(
            group_a[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        for port in group_a[1:]:
            proc = start_node(binary, port, peers=[group_a[0]], args=args)
            wait_for_node(
                port,
                proc=proc,
                expected_data_dir=data_dir,
                expected_consensus=True,
            )

        proc = start_node(binary, group_b[0], args=args)
        wait_for_node(
            group_b[0],
            proc=proc,
            expected_data_dir=data_dir,
            expected_consensus=True,
        )
        for port in group_b[1:]:
            proc = start_node(binary, port, peers=[group_b[0]], args=args)
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


def test_load(binary: str, base_port: int, sizes, duration: int):
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    overall_results = []

    log("Prax2 consensus load experiment")
    log(f"binary={binary}")
    log(f"sizes={sizes}")
    log(f"duration_secs={duration}")

    for index, node_count in enumerate(sizes):
        ports = [base_port + 500 + (index * 100) + i for i in range(node_count)]
        data_dir = f"/tmp/p2p-prax2-load-{run_id}-{node_count}"
        posted = 0
        failed_posts = 0
        start_time = None

        log(f"\n[load size {node_count}]")
        log(f"ports={ports[0]}..{ports[-1]}")
        log(f"data_dir={data_dir}")

        try:
            start_consensus_cluster(binary, ports, data_dir)
            log("waiting for peer discovery and empty consensus rounds...")
            time.sleep(10)

            start_time = time.time()
            deadline = start_time + duration
            while time.time() < deadline:
                port = random.choice(ports)
                body = f"load tx {posted + 1} from {port}"
                try:
                    post_tx(port, body)
                    posted += 1
                except (urllib.error.URLError, TimeoutError, RuntimeError) as error:
                    failed_posts += 1
                    if failed_posts <= 3:
                        log(f"post_failed port={port} error={error}")
                time.sleep(0.25)

            log(f"posted_transactions={posted}")
            log(f"failed_posts={failed_posts}")

            converged = False
            convergence_secs = None
            try:
                snapshots = wait_same_ledger(ports, expected_len=posted, timeout=60)
                convergence_secs = time.time() - start_time
                converged = True
            except AssertionError as error:
                log(f"convergence_result=failed: {error}")
                snapshots = collect_ledger_snapshots(ports)

            ledger_lengths = {
                port: snapshots[port]["status"]["ledger_len"] for port in ports
            }
            ledger_hashes = {
                snapshots[port]["status"]["ledger_hash"] for port in ports
            }
            commit_counts = {
                port: snapshots[port]["status"]["commit_count"] for port in ports
            }

            log(f"converged={converged}")
            if convergence_secs is not None:
                log(f"convergence_secs={convergence_secs:.1f}")
            log(f"ledger_lengths={ledger_lengths}")
            log(f"unique_ledger_hashes={len(ledger_hashes)}")
            log(f"commit_counts={commit_counts}")

            overall_results.append(
                {
                    "node_count": node_count,
                    "posted": posted,
                    "failed_posts": failed_posts,
                    "converged": converged,
                    "convergence_secs": convergence_secs,
                    "min_ledger_len": min(ledger_lengths.values()),
                    "max_ledger_len": max(ledger_lengths.values()),
                    "unique_ledger_hashes": len(ledger_hashes),
                }
            )
        finally:
            stop_all()

    log("\n[load summary]")
    for result in overall_results:
        log(json.dumps(result, sort_keys=True))

    if not overall_results or not any(result["converged"] for result in overall_results):
        raise AssertionError("load experiment did not produce any converged run")

    log("RESULT: PASSED")


def parse_load_sizes(value: str):
    sizes = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        size = int(part)
        if size < 1:
            raise argparse.ArgumentTypeError("load sizes must be positive")
        sizes.append(size)

    if not sizes:
        raise argparse.ArgumentTypeError("at least one load size is required")

    return sizes


def parse_args():
    parser = argparse.ArgumentParser(description="Run Prax2 experiments")
    parser.add_argument("binary", nargs="?", default=DEFAULT_BINARY)
    parser.add_argument("--base-port", type=int, default=DEFAULT_BASE_PORT)
    parser.add_argument("--divergence", action="store_true")
    parser.add_argument("--converge", action="store_true")
    parser.add_argument("--leader-failure", action="store_true")
    parser.add_argument(
        "--leader-failure-expect",
        choices=["any", "recovered", "stalled", "partial_progress"],
        default="stalled",
        help="expected leader-failure outcome; default documents the current no-view-change behavior",
    )
    parser.add_argument("--invalid", action="store_true")
    parser.add_argument("--no-quorum", action="store_true")
    parser.add_argument("--partition", action="store_true")
    parser.add_argument("--load", action="store_true")
    parser.add_argument(
        "--load-sizes",
        type=parse_load_sizes,
        default=parse_load_sizes("5,10,25,50"),
    )
    parser.add_argument("--load-duration", type=int, default=30)
    return parser.parse_args()


def main():
    args = parse_args()
    if not os.path.exists(args.binary):
        raise SystemExit(f"binary not found: {args.binary}; run cargo build first")

    selected = (
        args.divergence
        or args.converge
        or args.leader_failure
        or args.invalid
        or args.no_quorum
        or args.partition
        or args.load
    )
    if args.divergence or not selected:
        test_divergence(args.binary, args.base_port)
        save_results("divergence")
    if args.converge:
        test_converge(args.binary, args.base_port)
        save_results("converge")
    if args.leader_failure:
        test_leader_failure(args.binary, args.base_port, args.leader_failure_expect)
        save_results("leader_failure")
    if args.invalid:
        test_invalid(args.binary, args.base_port)
        save_results("invalid")
    if args.no_quorum:
        test_no_quorum(args.binary, args.base_port)
        save_results("no_quorum")
    if args.partition:
        test_partition(args.binary, args.base_port)
        save_results("partition")
    if args.load:
        test_load(args.binary, args.base_port, args.load_sizes, args.load_duration)
        save_results("load")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        stop_all()
        sys.exit(130)
