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

The real scenarios start real node processes, mine real proof-of-work blocks, render live status tables, pause before cleanup, and write a full event timeline next to each script as a `.log` file.

```bash
python3 scripts/scenarios/01_no_sync_divergence.py
python3 scripts/scenarios/02_mining_convergence.py
python3 scripts/scenarios/03_longer_chain_reorg.py
python3 scripts/scenarios/04_invalid_data_rejection.py
python3 scripts/scenarios/05_orphan_block_recovery.py
python3 scripts/scenarios/06_partition_failure.py
python3 scripts/scenarios/07_overload_failure.py --nodes 50 --duration 120
```
