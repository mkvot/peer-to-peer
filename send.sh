CONTENT="Jaan carries 0.002 BTC to Ants"

HASH=$(echo -n "$CONTENT" | shasum -a 256 | awk '{print $1}')

curl -X POST http://127.0.0.1:8080/block \
     -H "Content-Type: application/json" \
     -d "{\"hash\": \"$HASH\", \"content\": \"$CONTENT\"}"

sleep 2
curl -X GET http://127.0.0.1:8080/getblock/$HASH
