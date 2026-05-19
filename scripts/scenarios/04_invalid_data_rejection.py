#!/usr/bin/env python3

from pathlib import Path
import hashlib
import json
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lib import Scenario, all_same_tip, common_parser, ports_from, wait_for_enter


def canonical(value):
    return json.dumps(value, separators=(",", ":"))


def sha256(text):
    return hashlib.sha256(text.encode()).hexdigest()


def tx_id(payload, signature=""):
    return sha256(canonical(payload) + signature)


def merkle_root(txs):
    if not txs:
        return sha256("")
    level = [tx["id"] for tx in txs]
    while len(level) > 1:
        if len(level) % 2 == 1:
            level.append(level[-1])
        level = [sha256(level[i] + level[i + 1]) for i in range(0, len(level), 2)]
    return level[0]


def reward_tx(creator, timestamp):
    payload = {
        "from": "0",
        "to": creator,
        "amount": 1,
        "timestamp": timestamp,
        "memo": "block reward",
    }
    return {"id": tx_id(payload, ""), "payload": payload, "signature": ""}


def header_hash(header):
    return sha256(canonical(header))


def make_block(parent, creator, difficulty, want_pow=True, wrong_hash=False, bad_merkle=False):
    timestamp = max(int(time.time() * 1000), parent["header"]["timestamp"] + 1)
    txs = [reward_tx(creator, timestamp)]
    actual_root = merkle_root(txs)
    root = sha256("bad merkle root") if bad_merkle else actual_root
    header = {
        "height": parent["header"]["height"] + 1,
        "previous_hash": parent["hash"],
        "timestamp": timestamp,
        "nonce": 0,
        "difficulty": difficulty,
        "creator": creator,
        "merkle_root": root,
        "tx_count": len(txs),
    }
    target = "0" * difficulty
    while True:
        block_hash = header_hash(header)
        valid = block_hash.startswith(target)
        if valid == want_pow:
            break
        header["nonce"] += 1
    if wrong_hash:
        block_hash = ("f" if block_hash[0] != "f" else "e") + block_hash[1:]
    return {"header": header, "hash": block_hash, "transactions": txs}


def main():
    parser = common_parser(
        "Show invalid transactions and blocks are rejected.",
        default_difficulty=4,
    )
    args = parser.parse_args()
    ctx = Scenario(
        "04_invalid_data_rejection",
        "bad data is rejected before one valid block is mined",
        args,
        __file__,
    )
    ports = ports_from(args.base_port, 3)
    ctx.prepare()
    ctx.print_plan(
        ports,
        extra="plan: submit bad transaction, bad hash, bad proof, bad Merkle root, then mine a valid block",
    )
    ctx.install_signal_handlers()

    try:
        ctx.start_fully_connected(ports)
        wallet = ctx.wallet(ports[0])
        _, chain = ctx.get(ports[0], "/chain")
        parent = chain[-1]

        bad_tx = {
            "id": "not-the-real-id",
            "payload": {
                "from": wallet,
                "to": wallet,
                "amount": 1,
                "timestamp": int(time.time() * 1000),
                "memo": "invalid signature",
            },
            "signature": "00",
        }
        checks = []
        status, body = ctx.post(ports[0], "/transactions", bad_tx)
        checks.append(("bad transaction", status, body))

        for label, block in [
            ("wrong hash", make_block(parent, wallet, args.difficulty, want_pow=False, wrong_hash=True)),
            ("insufficient proof", make_block(parent, wallet, args.difficulty, want_pow=False)),
            ("bad merkle", make_block(parent, wallet, args.difficulty, want_pow=True, bad_merkle=True)),
        ]:
            status, body = ctx.post_block(ports[0], block)
            checks.append((label, status, body))
            ctx.note("block", f"{label} response status={status}")

        ctx.mine_async(ports[0], 1)
        valid_synced = ctx.wait_until(
            lambda snapshots: all_same_tip(snapshots)
            and all(snap["height"] >= 1 for snap in snapshots.values() if snap["online"]),
            timeout=180,
            phase="validating: invalid data rejected, valid block is syncing",
        )
        rejected = all(status >= 400 for _label, status, _body in checks)
        result = "PASS" if rejected and valid_synced else "EXPECTED FAILURE"
        detail = ", ".join(f"{label}:{status}" for label, status, _body in checks)
        ctx.final(result, f"invalid responses [{detail}], valid block synced={valid_synced}")
        wait_for_enter("Press Enter to stop nodes and clean up...")
    finally:
        ctx.stop_nodes()


if __name__ == "__main__":
    main()
