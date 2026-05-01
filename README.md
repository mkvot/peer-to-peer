
# ITI0215_26 Hajussüsteemid

## Praktikum 1

## Installeerimine ja käivitamine

Rakenduse käivitamiseks on vaja Rust-i kompilaatorit: [https://rustup.rs/](https://rustup.rs/)

### Käivitamine
```bash
cargo run -- <port> [peers.json] [bind-ip]

# loob uue sõlme pordil 5000
cargo run -- 5000

# loob uue sõlme koos teadaolevate sõlmedega failist
cargo run -- 5000 peers.json

# võimaldab suhelda teiste sõlmedega lokaalsel võrgul
cargo run -- 5000 peers.json 192.168.1.42
```

`peers.json` näidis:
```json
["127.0.0.1:5000", "192.168.0.1:5000"]
```

## Võrgu topoloogia

Iga sõlm käitub serveri ja kliendina, pole keskset sõlme:
- server: kuulab sissetulevaid päringuid.
- klient: pöördub perioodiliselt teiste sõlmede poole.

Võrguga liitumiseks peab uuel sõlmel olema vähemalt ühe juba töötava sõlme aadress (`peers.json`). Sõlmed jagavad omavahel infot juba teadaolevatest sõlmedest võrgus ning vajadusel uuendavad enda infot.

## Protokolli kirjeldus

### `GET /status`
Tagastab sõlme hetkeseisu: aadressi, naabrite nimekirja ning plokkide ja ledgeri seisu.

```bash
curl http://127.0.0.1:5000/status
```

```json
{
  "addr": "127.0.0.1:5000",
  "peers": ["127.0.0.1:5001", "127.0.0.1:5002"],
  "block_count": 5,
  "ledger_len": 3,
  "ledger_hash": "72df...",
  "mempool_count": 0
}
```

### `GET /addr`
Tagastab nimekirja kõigist sõlmele teadaolevatest naabritest.

```bash
curl http://127.0.0.1:5000/addr
```

```json
["127.0.0.1:5001", "127.0.0.1:5002", "127.0.0.1:5003"]
```

### `POST /peers/announce`
Sõlm reklaamib ennast teisele sõlmele. Vastuseks saab nimekirja kõigist teadaolevatest naabritest.

```bash
curl -X POST http://127.0.0.1:5000/peers/announce \
  -d '{"address": "127.0.0.1:5005"}'
```

```json
["127.0.0.1:5000", "127.0.0.1:5001", "127.0.0.1:5005"]
```

### `GET /getblocks`
Tagastab nimekirja kõigi sõlmel olevate plokkide räsides (hash).

```bash
curl http://127.0.0.1:5000/getblocks
```

```json
["b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9", "a3bc...", "ca3d..."]
```

### `GET /getblocks/{hash}`
Tagastab plokkide räside nimekirja alates etteantud räsist.

```bash
curl http://127.0.0.1:5000/getblocks/f3a2...​
```

```json
["77f44b9024fd19a6674a62d98939f4e7f1b77f64eac4c7559414c46bdaec494c", "01ca...", "asf2..."]
```

### `GET /getdata/{hash}`
Tagastab konkreetse ploki sisu vastavalt etteantud räsile.

```bash
curl http://127.0.0.1:5000/getdata/f3a2...
```

```json
{
  "hash": "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03",
  "content": "hello"
}
```

### `POST /tx`
Loob selles sõlmes uue transaktsiooni. Sõlm lisab aadressi, järjenumbri ja deterministliku id.

```bash
curl -X POST http://127.0.0.1:5000/tx \
  -d '{"body": "promise"}'
```

```json
{
  "status": "ok",
  "tx": {
    "id": "f26e...",
    "origin": "127.0.0.1:5000",
    "seq": 1,
    "body": "promise"
  }
}
```

### `POST /inv`
Saadab edasi juba loodud typed transaktsiooni. Räsi/sisu kujul vana formaat ei ole kasutusel.

```json
{
  "id": "f26e...",
  "origin": "127.0.0.1:5000",
  "seq": 1,
  "body": "promise"
}
```

### `GET /ledger`
Tagastab järjestatud ledgeri transaktsioonid.

```bash
curl http://127.0.0.1:5000/ledger
```

### `POST /block`
Saadab uue ploki. Sõlm kontrollib, kas räsi klapib sisuga ning lisab ploki oma registrisse, kui seda veel seal pole.

```bash
curl -X POST http://127.0.0.1:5000/block \
  -d '{
    "hash": "90a7b08f76a1a33dc3e4c9decf39ff93a88918f1f46fd1b3fbf5edd619d77dc6",
    "content": "block"
  }'
```

```json
{ "status": "ok" }
```

### `GET /ping`
Tavaline ping, et kontrollida, kas sõlm on elus.

```bash
curl http://127.0.0.1:5000/ping
```

`200 OK`

## Katsed

Testid tegin läbi ühes arvutis, mitu protsessi eri portidel. Proovisin kohalikus võrgus luua sõlmesid, mis töötas, ehkki seda oli käsitsi üpriski tüütu teha. Testid on `test.py` failis.
```bash
python3 test.py                  # kõik testid
python3 test.py --large          # 30 sõlme
python3 test.py --large2         # 40 sõlme järkjärgult
python3 test.py --scale          # limit test
```

### Tulemused

| Test | Kirjeldus | Tulemus |
|------|-----------|---------|
| 1. Lineaarne ahel | 5 sõlme, plokk levib otspunktist otspunkti | ✓ |
| 2. Täht-topoloogia | 1 "peamine" sõlm + 4 sõlme, plokk + transaktsioon | ✓ |
| 3. Paaride ühendamine | 2 eraldatud paari liidetakse, plokk levib | ✓ |
| 4. Sõlme eemaldamine | 6 sõlmest 2 eemaldatakse, ülejäänud 4 töötavad edasi | ✓ |
| 5. Hiline liituja | uus sõlm liitub ja sünkroniseerib 3 olemasolevat plokki | ✓ |
| 6. 30 sõlme | kõik 30 sõlme saavad 2 plokki kätte | ✓ |
| 7. 40 sõlme järkjärgult | sõlmed liituvad ükshaaval, 5 plokki + 3 txn | ✓ |
| 8. Limit test | sõlmi lisatakse kuni ploki levik on piisavalt madal | ✓ |

### Limit test

See test lisab 10 sõlme iga 12s tagant, siis saadab ploki ja vaatab selle levimist.

Ühe testi tulemus:
| Sõlmi | Levis | Levik% | Aeg |
|-------|-------|--------|-----|
| 11 | 11/11 | 100% | 0.0s |
| 21 | 21/21 | 100% | 4.1s |
| 31–101 | 100% | 100% | ~4.0–4.3s |
| 111 | 111/111 | 100% | 56.4s |
| 121 | 121/121 | 100% | 27.1s |
| 131 | 131/131 | 100% | 61.3s |
| 141 | 141/141 | 100% | 27.6s |
| 150 | 149/150 | 99% | 35.0s |

Praktiline piir: ~150 sõlme. Kuni 101 sõlmeni on levimine stabiilselt ~4s. Üle 100 sõlme hakkab levimisaeg kasvama ja muutub ebaühtlaseks (27–61s). 150 sõlme juures jõudis plokk 149/150 sõlmeni.
Mõni test kukkus juba ~120 juures 0% levikuni, kuid ma pole kindel, mis seda põhjustas.

## Praktikum 2

Praktikum 2 lisab järjestatud transaktsioonide ledgeri ja lihtsa konsensuse. Vaikimisi töötab sõlm endiselt ilma konsensuseta, et Praktikum 1 käitumine säiliks. Konsensus lülitatakse sisse keskkonnamuutujaga.

### Käivituse seaded

```bash
P2P_CONSENSUS=1       # käivitab konsensuse taustatsükli
P2P_FORWARD_INV=0     # lülitab transaktsioonide gossip-forwardingu välja
P2P_DATA_DIR=data     # baaskaust püsivate ledgerite jaoks
P2P_ROUND_SECS=5      # konsensuse roundi pikkus sekundites
```

Iga sõlm kirjutab enda andmed kausta:

```text
${P2P_DATA_DIR}/${port}/
```

Olulised failid:

- `ledger.json`: sõlme järjestatud kinnitatud transaktsioonid
- `commits.jsonl`: konsensusega vastu võetud commitid, üks JSON rida iga roundi kohta

### Konsensuse algoritm

Kasutusel on lihtne permissioned rotating-king protokoll.

1. Iga roundi liikmed on sorteeritud nimekiri: sõlm ise + teadaolevad peerid, välja arvatud eksperimendi jaoks blokeeritud outbound peerid.
2. Roundi juht on `members[round % members.len()]`.
3. Juht küsib kõigilt liikmetelt `GET /consensus/proposal/{round}`.
4. Arvesse lähevad ainult proposalid, mille `round` ja `ledger_hash` kattuvad juhi omaga.
5. Kui proposalite arv on vähemalt enamus `members.len() / 2 + 1`, teeb juht commiti.
6. Transaktsioonid deduplikeeritakse ja sorteeritakse deterministlikult `origin`, `seq`, `id` järgi.
7. Commit võetakse vastu ainult siis, kui see laiendab kohalikku `ledger_hash` väärtust ja round vastab `next_round` väärtusele.
8. Sõlmed, mis on roundist maha jäänud, saavad catch-up teha endpointist `GET /consensus/commits/{from_round}`.

Piirang: liikmeskond tuleb dünaamilisest peer-listist ja signatuure ei ole. See sobib labori eksperimendiks, aga pahatahtlik juht saaks liikmeskonna kohta valetada.

### Uued endpointid

```bash
curl http://127.0.0.1:9000/ledger/status
curl http://127.0.0.1:9000/ledger
curl http://127.0.0.1:9000/consensus/proposal/0
curl http://127.0.0.1:9000/consensus/commits/0
```

Commiti saatmine:

```bash
curl -X POST http://127.0.0.1:9000/consensus/commit -d '{...Commit JSON...}'
```

Eksperimendi veasüst:

```bash
curl -X POST http://127.0.0.1:9000/debug/faults \
  -d '{"forward_inv_enabled": false, "block_peer": "127.0.0.1:9003"}'
```

`unblock_peer` eemaldab outbound bloki:

```bash
curl -X POST http://127.0.0.1:9000/debug/faults \
  -d '{"unblock_peer": "127.0.0.1:9003"}'
```

### Demo

```bash
cargo build

echo '[]' > /tmp/p2p-9000.json
echo '["127.0.0.1:9000"]' > /tmp/p2p-bootstrap.json

P2P_CONSENSUS=1 P2P_FORWARD_INV=0 P2P_DATA_DIR=/tmp/p2p-demo ./target/debug/peer-to-peer 9000 /tmp/p2p-9000.json
P2P_CONSENSUS=1 P2P_FORWARD_INV=0 P2P_DATA_DIR=/tmp/p2p-demo ./target/debug/peer-to-peer 9001 /tmp/p2p-bootstrap.json
P2P_CONSENSUS=1 P2P_FORWARD_INV=0 P2P_DATA_DIR=/tmp/p2p-demo ./target/debug/peer-to-peer 9002 /tmp/p2p-bootstrap.json
```

Teises terminalis:

```bash
curl -X POST http://127.0.0.1:9000/tx -d '{"body":"T from 9000"}'
curl -X POST http://127.0.0.1:9001/tx -d '{"body":"T from 9001"}'
curl -X POST http://127.0.0.1:9002/tx -d '{"body":"T from 9002"}'

curl http://127.0.0.1:9000/ledger/status
curl http://127.0.0.1:9001/ledger/status
curl http://127.0.0.1:9002/ledger/status
```

### Prax2 katsed

Katsed on failis `tests/test_prax2.py`.

```bash
cargo build
python3 tests/test_prax2.py --divergence
python3 tests/test_prax2.py --converge
python3 tests/test_prax2.py --invalid
python3 tests/test_prax2.py --no-quorum
python3 tests/test_prax2.py --partition
python3 tests/test_prax2.py --load
```

Lühem load smoke-test:

```bash
python3 tests/test_prax2.py --load --load-sizes 5 --load-duration 5
```

Viimati kontrollitud tulemused:

| Katse | Tulemus |
|-------|---------|
| `--divergence` | 3 sõlme ilma konsensuseta ja forwardinguta tekitasid 3 erinevat ledger hash'i. |
| `--converge` | 5 sõlme konsensusega jõudsid sama 3-transaktsioonilise ledgerini. |
| `--invalid` | Vigase id-ga `/inv` sai `400` vastuse ja ei jõudnud ledgerisse; järgnev korrektne tx commititi kõigis sõlmedes. |
| `--no-quorum` | 1 sõlm + 4 kättesaamatut phantom peer'i ei commitinud transaktsiooni; tx jäi mempooli. |
| `--partition` | 3-sõlmeline ja 2-sõlmeline eraldatud grupp converge'isid eraldi, kuid lõpuks erinevate ledger hash'idega. |
| `--load --load-sizes 50 --load-duration 30` | 50 sõlme commitisid 120 transaktsiooni; kõik jõudsid sama ledger hash'ini umbes 30.7 sekundiga. |

Täismõõtmise jaoks kasutab `--load` vaikimisi suurusi `5,10,25,50` ja 30 sekundit transaktsioonide tekitamist iga suuruse kohta.
