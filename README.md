# ord-pepecoin

Ordinal indexer and block explorer for **Pepecoin**, forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) (based on [ordinals/ord](https://github.com/ordinals/ord) v0.5.1).

Inscriptions on Pepecoin use `script_sig` (no SegWit). The indexer and explorer support PRC-20 tokens and all inscription content types.

## Requirements

- Synced `pepecoind` node with `-txindex`
- Rust 1.67+

## Building

```bash
git clone https://github.com/mvdnbrk/ord-pepecoin.git
cd ord-pepecoin
cargo build --release
```

The binary is at `./target/release/ord-pepecoin`.

## Configuration

`ord-pepecoin` can be configured with command-line flags, a YAML configuration file, or both. Command-line flags take precedence over the configuration file.

### Configuration file

Create an `ord.yaml` file:

```yaml
pepecoin_rpc_username: "your_rpc_user"
pepecoin_rpc_password: "your_rpc_password"
rpc_url: "127.0.0.1:33873"
data_dir: "/data/ord-pepecoin"
index: "/data/ord-pepecoin/index.redb"
```

The configuration file is loaded from the first location found:

1. `--config <path>` — explicit path (errors if not found)
2. `--config-dir <dir>/ord.yaml`
3. `--data-dir <dir>/ord.yaml`
4. Default data directory (`ord.yaml`)

All configuration file fields are optional:

| Field | Description |
|---|---|
| `pepecoin_rpc_username` | RPC username (alternative to cookie auth) |
| `pepecoin_rpc_password` | RPC password (alternative to cookie auth) |
| `rpc_url` | Pepecoin Core RPC URL |
| `pepecoin_data_dir` | Pepecoin Core data directory |
| `data_dir` | ord-pepecoin data directory |
| `index` | Path to the index database |
| `index_sats` | Track location of all satoshis (`true`/`false`) |
| `cookie_file` | Path to RPC cookie file |
| `hidden` | List of inscription IDs to hide |

### Authentication

RPC authentication is resolved in this order:

1. `pepecoin_rpc_username` + `pepecoin_rpc_password` in config file (username/password auth)
2. `--cookie-file` flag or `cookie_file` in config (cookie auth)
3. Default cookie file location (`~/.pepecoin/.cookie`)

## Usage

### With a configuration file

```bash
ord-pepecoin --config /path/to/ord.yaml server --http-port 3080
ord-pepecoin --config /path/to/ord.yaml index update
```

### With command-line flags

```bash
ord-pepecoin --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie server --http-port 3080
ord-pepecoin --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie index update
```

### Export inscriptions to TSV

```bash
ord-pepecoin index export --include-addresses > inscriptions.tsv
```

### Compact the database

```bash
ord-pepecoin index compact
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

## Reorg Resistance

The indexer automatically creates database savepoints near the chain tip. If a blockchain reorganization is detected, it restores the most recent savepoint and re-indexes from there. This is important for Pepecoin due to its 1-minute block times which make reorgs more frequent than Bitcoin.

## Wallet

`ord-pepecoin` relies on Pepecoin Core for private key management and transaction signing.

- Pepecoin Core is not aware of inscriptions and does not perform sat control. Using `pepecoin-cli` commands with `ord-pepecoin` wallets may lead to loss of inscriptions.
- Keep ordinal and cardinal wallets segregated.

### Inscribing

```bash
ord-pepecoin --config /path/to/ord.yaml wallet inscribe /path/to/file.png
ord-pepecoin --config /path/to/ord.yaml wallet inscribe --dry-run /path/to/file.png
```

Inscriptions use P2SH `script_sig` transactions (Pepecoin has no SegWit). Large files are split across multiple chained transactions using 240-byte data chunks. Reveal transactions are signed locally.

## Credits

- [ordinals/ord](https://github.com/ordinals/ord) — Original Bitcoin ordinals indexer
- [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) — Dogecoin adaptation with `script_sig` support

## License

[CC0-1.0](LICENSE)
