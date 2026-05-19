#!/usr/bin/env python3

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show normal convergence after mining and transaction gossip.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "02_mining_convergence",
        "connected nodes converge to one mined chain tip",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 5)
    reward_blocks = 6 if not args.long else 10
    followup_blocks = 8 if not args.long else 14
    target_height = reward_blocks + followup_blocks
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra=(
            f"plan: mine {reward_blocks} reward blocks, create transfers, "
            f"then mine {followup_blocks} more blocks"
        ),
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_fully_connected(ports)
        ctx.mine_async(ports[0], reward_blocks)
        ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and min(snap["height"] for snap in snapshots.values() if snap["online"]) >= reward_blocks,
            timeout=180,
            phase="funding: node 1 is mining spendable rewards",
        )

        recipients = [ctx.wallet(port) for port in ports[1:]]
        for index, recipient in enumerate(recipients, start=1):
            ctx.create_transaction(
                ports[0],
                recipient,
                amount=1,
                memo=f"convergence transfer {index}",
            )

        ctx.mine_async(ports[0], followup_blocks)
        converged = ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and min(snap["height"] for snap in snapshots.values() if snap["online"]) >= target_height,
            timeout=240,
            phase="converging: transfers are mined and synced",
        )
        ctx.final(
            "PASS" if converged else "EXPECTED FAILURE",
            "all online nodes share one canonical tip" if converged else "nodes did not converge before timeout",
        )
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
