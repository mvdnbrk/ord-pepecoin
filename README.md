# ord-pepecoin

Ordinal indexer and block explorer for **Pepecoin**, forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) (based on [ordinals/ord](https://github.com/ordinals/ord) v0.5.1).

Inscriptions on Pepecoin use `script_sig` (no SegWit). The indexer and explorer support PRC-20 tokens and all inscription content types.

> **Note:** Pepecoin has more reorgs than Bitcoin due to its 1-minute block times. Periodically create checkpoints of the redb database. See [this issue](https://github.com/casey/ord/issues/148).

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

## Usage

### Start the explorer server

```bash
ord-pepecoin --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie server --http-port 3080
```

### Update the index

```bash
ord-pepecoin --rpc-url 127.0.0.1:33873 --cookie-file ~/.pepecoin/.cookie index update
```

### Export inscriptions to TSV

```bash
ord-pepecoin index export --include-addresses > inscriptions.tsv
```

### JSON API

The server returns JSON when the `Accept: application/json` header is set:

```bash
curl -H "Accept: application/json" http://localhost:3080/inscription/<inscription_id>
curl -H "Accept: application/json" http://localhost:3080/inscriptions
curl -H "Accept: application/json" http://localhost:3080/output/<outpoint>
curl -H "Accept: application/json" http://localhost:3080/block/<height>
```

Raw inscription content is always available at `/content/<inscription_id>`.

## Wallet

`ord-pepecoin` relies on Pepecoin Core for private key management and transaction signing.

- Pepecoin Core is not aware of inscriptions and does not perform sat control. Using `pepecoin-cli` commands with `ord-pepecoin` wallets may lead to loss of inscriptions.
- Keep ordinal and cardinal wallets segregated.

> **Note:** The `wallet inscribe` command currently uses Taproot transaction construction, which is not compatible with Pepecoin (SegWit is disabled). Inscriptions must be created using external tools that build `script_sig`-based transactions.

## Credits

- [ordinals/ord](https://github.com/ordinals/ord) — Original Bitcoin ordinals indexer
- [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin) — Dogecoin adaptation with `script_sig` support
- [mvdnbrk/rust-pepecoin](https://github.com/mvdnbrk/rust-pepecoin) — Pepecoin Rust library

## License

[CC0-1.0](LICENSE)
