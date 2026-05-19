#!/usr/bin/env python3

from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, distinct_tip_count, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show that a longer valid branch replaces a shorter valid branch.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "03_longer_chain_reorg",
        "partitioned branches rejoin and the longer branch wins",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 4)
    left = ports[:2]
    right = ports[2:]
    long_blocks = 100 if args.long else 90
    short_blocks = 50 if args.long else 45
    partition_map = {port: "A" for port in left} | {port: "B" for port in right}
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=(
            f"plan: split A={left} and B={right}, mine {long_blocks} vs {short_blocks}, "
            "then heal the partition"
        ),
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_fully_connected(ports)
        ctx.partition(left, right)
        ctx.mine_async(left[0], long_blocks)
        ctx.mine_async(right[0], short_blocks)
        started = time.time()
        estimate_written = False

        def partition_done(snapshots):
            nonlocal estimate_written
            left_height = max(snapshots[port]["height"] for port in left if snapshots[port]["online"])
            elapsed = max(1.0, time.time() - started)
            if not estimate_written and left_height >= 3:
                seconds_per_block = elapsed / left_height
                remaining = max(0, long_blocks - left_height) * seconds_per_block
                ctx.note("chain", f"estimated remaining mining time {remaining:.0f}s")
                estimate_written = True
            return (
                all(snapshots[port]["height"] >= long_blocks for port in left)
                and all(snapshots[port]["height"] >= short_blocks for port in right)
                and distinct_tip_count(snapshots) >= 2
            )

        ctx.wait_until(
            partition_done,
            timeout=600,
            phase="partitioned: two valid branches are growing",
            partition_map=partition_map,
        )
        ctx.heal(ports)
        converged = ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and min(snap["height"] for snap in snapshots.values() if snap["online"]) >= long_blocks,
            timeout=240,
            phase="healed: shorter branch is switching to the longer chain",
            partition_map=partition_map,
        )
        ctx.final(
            "PASS" if converged else "EXPECTED FAILURE",
            "all nodes converged to the longer branch" if converged else "reorg did not complete before timeout",
        )
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
