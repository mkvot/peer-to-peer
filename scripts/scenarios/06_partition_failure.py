#!/usr/bin/env python3

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, common_parser, distinct_tip_count, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show that global convergence cannot happen while the network stays partitioned.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "06_partition_failure",
        "two partitions keep separate canonical tips",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 6)
    left = ports[:3]
    right = ports[3:]
    partition_map = {port: "A" for port in left} | {port: "B" for port in right}
    blocks = 8 if not args.long else 16
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=f"plan: split into {left} and {right}, mine {blocks} blocks in each, do not heal",
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_fully_connected(ports)
        ctx.partition(left, right)
        ctx.mine_async(left[0], blocks)
        ctx.mine_async(right[0], blocks)
        diverged = ctx.wait_until(
            lambda snapshots: all(snap["online"] for snap in snapshots.values())
            and all(snapshots[port]["height"] >= blocks for port in [left[0], right[0]])
            and distinct_tip_count(snapshots) >= 2,
            timeout=240,
            phase="partitioned: both sides mine but cannot globally converge",
            partition_map=partition_map,
        )
        ctx.final(
            "EXPECTED FAILURE" if diverged else "PASS",
            "network remained split with multiple tips" if diverged else "network converged despite configured partition",
        )
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
