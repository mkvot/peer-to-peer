#!/usr/bin/env python3

from pathlib import Path
import random
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, distinct_tip_count, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show convergence can be delayed under many nodes and concurrent requests.",
        default_difficulty=4,
    )
    parser.add_argument("--nodes", type=int, default=50)
    parser.add_argument("--duration", type=int, default=120)
    parser.add_argument("--tx-interval", type=float, default=0.2)
    parser.add_argument("--miners", type=int, default=10)
    args = parser.parse_args()
    ctx = Scenario(
        "07_overload_failure",
        "many nodes submit transactions while several miners compete",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, args.nodes)
    miners = ports[: min(args.miners, args.nodes)]
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=(
            f"plan: start {args.nodes} nodes, use {len(miners)} miners, "
            f"submit tx every {args.tx_interval}s for {args.duration}s"
        ),
    )
    ctx.install_signal_handlers()

    stop_tx = threading.Event()
    rejected = {"count": 0}

    def tx_loop():
        wallets = {port: ctx.wallet(port) for port in ports[: min(len(ports), 20)]}
        counter = 0
        while not stop_tx.is_set():
            sender = random.choice(miners)
            recipient_port = random.choice(list(wallets.keys()))
            tx = ctx.create_transaction(
                sender,
                wallets[recipient_port],
                amount=1,
                memo=f"overload {counter}",
            )
            if tx is None:
                rejected["count"] += 1
            counter += 1
            time.sleep(args.tx_interval)

    try:
        ctx.start_seeded(ports)
        for miner in miners:
            ctx.mine_async(miner, 2, max_txs=20)
        ctx.wait_until(
            lambda snapshots: all(snapshots[miner]["height"] >= 1 for miner in miners if snapshots[miner]["online"]),
            timeout=180,
            phase="funding: miners are creating initial rewards",
        )

        tx_thread = threading.Thread(target=tx_loop, daemon=True)
        tx_thread.start()
        for miner in miners:
            ctx.mine_async(miner, max(3, args.duration // 8), max_txs=50)
        snapshots = ctx.run_for(args.duration, "overload: requests and mining are concurrent")
        stop_tx.set()
        tx_thread.join(timeout=2)
        snapshots = ctx.statuses()
        tips = distinct_tip_count(snapshots)
        heights = sorted({snap["height"] for snap in snapshots.values() if snap["online"]})
        mempool_total = sum(snap["mempool"] or 0 for snap in snapshots.values() if snap["online"])
        converged = all_same_tip(snapshots)
        result = "PASS" if converged else "EXPECTED FAILURE"
        ctx.final(
            result,
            (
                f"online={sum(1 for snap in snapshots.values() if snap['online'])}/{len(snapshots)}, "
                f"distinct_heights={heights}, distinct_tips={tips}, mempool_total={mempool_total}, "
                f"rejected_requests={rejected['count']}"
            ),
        )
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        stop_tx.set()
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
