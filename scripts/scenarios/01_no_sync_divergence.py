#!/usr/bin/env python3

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, common_parser, distinct_tip_count, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show that isolated mined nodes diverge without block sync.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "01_no_sync_divergence",
        "isolated nodes mine separate valid branches",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 3)
    targets = [8, 5, 11] if not args.long else [16, 10, 22]
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=f"plan: start 3 isolated nodes and mine heights {targets}",
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_isolated(ports)
        for port, blocks in zip(ports, targets):
            ctx.mine_async(port, blocks)

        done = ctx.wait_until(
            lambda snapshots: all(
                snapshots[port]["online"] and snapshots[port]["height"] >= target
                for port, target in zip(ports, targets)
            ),
            timeout=180,
            phase="isolated: nodes are mining without peers",
        )
        snapshots = ctx.statuses()
        tips = distinct_tip_count(snapshots)
        result = "PASS" if done and tips >= 2 else "EXPECTED FAILURE"
        ctx.final(result, f"observed {tips} distinct canonical tips")
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
