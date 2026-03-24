## Example on how to get nodes communicating on different machines on LAN

```bash
ip addr show | grep "inet " | grep -v 127 # get ip

# file can be empty
cargo run -- 6000 peers.json <192.168.0.1> # pc 1

# file must contain the node ip+port of the other pc e.g. ["192.168.0.1:6000"]
cargo run -- 6000 peers.json <192.168.0.2> # pc 2
```
## Various command examples

```bash
# checking status
curl http://192.168.0.1:6000/status
curl http://192.168.0.2:6000/status

# post block
CONTENT="hello world"
HASH=$(echo -n "$CONTENT" | sha256sum | cut -d' ' -f1)
curl -X POST http://192.168.0.1:6000/block \
  -H "Content-Type: application/json" \
  -d "{\"hash\":\"$HASH\",\"content\":\"$CONTENT\"}"

# post transaction
CONTENT="Jaan pays Ants 0.5 BTC"
HASH=$(echo -n "$CONTENT" | sha256sum | cut -d' ' -f1)
curl -X POST http://192.168.0.2:6000/inv \
  -H "Content-Type: application/json" \
  -d "{\"hash\":\"$HASH\",\"content\":\"$CONTENT\"}"

# list blocks
curl http://192.168.0.1:6000/getblocks

# get specific block by hash
CONTENT="hello world"
HASH=$(echo -n "$CONTENT" | sha256sum | cut -d' ' -f1)
curl http://192.168.0.1:6000/getdata/$HASH

# list 
curl http://192.168.1.77:6000/addr

# run tests (requires python)
python3 test.py                        # basic tests (5 scenarios)
python3 test.py --large                # 30-node test
python3 test.py --large2               # 40-node gradual test
```