#!/usr/bin/env bash
set -euo pipefail

ROOT="$(pwd)"
BINARY="./target/debug/peer-to-peer"
DATA_DIR="./ledger_manual"
DIFFICULTY=4

if ! command -v alacritty >/dev/null 2>&1; then
  echo "alacritty was not found in PATH."
  exit 1
fi

if [ ! -f Cargo.toml ]; then
  echo "Run this script from the repository root."
  exit 1
fi

cargo build

if [ -d "$DATA_DIR" ]; then
  read -r -p "Reset $DATA_DIR before starting? [y/N] " reset_answer
  case "$reset_answer" in
    y|Y|yes|YES) rm -rf "$DATA_DIR" ;;
  esac
fi

mkdir -p "$DATA_DIR"

mark_above() {
  local title="$1"
  if ! command -v wmctrl >/dev/null 2>&1; then
    return 0
  fi
  (
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      wmctrl -r "$title" -b add,above >/dev/null 2>&1 && exit 0
      sleep 0.3
    done
  ) &
}

launch_terminal() {
  local title="$1"
  local x="$2"
  local y="$3"
  local columns="$4"
  local lines="$5"
  local command="$6"

  alacritty \
    --title "$title" \
    --working-directory "$ROOT" \
    -o "window.position.x=$x" \
    -o "window.position.y=$y" \
    -o "window.dimensions.columns=$columns" \
    -o "window.dimensions.lines=$lines" \
    -e bash -lc "$command" &
  mark_above "$title"
}

node_command() {
  local port="$1"
  local peers="${2:-}"
  local cmd="$BINARY $port --data-dir $DATA_DIR --difficulty $DIFFICULTY --bind-ip 127.0.0.1"
  if [ -n "$peers" ]; then
    cmd="$cmd --peers $peers"
  fi
  printf '%s; echo; read -r -p "node %s exited. Press Enter to close..."' "$cmd" "$port"
}

launch_terminal "p2p-node-9000" 0 0 58 16 "$(node_command 9000)"
sleep 0.6
launch_terminal "p2p-node-9001" 520 0 58 16 "$(node_command 9001 127.0.0.1:9000)"
sleep 0.6
launch_terminal "p2p-node-9002" 1040 0 58 16 "$(node_command 9002 127.0.0.1:9000,127.0.0.1:9001)"
sleep 1.0
launch_terminal "p2p-monitor" 0 360 104 26 "python3 scripts/monitor_nodes.py --ports 9000-9002 --interval 2; echo; read -r -p 'monitor exited. Press Enter to close...'"
launch_terminal "p2p-curl-helper" 980 360 92 26 "bash scripts/manual_curl_helper.sh"

cat <<'INFO'
Started:
  node 9000
  node 9001
  node 9002
  monitor
  interactive curl helper

If windows are not kept above, install wmctrl or use KDE window rules.
INFO
