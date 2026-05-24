#!/usr/bin/env bash
set -euo pipefail

PORT0=9000
PORT1=9001
PORT2=9002
HOST=127.0.0.1

url() {
  local port="$1"
  local path="$2"
  printf 'http://%s:%s%s' "$HOST" "$port" "$path"
}

pretty() {
  local body
  body="$(cat)"
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$body" | jq . 2>/dev/null || printf '%s\n' "$body"
  else
    printf '%s\n' "$body"
  fi
}

json_field() {
  local field="$1"
  if command -v jq >/dev/null 2>&1; then
    jq -r ".$field"
  else
    python3 -c "import json,sys; print(json.load(sys.stdin).get('$field', ''))"
  fi
}

get_json() {
  local port="$1"
  local path="$2"
  echo
  echo "GET $(url "$port" "$path")"
  curl -sS "$(url "$port" "$path")" | pretty
}

post_json() {
  local port="$1"
  local path="$2"
  local payload="$3"
  echo
  echo "POST $(url "$port" "$path")"
  echo "$payload"
  curl -sS -X POST "$(url "$port" "$path")" \
    -H 'Content-Type: application/json' \
    -d "$payload" | pretty
}

pause() {
  echo
  read -r -p "Press Enter to continue..."
}

status_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /status
  done
}

wallets_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /wallet
  done
}

chain_status_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /chain/status
  done
}

balances_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /balances
  done
}

mempools_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /transactions
  done
}

events_all() {
  for port in "$PORT0" "$PORT1" "$PORT2"; do
    get_json "$port" /events
  done
}

create_transfer_9000_to_9001() {
  local recipient
  recipient="$(curl -sS "$(url "$PORT1" /wallet)" | json_field public_key)"
  local payload
  payload="$(printf '{"to":"%s","amount":1,"memo":"manual transfer 9000 to 9001"}' "$recipient")"
  post_json "$PORT0" /transactions/create "$payload"
}

mine_9000_rewards() {
  post_json "$PORT0" /mine '{"blocks":5,"max_txs":50}'
}

mine_9000_one_block() {
  post_json "$PORT0" /mine '{"blocks":1,"max_txs":50}'
}

show_menu() {
  clear
  cat <<'MENU'
Manual three-node test

Menu                                      | Recommended order
------------------------------------------+------------------------------------------
  1  Status all nodes                     |  1. Status all nodes
  2  Wallets all nodes                    |  2. Wallets all nodes
  3  Mine 5 reward blocks on 9000         |  3. Mine 5 reward blocks on 9000
  4  Create 1 coin tx 9000 -> 9001        |  4. Create transaction 9000 -> 9001
  5  Mine 1 block on 9000                 |  5. Mine 1 block to confirm the tx
  6  Chain status all nodes               |  6. Chain status all nodes
  7  Balances all nodes                   |  7. Balances all nodes
  8  Mempools all nodes                   |
  9  Events all nodes                     |
  q  Quit                                 |
MENU
}

while true; do
  show_menu
  echo
  read -r -p "Choice: " choice
  case "$choice" in
    1) status_all; pause ;;
    2) wallets_all; pause ;;
    3) mine_9000_rewards; pause ;;
    4) create_transfer_9000_to_9001; pause ;;
    5) mine_9000_one_block; pause ;;
    6) chain_status_all; pause ;;
    7) balances_all; pause ;;
    8) mempools_all; pause ;;
    9) events_all; pause ;;
    q|Q) exit 0 ;;
    *) echo "Unknown choice"; pause ;;
  esac
done
