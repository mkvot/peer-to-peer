# HTTP API

Each node serves a JSON HTTP API on its configured address. The examples below
assume three local nodes and use `jq` only to format JSON and extract fields.

## Start Three Nodes

Build once:

```bash
cargo build
```

Start each command in a separate terminal:

```bash
./target/debug/peer-to-peer 9000 --difficulty 4
./target/debug/peer-to-peer 9001 --peers 127.0.0.1:9000 --difficulty 4
./target/debug/peer-to-peer 9002 --peers 127.0.0.1:9000,127.0.0.1:9001 --difficulty 4
```

In a terminal for API calls:

```bash
N0=http://127.0.0.1:9000
N1=http://127.0.0.1:9001
N2=http://127.0.0.1:9002
```

Nodes in one network must use the same difficulty. A normal transfer uses
wallet public keys, not node ports, as its `to` and `from` account identifiers.

## Basic Transfer Flow

Check that all nodes answer and inspect their public keys:

```bash
curl -sS "$N0/status" | jq
curl -sS "$N1/status" | jq
curl -sS "$N2/status" | jq

ALICE="$(curl -sS "$N0/wallet" | jq -r '.public_key')"
BOB="$(curl -sS "$N1/wallet" | jq -r '.public_key')"
printf 'alice=%s\nbob=%s\n' "$ALICE" "$BOB"
```

Node `9000` begins with no coins. Mine five reward-only blocks so its wallet
has a balance of `5` on the canonical chain:

```bash
curl -sS -X POST "$N0/mine" \
  -H 'Content-Type: application/json' \
  -d '{"blocks":5,"max_txs":50}' | jq

sleep 1
curl -sS "$N0/balances" | jq
curl -sS "$N1/balances" | jq
```

Create a signed transfer from node `9000` to node `9001`. The creating node
puts it in its mempool and broadcasts it to known peers:

```bash
curl -sS -X POST "$N0/transactions/create" \
  -H 'Content-Type: application/json' \
  -d "{\"to\":\"$BOB\",\"amount\":2,\"memo\":\"payment to bob\"}" | jq

sleep 1
curl -sS "$N1/transactions" | jq
```

The transfer is pending until a miner includes it in a block. Any node can
mine that block; the reward goes to the miner while the transfer still moves
coins from `9000`'s wallet to `9001`'s wallet:

```bash
curl -sS -X POST "$N2/mine" \
  -H 'Content-Type: application/json' \
  -d '{"blocks":1,"max_txs":50}' | jq

sleep 1
curl -sS "$N0/chain/status" | jq
curl -sS "$N0/balances" | jq
curl -sS "$N0/transactions" | jq
```

After propagation, the balances show `9000` spent two of its rewards,
`9001` received two coins, and `9002` received one mining reward.

## Endpoint Summary

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/ping` | Liveness check. |
| `GET` | `/status` | Compact node and selected-chain status. |
| `GET` | `/peers` | Known peer addresses. |
| `POST` | `/peers` | Add peer addresses. |
| `GET` | `/wallet` | Local node public key. |
| `POST` | `/transactions/create` | Create, sign, store, and broadcast a local transfer. |
| `POST` | `/transactions` | Submit an already signed transfer. |
| `GET` | `/transactions` | Current pending mempool transactions. |
| `POST` | `/mine` | Mine one or more blocks on the current selected tip. |
| `GET` | `/mining/status` | Current or last mining job state. |
| `POST` | `/blocks` | Submit a complete mined block. |
| `GET` | `/blocks/{hash}` | Fetch one known block by full hash. |
| `GET` | `/hashes` | Canonical-chain hashes, genesis first. |
| `GET` | `/chain` | Canonical-chain blocks, genesis first. |
| `GET` | `/chain/status` | Canonical-chain counts and tip. |
| `GET` | `/balances` | Balances at the current canonical tip. |
| `GET` | `/events` | Recent in-memory node events. |
| `POST` | `/debug/faults` | Disable or restore communication with selected peers. |

All successful responses are JSON. Invalid transaction/block input generally
returns HTTP `400` with an `error` field.

## Node And Peer Endpoints

### `GET /ping`

```bash
curl -sS "$N0/ping" | jq
```

Response:

```json
{}
```

### `GET /status`

```bash
curl -sS "$N0/status" | jq
```

Example shape:

```json
{
  "addr": "127.0.0.1:9000",
  "height": 5,
  "tip": "<full-block-hash>",
  "mempool": 0,
  "peers": 2,
  "orphans": 0,
  "mining": false,
  "difficulty": 4
}
```

`height` and `tip` describe the selected canonical chain. `mempool` is the
number of pending normal transfers. `orphans` is the number of received blocks
whose parent block is not known yet.

### `GET /peers`

```bash
curl -sS "$N0/peers" | jq
```

Response shape:

```json
["127.0.0.1:9001", "127.0.0.1:9002"]
```

### `POST /peers`

Add one peer:

```bash
curl -sS -X POST "$N0/peers" \
  -H 'Content-Type: application/json' \
  -d '{"address":"127.0.0.1:9001"}' | jq
