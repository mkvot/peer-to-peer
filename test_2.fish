#!/usr/bin/env fish
pkill -f peer-to-peer 2>/dev/null
sleep 1

tmux kill-session -t p2p 2>/dev/null
cargo build
if test $status -ne 0
    echo "Build failed"
    exit 1
end

tmux new-session -d -s p2p -x 220 -y 50
tmux split-window -h -t p2p
tmux split-window -v -t p2p:0.1

tmux send-keys -t p2p:0.0 './target/debug/peer-to-peer 8081' Enter
sleep 0.3
tmux send-keys -t p2p:0.1 './target/debug/peer-to-peer 8080 peers_8080.json' Enter
sleep 0.3
tmux send-keys -t p2p:0.2 './target/debug/peer-to-peer 8082' Enter
sleep 0.3

curl -s -X POST http://127.0.0.1:8080/peers/announce \
  -H 'Content-Type: application/json' \
  -d '{"address": "127.0.0.1:8082"}'

tmux attach -t p2p
