
Usage:
# Prax2 CLI And LAN Guide

## Build

```bash
cargo build
```

The binary is:

```bash
./target/debug/peer-to-peer
```

## Start Local Nodes

Consensus is enabled by default. Direct transaction forwarding is disabled by default, so transaction ordering comes from the consensus protocol.

Terminal 1:

```bash
./target/debug/peer-to-peer 9000 --data-dir /tmp/p2p-demo
```

Terminal 2:

```bash
./target/debug/peer-to-peer 9001 --peer 127.0.0.1:9000 --data-dir /tmp/p2p-demo
```

Terminal 3:

```bash
./target/debug/peer-to-peer 9002 --peer 127.0.0.1:9000 --data-dir /tmp/p2p-demo
```

Open the dashboard:

```text
http://127.0.0.1:9000/
```

Open the experiment action page:

```text
http://127.0.0.1:9000/experiments
```

## Useful CLI Options

```bash
./target/debug/peer-to-peer <port> [options]
```

| Option | Meaning |
| --- | --- |
| `--peer <addr-or-ip>` | Add one bootstrap peer. If the value has no port, this node's port is used. |
| `--peers <a,b,c>` | Add comma-separated peers. |
| `--peers-file <path>` | Load peers from a JSON array. |
| `--advertise-ip <ip>` | Address other machines should use for this node. |
| `--bind-ip <ip>` | Local bind address. Default is `0.0.0.0`. |
| `--data-dir <path>` | Base directory for per-node `ledger.json` and `commits.jsonl`. |
| `--round-secs <n>` | Consensus tick interval. Default is `2`. |
| `--no-consensus` | Disable consensus and append accepted transactions locally. |
| `--forward-inv` | Gossip transactions directly with `/inv`. |

## Basic Curl Commands

Post a transaction:

```bash
curl -X POST http://127.0.0.1:9000/tx -d '{"body":"hello from 9000"}'
```

Read one node's status:

```bash
curl http://127.0.0.1:9000/ledger/status
```

Read the committed ledger:

```bash
curl http://127.0.0.1:9000/ledger
```

Disable transaction forwarding on a node:

```bash
curl -X POST http://127.0.0.1:9000/debug/faults -d '{"forward_inv_enabled":false}'
```

Block outbound traffic from one node to another:

```bash
curl -X POST http://127.0.0.1:9000/debug/faults -d '{"block_peer":"127.0.0.1:9001"}'
```

Unblock it:

```bash
curl -X POST http://127.0.0.1:9000/debug/faults -d '{"unblock_peer":"127.0.0.1:9001"}'
```

## Run With Another Machine

Assume:

- Machine A IP is `192.168.1.10`.
- Machine B IP is `192.168.1.20`.
- Both machines use port `9000`.

On machine A:

```bash
./target/debug/peer-to-peer 9000 \
  --advertise-ip 192.168.1.10 \
  --data-dir /tmp/p2p-lan
```

On machine B:

```bash
./target/debug/peer-to-peer 9000 \
  --advertise-ip 192.168.1.20 \
  --peer 192.168.1.10 \
  --data-dir /tmp/p2p-lan
```

Because `--peer 192.168.1.10` has no port, the program uses the local node port, so it becomes `192.168.1.10:9000`.

If the machines use different ports, include the peer port explicitly:

```bash
./target/debug/peer-to-peer 9002 \
  --advertise-ip 192.168.1.20 \
  --peer 192.168.1.10:9000 \
  --data-dir /tmp/p2p-lan
```

Open firewall access for the TCP port you use. From machine B, this should work:

```bash
curl http://192.168.1.10:9000/ping
curl http://192.168.1.10:9000/ledger/status
```

You can also open the remote node UI in a browser:

```text
http://192.168.1.10:9000/
```

## Run Automated Experiments

The Python harness starts local processes, posts transactions, waits for convergence, and writes logs under `prax2_results/`.

```bash
cargo build
python3 tests/test_prax2.py --divergence
python3 tests/test_prax2.py --converge
python3 tests/test_prax2.py --leader-failure
python3 tests/test_prax2.py --invalid
python3 tests/test_prax2.py --no-quorum
python3 tests/test_prax2.py --partition
python3 tests/test_prax2.py --load --load-sizes 5,10,25,50 --load-duration 30
```

Fast smoke run:

```bash
python3 tests/test_prax2.py \
  --divergence --converge --leader-failure --invalid --no-quorum --partition \
  --load --load-sizes 5 --load-duration 5
```

