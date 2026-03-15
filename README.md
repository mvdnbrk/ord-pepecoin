# ordpep

Ordinal indexer and block explorer for **Pepecoin**, forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) (based on [ordinals/ord](https://github.com/ordinals/ord) v0.5.1).

The indexer and explorer support all inscription content types. While PRC-20 inscriptions are indexed, specialized PRC-20 balance tracking is not supported.

## Requirements

- Synced `pepecoind` node with `-txindex`
- Rust 1.67+

## Building

```bash
git clone https://github.com/mvdnbrk/ord-pepecoin.git
cd ord-pepecoin
cargo build --release
```

The binary is at `./target/release/ordpep`.

## Configuration

`ordpep` can be configured with command-line flags, a YAML configuration file, or both. Command-line flags take precedence over the configuration file.

### Configuration file

Create an `ordpep.yaml` file:

```yaml
pepecoin_rpc_username: "your_rpc_user"
pepecoin_rpc_password: "your_rpc_password"
rpc_url: "127.0.0.1:33873"
data_dir: "/data/ordpep"
index: "/data/ordpep/index.redb"
```

The configuration file is loaded from the first location found:

1. `--config <path>` — explicit path (errors if not found)
2. `--config-dir <dir>/ordpep.yaml`
3. `--data-dir <dir>/ordpep.yaml`
4. Default data directory (`ordpep.yaml`)

All configuration file fields are optional:

| Field | Description |
|---|---|
| `pepecoin_rpc_username` | RPC username (alternative to cookie auth) |
| `pepecoin_rpc_password` | RPC password (alternative to cookie auth) |
| `rpc_url` | Pepecoin Core RPC URL |
| `pepecoin_data_dir` | Pepecoin Core data directory |
| `data_dir` | ordpep data directory |
| `index` | Path to the index database |
| `index_sats` | Track location of all satoshis (`true`/`false`) |
| `cookie_file` | Path to RPC cookie file |
| `server_url` | URL of the ordpep server |
| `hidden` | List of inscription IDs to hide |

### Authentication

RPC authentication is resolved in this order:

1. `pepecoin_rpc_username` + `pepecoin_rpc_password` in config file (username/password auth)
2. `--cookie-file` flag or `cookie_file` in config (cookie auth)
3. Default cookie file location (`~/.pepecoin/.cookie`)

## Usage

### With a configuration file

```bash
ordpep --config /path/to/ordpep.yaml server --http-port 3080
ordpep --config /path/to/ordpep.yaml index update
```

### With command-line flags

```bash
ordpep --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie server --http-port 3080
ordpep --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie index update
```

### Export inscriptions to TSV

```bash
ordpep index export --include-addresses > inscriptions.tsv
```

### Compact the database

```bash
ordpep index compact
```

### JSON API

The server returns JSON when the `Accept: application/json` header is set:

```bash
curl -s -H "Accept: application/json" http://localhost:3080/status
curl -s -H "Accept: application/json" http://localhost:3080/inscription/<inscription_id>
curl -s -H "Accept: application/json" http://localhost:3080/inscriptions
curl -s -H "Accept: application/json" http://localhost:3080/output/<outpoint>
curl -s -H "Accept: application/json" http://localhost:3080/block/<height>
curl -s -H "Accept: application/json" http://localhost:3080/address/<address>
curl -s http://localhost:3080/blockcount
curl -s http://localhost:3080/content/<inscription_id>
```

The `/address/<address>` endpoint returns inscription IDs and their corresponding output points, useful for inscription-aware UTXO selection in wallets:

```json
{
  "inscriptions": ["<inscription_id>", ...],
  "outputs": ["<txid>:<vout>", ...]
}
```

The `/status` endpoint returns index information:

```json
{
  "address_index": true,
  "chain": "mainnet",
  "height": 945000,
  "inscriptions": 12345,
  "sat_index": false,
  "unrecoverably_reorged": false
}
```

Raw inscription content is always available at `/content/<inscription_id>`.

## Wallet

`ordpep` includes a standalone wallet with local key management. Keys are derived locally (BIP-44 `m/44'/3434'/0'`) and stored in `wallet.redb` with restricted permissions (0600).

Signing and coin selection are performed locally, ensuring inscriptions are protected from accidental spending.

### Commands

| Command | Description |
|---|---|
| `create` | Create a new wallet and display the mnemonic |
| `receive` | Generate a new receive address |
| `balance` | Display the wallet's balance |
| `send` | Send a specific amount, inscription, or satpoint |
| `inscribe` | Create a new inscription |
| `inscriptions` | List all inscriptions held by the wallet |
| `addresses` | List all addresses in the wallet |
| `restore` | Restore a wallet from a mnemonic |

### Example Usage

```bash
# Create a new wallet
ordpep wallet create

# Generate a receive address
ordpep wallet receive

# Check balance
ordpep wallet balance

# Send an inscription
ordpep wallet send <DESTINATION_ADDRESS> <INSCRIPTION_ID>

# Send 100 pep (requires unit: pep or rib)
ordpep wallet send <DESTINATION_ADDRESS> 100pep
```

### Inscribing

```bash
ordpep wallet inscribe /path/to/file.png
ordpep wallet inscribe --dry-run /path/to/file.png
```

Inscriptions use P2SH `script_sig` transactions (Pepecoin has no SegWit). Large files are split across multiple chained transactions using 240-byte data chunks. Reveal transactions are signed locally.

### Batch Inscribing

Inscribe multiple files in a single operation:

```bash
ordpep wallet inscribe --batch batch.yaml
```

Example `batch.yaml`:

```yaml
inscriptions:
  - file: first.png
  - file: second.txt
    destination: "PR7id..." # Optional custom destination
```

## Credits

- [ordinals/ord](https://github.com/ordinals/ord) — Original Bitcoin ordinals indexer
- [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) — Dogecoin adaptation with `script_sig` support

## License

[CC0-1.0](LICENSE)
