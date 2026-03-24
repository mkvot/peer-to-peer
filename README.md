
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
Tagastab sõlme hetkeseisu: aadressi, naabrite nimekirja ning plokkide ja transaktsioonide arvu.

```bash
curl http://127.0.0.1:5000/status
```

```json
{
  "addr": "127.0.0.1:5000",
  "peers": ["127.0.0.1:5001", "127.0.0.1:5002"],
  "block_count": 5,
  "transaction_count": 12
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

### `POST /inv`
Saadab uue transaktsiooni. Kui sõlmel seda veel pole, salvestab ta selle ja saadab (floodib) edasi oma naabritele.

```bash
curl -X POST http://127.0.0.1:5000/inv \
  -d '{
    "hash": "012ff72aa4e6f67d836bdce670c938023c08c90cdc1ee33f8c4151151ab6f028",
    "content": "promise"
  }'
```

```json
{ "status": "ok" }
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