Use a different base port if needed:

```bash
python3 tests/test_prax2.py --base-port 12000 --converge
```

## Demo Script

```bash
python3 scripts/demo.py
```

It starts real nodes and prints six default sections:

1. moderate 10-node consensus,
2. baseline divergence without consensus,
3. convergence with consensus,
4. bad actor invalid transaction rejection,
5. no-quorum consensus failure,
6. leader failure and the current no-view-change limitation.

Run only one section:

```bash
python3 scripts/demo.py --scenario moderate
python3 scripts/demo.py --scenario converge
python3 scripts/demo.py --scenario bad-actor
python3 scripts/demo.py --scenario no-quorum
python3 scripts/demo.py --scenario leader-failure
```

It prints an `open:` URL for `/experiments`. Open that page once and leave it open. The page auto-detects the demo helper server and updates its scanned ports automatically when the demo switches scenarios.

Use `--step` when you want confirmation before/after the important actions and before each selected scenario is shut down:

```bash
python3 scripts/demo.py --step
```

30-node scale demo:

```bash
python3 scripts/demo.py --scenario scale --scale-nodes 30 --scale-txs 30 --step
python3 scripts/demo.py --include-scale --step
```
Save the terminal log only when needed:

```bash
python3 scripts/demo.py --output demo_results.txt
python3 scripts/demo.py --save-results
```

## Consensus Scale Comparison


```bash
python3 tests/test_prax2.py --base-port 23000 --divergence
python3 tests/test_prax2.py --base-port 24000 --load --load-sizes 10,20,30 --load-duration 5
python3 tests/test_prax2.py --base-port 26000 --load --load-sizes 40,50,75,100 --load-duration 5
python3 tests/test_prax2.py --base-port 30000 --load --load-sizes 110,120 --load-duration 5
python3 tests/test_prax2.py --base-port 28000 --load --load-sizes 125,150,200 --load-duration 5
```

Without consensus, 3 nodes immediately produced 3 different ledgers and 3 different ledger hashes.

With consensus enabled:

| Mode | Nodes | Duration | Posted tx | Failed posts | Ledger agreement | Result |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| No consensus | 3 | smoke | 3 | 0 | 3 different hashes / 3 nodes | no agreement |
| Consensus | 10 | 5s | 20 | 0 | 10/10 same length, 1 hash | converged |
| Consensus | 20 | 5s | 20 | 0 | 20/20 same length, 1 hash | converged |
| Consensus | 30 | 5s | 20 | 0 | 30/30 same length, 1 hash | converged |
| Consensus | 40 | 5s | 20 | 0 | 40/40 same length, 1 hash | converged |
| Consensus | 50 | 5s | 20 | 0 | 50/50 same length, 1 hash | converged |
| Consensus | 75 | 5s | 20 | 0 | 75/75 same length, 1 hash | converged, but slow |
| Consensus | 100 | 5s | 20 | 0 | 100/100 same length, 1 hash | converged |
| Consensus | 110 | 5s | 19 | 1 | 2 hashes, some nodes unreachable in snapshot | failed to converge before timeout |
| Consensus | 120 | 5s | 2 | 2 | 2 hashes, several nodes unreachable in snapshot | failed to converge before timeout |
| Consensus | 125 | startup | 0 | n/a | node 28536 did not start | startup failed |

The observed local failure boundary is around 110 nodes for this short 5-second load test. At 75 nodes the system still converged, but convergence took about 73.7 seconds. At 110+ nodes the local machine/harness starts hitting timeouts and unreachable node snapshots, so consensus does not finish within the test timeout.

## Browser Experiment Page

`/experiments` can post transactions, inject debug faults, read ledgers, and build harness commands. It does not spawn binaries by itself. Start nodes in terminals first, or run `scripts/demo.py --step`, then use the page to drive actions and watch state change. When opened during a demo run, it auto-detects the helper server and updates `Host`, `Base port`, `Ports`, and `Target ports` from the running demo.

General interface:

| Area | Purpose |
| --- | --- |
| Top inputs | `Host`, `Base port`, and `Ports` define which running nodes the page contacts. |
| Summary counters | Show how many nodes are online, how many different ledger hashes exist, total committed ledger entries, and total pending mempool entries. |
| Current scan table | Shows per-node ledger length, mempool size, consensus round, and short ledger hash. |
| Log | Records HTTP actions and full JSON responses. |
