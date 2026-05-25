# Peer-to-Peer Mined Ledger

Small peer-to-peer ledger using simplified Nakamoto consensus:
proof-of-work mining, signed transactions, fork storage, and deterministic longest-chain selection.

## Build

```bash
cargo build
```

## Start Nodes

```bash
./target/debug/peer-to-peer 9000
./target/debug/peer-to-peer 9001 --peers 127.0.0.1:9000
./target/debug/peer-to-peer 9002 --peers 127.0.0.1:9000,127.0.0.1:9001
```

CLI options:

```text
--peers <addr,addr>       Add one or more peers.
--bind-ip <ip>            Local bind address. Default: 127.0.0.1.
--data-dir <path>         Base directory for per-node data. Default: ledger_data.
--difficulty <n>          Leading-zero proof-of-work difficulty. Default: 4.
```

Nodes do not mine automatically. Mining starts only when `POST /mine` is called.

## Wallets And Transactions

Each node owns an Ed25519 wallet in its data directory:

```text
ledger_data/<port>/wallet.json
```

Transactions transfer integer amounts between public keys. A normal transaction signs canonical JSON of its payload, and its id is the SHA-256 hash of `canonical_json(payload) + signature`.

Create a signed transaction from the local wallet:

```bash
curl -s http://127.0.0.1:9000/wallet
curl -s -X POST http://127.0.0.1:9000/transactions/create \
  -d '{"to":"<recipient-public-key>","amount":1,"memo":"hello"}'
```

Reward transactions are created only by mining. The reward is `1`, paid to the block creator, and must be the first transaction in a mined block.

## Mining

```bash
curl -s -X POST http://127.0.0.1:9000/mine \
  -d '{"blocks":1,"max_txs":50}'
```

Mining builds a candidate on the current canonical tip, adds one reward transaction, selects valid pending transactions, and searches for a nonce whose block hash starts with the configured number of zeroes.

Progress is observable while mining:

```bash
curl -s http://127.0.0.1:9000/mining/status
```

If the canonical parent changes during mining, the node stops that candidate and restarts on the new tip.

## Consensus Rule

Nodes store forks instead of discarding them.

Fork choice is deterministic:

```text
1. higher valid height wins
2. if tied, higher total transaction count wins
3. if tied, newer tip timestamp wins
4. if tied, lexicographically smaller tip hash wins
```

Balances are derived from the canonical chain by replaying rewards and transfers from genesis to tip.

## API

Detailed endpoint documentation is in [api.md](api.md).

```text
GET  /ping
GET  /status
GET  /peers
POST /peers

GET  /wallet
POST /transactions/create
POST /transactions
GET  /transactions

POST /mine
GET  /mining/status

POST /blocks
GET  /blocks/{hash}
GET  /hashes
GET  /chain
GET  /chain/status
GET  /balances

GET  /events
POST /debug/faults
```

Blocked peers are skipped for peer sync, transaction gossip, and block gossip.

## Scenario Scripts

Each script starts node processes, triggers transaction/block activity or connectivity faults, shows a live node table, pauses before shutdown, and records the event timeline in a `.log` file beside the script. Node state from each run is saved under:

```text
ledger_runs/<scenario_name>/<node_port>/
```

```bash
python3 scripts/scenarios/01_no_sync_divergence.py
python3 scripts/scenarios/02_mining_convergence.py
python3 scripts/scenarios/03_longer_chain_reorg.py
python3 scripts/scenarios/04_invalid_data_rejection.py
python3 scripts/scenarios/05_orphan_block_recovery.py
python3 scripts/scenarios/06_partition_failure.py
python3 scripts/scenarios/07_overload_failure.py --nodes 50 --duration 120
python3 scripts/scenarios/08_node_capacity.py --nodes 20 --miners 4 --duration 20
```

All scripts accept `--difficulty <n>` and default to difficulty `4`.

### Results


| Scenario | Situation created | Actual result |
| --- | --- | --- |
| `01_no_sync_divergence.py` | Three isolated nodes independently mined heights `8`, `5`, and `11`. | `PASS`: at about `15s`, all three nodes had different canonical tips. |
| `02_mining_convergence.py` | Five connected nodes; node 9000 mined six rewards, submitted four transfers, then mined eight more blocks. | `PASS`: at about `20s`, all `5/5` nodes had the same height-`14` tip and mempool size `0`. |
| `03_longer_chain_reorg.py` | Four nodes split into two partitions; one side mined `90` blocks and the other `45`, then communication was restored. | `PASS`: two branch tips existed during the split; after healing, all `4/4` nodes selected the height-`90` branch at about `119s`. |
| `04_invalid_data_rejection.py` | Three connected nodes received a forged transaction, wrong block hash, insufficient proof-of-work, and incorrect Merkle root, followed by one valid block. | `PASS`: all four invalid requests returned HTTP `400`; the valid block synchronized to all nodes at height `1`. |
| `05_orphan_block_recovery.py` | Two nodes; a height-`2` child block was sent before its height-`1` parent. | `PASS`: receiver reported `orphans=1`, then `orphans=0` and the same height-`2` tip after its parent arrived. |
| `06_partition_failure.py` | Six nodes split into groups of three; each side mined eight blocks and the split was not healed. | `EXPECTED FAILURE`: at about `15s`, both partitions were at height `8` but retained two different tips. |
| `07_overload_failure.py --nodes 30 --miners 6 --duration 45 --tx-interval 0.2` | Thirty nodes processed concurrent transfers while six miners competed. | `EXPECTED FAILURE`: final snapshot answered for `29/30` nodes with `1786` total pending mempool entries; the responding nodes shared one height-`17` tip. |
| `08_node_capacity.py` | A funded sender submitted transfers every `0.2s` while four miners competed for ten blocks each; node count was increased between runs. | `20` nodes passed; `25` and `30` nodes eventually converged but each had an unresponsive node during active load. |

### Capacity Measurement

`08_node_capacity.py` measures loaded operation. Before measuring, node `9000` mines `12` blocks so it owns spendable funds. During the measured phase it submits a signed transfer every `0.2s` while four nodes concurrently mine ten proof-of-work blocks each. After submission stops, the run passes only when:

```text
every node responded throughout the loaded phase
at least one normal transfer was included in the canonical chain
all nodes eventually reached one canonical tip
all mining jobs completed
```

Measured with `--miners 4 --funding-blocks 12 --duration 20 --tx-interval 0.2 --work-blocks 10`:

| Nodes | Responsive throughout load | Transfers submitted / included | Peak tips | Final state | Result |
| ---: | --- | ---: | ---: | --- | --- |
| 10 | `10/10` | `86 / 20` | `5` | `10/10` settled on one tip | `PASS` |
| 20 | `20/20` | `63 / 18` | `5` | `20/20` settled on one tip | `PASS` |
| 25 | `24/25` | `51 / 18` | `7` | Eventually settled on one tip | `EXPECTED FAILURE` |
| 30 | `29/30` | `46 / 16` | `4` | Eventually settled on one tip | `EXPECTED FAILURE` |

The failure at `25` and `30` nodes is overload during activity, but not permanent ledger disagreement. One status request timed out during active transfer/mining load, but the network later converged after the work stopped. Pending mempool totals are aggregate copies across nodes.

```

The live table exposes the values used to interpret a run:

```text
height   canonical chain length selected by that node
tip      short hash of that node's selected chain end
mempool  pending transactions not yet included in the canonical chain
orphans  blocks waiting for an unknown parent block
mining   whether that node is currently searching for a valid nonce
attempts number of nonce candidates tried for the current mining job
```