```

Add multiple peers:

```bash
curl -sS -X POST "$N0/peers" \
  -H 'Content-Type: application/json' \
  -d '["127.0.0.1:9001","127.0.0.1:9002"]' | jq
```

The response is the node's resulting peer-address array.

## Wallet And Transaction Endpoints

### `GET /wallet`

```bash
curl -sS "$N0/wallet" | jq
```

Response shape:

```json
{
  "public_key": "<hex-public-key>"
}
```

Only the public key is exposed through the API. It is used as a transfer
recipient address and as the key in `/balances`.

### `POST /transactions/create`

The local node creates and signs a transfer using its private wallet, stores it
in its mempool, and broadcasts it.

```bash
BOB="$(curl -sS "$N1/wallet" | jq -r '.public_key')"

curl -sS -X POST "$N0/transactions/create" \
  -H 'Content-Type: application/json' \
  -d "{\"to\":\"$BOB\",\"amount\":1,\"memo\":\"hello\"}" | jq
```

Request body:

```json
{
  "to": "<recipient-public-key>",
  "amount": 1,
  "memo": "hello"
}
```

`memo` may be omitted. `amount` must be a positive integer. The transaction
can enter the mempool before it is spendable; miners include it only when the
sender has sufficient canonical-chain balance.

Response shape:

```json
{
  "id": "<transaction-id>",
  "payload": {
    "from": "<sender-public-key>",
    "to": "<recipient-public-key>",
    "amount": 1,
    "timestamp": 0,
    "memo": "hello"
  },
  "signature": "<signature>"
}
```

### `POST /transactions`

Submit a fully formed signed transaction, normally for node-to-node gossip or
testing validation. Reposting an accepted transaction is not an error.

```bash
TX="$(curl -sS -X POST "$N0/transactions/create" \
  -H 'Content-Type: application/json' \
  -d "{\"to\":\"$BOB\",\"amount\":1,\"memo\":\"relay test\"}")"

curl -sS -X POST "$N1/transactions" \
  -H 'Content-Type: application/json' \
  -d "$TX" | jq
```

Response:

```json
{"status":"accepted"}
```

or:

```json
{"status":"already_known"}
```

### `GET /transactions`

Returns pending transactions in the local mempool, not transactions already in
the selected chain.

```bash
curl -sS "$N0/transactions" | jq
```

## Mining And Chain Endpoints

### `POST /mine`

Mining creates one reward transaction for the mining node in every block, then
adds up to `max_txs` valid pending normal transfers. The request returns after
the requested mining work completes.

Mine one reward-only block when no normal transaction is pending:

```bash
curl -sS -X POST "$N0/mine" \
  -H 'Content-Type: application/json' \
  -d '{"blocks":1,"max_txs":50}' | jq
```

Mine three blocks:

```bash
curl -sS -X POST "$N0/mine" \
  -H 'Content-Type: application/json' \
  -d '{"blocks":3,"max_txs":50}' | jq
