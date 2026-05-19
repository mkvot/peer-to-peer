#!/usr/bin/env python3

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, ports_from, wait_for_enter


def main():
    parser = common_parser(
        "Show an orphan child block connects after its missing parent arrives.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "05_orphan_block_recovery",
        "child block arrives before parent and is recovered",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 2)
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra="plan: block sync, mine two blocks, send child before parent, then send parent",
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_fully_connected(ports)
        ctx.partition([ports[0]], [ports[1]])
        ctx.mine_async(ports[0], 2)
        mined = ctx.wait_until(
            lambda snapshots: snapshots[ports[0]]["online"] and snapshots[ports[0]]["height"] >= 2,
            timeout=180,
            phase="mining: source node creates a two-block branch",
        )
        chain = ctx.chain(ports[0])
        block1, block2 = chain[1], chain[2]

        child_status, child_body = ctx.post_block(ports[1], block2)
        ctx.note("block", f"posted child first: status={child_status} body={child_body}")
        ctx.render(
            "orphaned: child is stored while parent is missing",
            ctx.statuses(),
            partition_map={ports[0]: "A", ports[1]: "B"},
        )

        parent_status, parent_body = ctx.post_block(ports[1], block1)
        ctx.note("block", f"posted parent second: status={parent_status} body={parent_body}")
        recovered = ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and all(snap["height"] >= 2 for snap in snapshots.values() if snap["online"])
            and all((snap["orphans"] or 0) == 0 for snap in snapshots.values() if snap["online"]),
            timeout=60,
            phase="recovered: parent connected and orphan was validated",
            partition_map={ports[0]: "A", ports[1]: "B"},
        )
        result = "PASS" if mined and child_body.get("status") == "orphan" and recovered else "EXPECTED FAILURE"
        ctx.final(result, f"child response={child_body}, parent response={parent_body}")
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
