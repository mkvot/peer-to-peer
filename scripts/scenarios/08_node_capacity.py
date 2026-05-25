#!/usr/bin/env python3

from pathlib import Path
import random
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, distinct_tip_count, ports_from, wait_for_enter


def normal_transaction_count(blocks):
    return sum(
        1
        for block in blocks
        for tx in block.get("transactions", [])
        if tx.get("payload", {}).get("from") != "0"
    )


def main():
    parser = common_parser(
        "Measure node-count capacity while transfers and competing mining occur together.",
        default_difficulty=4,
    )
    parser.add_argument("--nodes", type=int, default=20)
    parser.add_argument("--miners", type=int, default=4)
    parser.add_argument("--funding-blocks", type=int, default=12)
    parser.add_argument("--duration", type=int, default=20)
    parser.add_argument("--tx-interval", type=float, default=0.2)
    parser.add_argument("--work-blocks", type=int, default=10)
    parser.add_argument("--startup-timeout", type=int, default=60)
    parser.add_argument("--settle-timeout", type=int, default=60)
    args = parser.parse_args()
    if args.nodes < 2:
        parser.error("--nodes must be at least 2")
    if args.miners < 1:
        parser.error("--miners must be greater than zero")

    ctx = Scenario(
        "08_node_capacity",
        f"{args.nodes} nodes process transfers during competing mining",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, args.nodes)
    source = ports[0]
    miners = ports[: min(args.miners, args.nodes)]
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=(
            f"plan: fund node {source} with {args.funding_blocks} blocks, then submit transfers "
            f"every {args.tx_interval}s while {len(miners)} miners each mine "
            f"{args.work_blocks} blocks for {args.duration}s"
        ),
    )
    ctx.install_signal_handlers()

    stop_tx = threading.Event()
    counters = {"submitted": 0, "rejected": 0}

    try:
        startup_started = time.time()
        ctx.start_seeded(ports, timeout=args.startup_timeout)
        startup_seconds = time.time() - startup_started

        ctx.mine_async(source, args.funding_blocks)
        funded = ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and all(
                snapshot["height"] >= args.funding_blocks
                for snapshot in snapshots.values()
                if snapshot["online"]
            ),
            timeout=max(120, args.funding_blocks * 30),
            phase="funding: source miner creates spendable rewards",
        )
        if not funded:
            ctx.final("EXPECTED FAILURE", "nodes did not converge after the funding phase")
            wait_for_enter("Press Enter to stop nodes and clean up...")
            return

        recipients = [ctx.wallet(port) for port in ports[1:]]

        def tx_loop():
            while not stop_tx.is_set():
                recipient = random.choice(recipients)
                tx = ctx.create_transaction(
                    source,
                    recipient,
                    amount=1,
                    memo=f"capacity transfer {counters['submitted']}",
                )
                if tx is None:
                    counters["rejected"] += 1
                else:
                    counters["submitted"] += 1
                time.sleep(args.tx_interval)

        tx_thread = threading.Thread(target=tx_loop, daemon=True)
        tx_thread.start()
        for miner in miners:
            ctx.mine_async(miner, args.work_blocks, max_txs=50)

        load_started = time.time()
        peak_tips = 0
        peak_mempool = 0
        peak_orphans = 0
        lowest_online = args.nodes
        while time.time() - load_started < args.duration:
            snapshots = ctx.statuses()
            peak_tips = max(peak_tips, distinct_tip_count(snapshots))
            peak_mempool = max(
                peak_mempool,
                sum(snapshot["mempool"] or 0 for snapshot in snapshots.values() if snapshot["online"]),
            )
            peak_orphans = max(
                peak_orphans,
                sum(snapshot["orphans"] or 0 for snapshot in snapshots.values() if snapshot["online"]),
            )
            lowest_online = min(
                lowest_online,
                sum(1 for snapshot in snapshots.values() if snapshot["online"]),
            )
            ctx.render("loaded: transfers and mining are concurrent", snapshots)
            time.sleep(2)

        stop_tx.set()
        tx_thread.join(timeout=2)
        settled = ctx.wait_until(
            lambda snapshots: all(snapshot["online"] for snapshot in snapshots.values())
            and all_same_tip(snapshots)
            and not any(snapshot["mining"] for snapshot in snapshots.values()),
            timeout=args.settle_timeout,
            phase="settling: transfer submission stopped, branches should converge",
        )

        snapshots = ctx.statuses()
        online = sum(1 for snapshot in snapshots.values() if snapshot["online"])
        mempool_total = sum(
            snapshot["mempool"] or 0 for snapshot in snapshots.values() if snapshot["online"]
        )
        blocks = ctx.chain(source)
        included = normal_transaction_count(blocks)
        working = (
            funded
            and settled
            and online == args.nodes
            and included > 0
            and lowest_online == args.nodes
        )
        ctx.final(
            "PASS" if working else "EXPECTED FAILURE",
            (
                f"nodes={args.nodes}, online={online}/{args.nodes}, "
                f"lowest_online_during_load={lowest_online}/{args.nodes}, "
                f"startup_seconds={startup_seconds:.1f}, submitted={counters['submitted']}, "
                f"included={included}, pending_total={mempool_total}, "
                f"peak_tips={peak_tips}, peak_mempool={peak_mempool}, "
                f"peak_orphans={peak_orphans}, settled={settled}"
            ),
        )
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        stop_tx.set()
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