```

An empty request body mines one block with `max_txs` defaulting to `50`:

```bash
curl -sS -X POST "$N0/mine" | jq
```

Response shape:

```json
{
  "status": "mined",
  "count": 1,
  "blocks": ["<new-block-hash>"]
}
```

With `blocks: 1`, the new block is broadcast to known, unblocked peers when
the request completes. With `blocks` greater than `1`, this implementation
mines the requested batch locally and broadcasts its blocks after the whole
request completes, rather than immediately after each block in the batch.

### `GET /mining/status`

Use this from another terminal while a `/mine` request is running:

```bash
watch -n 0.5 "curl -sS '$N0/mining/status' | jq"
```

Response shape:

```json
{
  "active": true,
  "current_height": 6,
  "candidate_parent": "<parent-hash>",
  "attempts": 100001,
  "last_hash": "<last-attempt-hash>",
  "last_mined_hash": "<last-successful-hash>",
  "started_at_ms": 0
}
```

`attempts` is the number of nonces attempted for the current candidate block.
If another accepted block changes the preferred parent while mining, mining
restarts on the new selected tip.

### `GET /chain/status`

```bash
curl -sS "$N0/chain/status" | jq
```

Response shape:

```json
{
  "height": 6,
  "tip": "<full-block-hash>",
  "total_transactions": 8,
  "known_blocks": 7,
  "orphans": 0,
  "mempool": 0
}
```

`total_transactions` includes mining reward transactions. `known_blocks` may
be greater than the canonical chain length because a node stores valid forks.

### `GET /hashes`

Returns full hashes of the selected chain, starting with genesis:

```bash
curl -sS "$N0/hashes" | jq
```

### `GET /chain`

Returns the complete blocks of the selected chain, starting with genesis:

```bash
curl -sS "$N0/chain" | jq
```

Inspect only the selected tip:

```bash
curl -sS "$N0/chain" | jq '.[-1]'
```

### `GET /blocks/{hash}`

```bash
TIP="$(curl -sS "$N0/status" | jq -r '.tip')"
curl -sS "$N0/blocks/$TIP" | jq
```

Returns HTTP `404` if that block is not known to the addressed node.

### `POST /blocks`

Submit a complete mined block. This is normally used by block gossip; it is
also useful when manually demonstrating block propagation.

On fresh nodes, after mining exactly one new block on `N0`, forward its tip to
an isolated node that has the same genesis block:

```bash
BLOCK="$(curl -sS "$N0/chain" | jq -c '.[-1]')"

curl -sS -X POST "$N1/blocks" \
  -H 'Content-Type: application/json' \
  -d "$BLOCK" | jq
```

Possible successful statuses:

```json
{"status":"added"}
{"status":"duplicate"}
{"status":"orphan"}
```

`orphan` means the block itself was structurally acceptable, but its parent
has not arrived at that node. When the missing parent arrives, stored children
are connected and validated.

### `GET /balances`

Returns canonical-chain balances keyed by wallet public key:

```bash
curl -sS "$N0/balances" | jq
curl -sS "$N0/balances" | jq --arg key "$ALICE" '.[$key] // 0'
curl -sS "$N0/balances" | jq --arg key "$BOB" '.[$key] // 0'
```

## Observation And Fault Endpoints

### `GET /events`

Returns up to the most recent 200 events held in memory by the node:

```bash
curl -sS "$N0/events" | jq
curl -sS "$N0/events" | jq '.[-10:]'
```

Events include transaction acceptance/rejection, mined and received blocks,
chain reorganization, and peer blocking.

### `POST /debug/faults`

This endpoint is intended for controlled fault experiments. A blocked peer is
excluded from this node's outgoing synchronization and transaction/block
gossip. To create a two-way partition, block each direction.

Separate nodes `9000` and `9001`:

```bash
curl -sS -X POST "$N0/debug/faults" \
  -H 'Content-Type: application/json' \
  -d '{"block_peer":"127.0.0.1:9001"}' | jq

curl -sS -X POST "$N1/debug/faults" \
  -H 'Content-Type: application/json' \
  -d '{"block_peer":"127.0.0.1:9000"}' | jq
```

Restore the connection:

```bash
curl -sS -X POST "$N0/debug/faults" \
  -H 'Content-Type: application/json' \
  -d '{"unblock_peer":"127.0.0.1:9001"}' | jq

curl -sS -X POST "$N1/debug/faults" \
  -H 'Content-Type: application/json' \
  -d '{"unblock_peer":"127.0.0.1:9000"}' | jq
```

Clear every block on one node:

```bash
curl -sS -X POST "$N0/debug/faults" \
  -H 'Content-Type: application/json' \
  -d '{"clear_blocked_peers":true}' | jq
```

Response shape:

```json
{
  "status": "ok",
  "blocked_peers": ["127.0.0.1:9001"]
}
```
