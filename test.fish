#!/usr/bin/env fish
pkill -f peer-to-peer 2>/dev/null
sleep 1
tmux kill-session -t p2p 2>/dev/null
cargo build
if test $status -ne 0
    echo "Build failed"
    exit 1
end

echo '[]' > peers_8080.json
echo '["127.0.0.1:8080"]' > peers_bootstrap.json

tmux new-session -d -s p2p -x 280 -y 60
tmux split-window -h -t p2p
tmux split-window -h -t p2p:0.1
tmux split-window -v -t p2p:0.0
tmux split-window -v -t p2p:0.1

tmux send-keys -t p2p:0.0 './target/debug/peer-to-peer 8080 peers_8080.json' Enter
sleep 0.3

tmux send-keys -t p2p:0.1 './target/debug/peer-to-peer 8081 peers_bootstrap.json' Enter
sleep 0.3
tmux send-keys -t p2p:0.2 './target/debug/peer-to-peer 8082 peers_bootstrap.json' Enter
sleep 0.3
tmux send-keys -t p2p:0.3 './target/debug/peer-to-peer 8083 peers_bootstrap.json' Enter
sleep 0.3
tmux send-keys -t p2p:0.4 './target/debug/peer-to-peer 8084 peers_bootstrap.json' Enter
sleep 0.3
tmux send-keys -t p2p:0.2 './target/debug/peer-to-peer 8085 peers_bootstrap.json' Enter
sleep 0.3

tmux attach -t p2p
sleep 1

# for testing blocks
# curl -X POST http://127.0.0.1:8080/block \
#   -H 'Content-Type: application/json' \
#   -d '{"hash": "abc123", "content": "hello world"}'

# curl http://127.0.0.1:8081/getblocks
# curl http://127.0.0.1:8081/getdata/abc123
