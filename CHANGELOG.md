# Changelog

All notable changes to ordpep are documented in this file.

This project is forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin),
which is itself forked from [ordinals/ord](https://github.com/ordinals/ord) `0.5.1`.

## [0.7.1](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.7.1) - 2026-03-16

### Fixed
- Reorg recovery: dedicated reorg module with proper savepoint management ([#23](https://github.com/mvdnbrk/ord-pepecoin/pull/23))

## [0.7.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.7.0) - 2026-03-15

### Added
- Standalone wallet with local key management, no Core wallet dependency ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- Minimum fee rate validation (10,000 sat/vB Pepecoin relay fee) ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- `wallet send --max` to sweep all cardinal UTXOs ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- `wallet send <address> <amount>` for sending PEP by amount ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- `wallet addresses` subcommand ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- Destination address in inscribe output ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))
- `wallet outputs` enhanced with address, inscriptions, and sat ranges ([#19](https://github.com/mvdnbrk/ord-pepecoin/pull/19))

## [0.6.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.6.0) - 2026-03-14

### Added
- P2SH scriptSig inscription support (commit, reveal, batch inscribe) ([#3](https://github.com/mvdnbrk/ord-pepecoin/pull/3))
- OP_PUSHDATA2/4 parser support ([#5](https://github.com/mvdnbrk/ord-pepecoin/pull/5))
- 240-byte chunk size for inscription data
- JSON API support
- Page-based pagination for `/inscriptions` endpoint ([#6](https://github.com/mvdnbrk/ord-pepecoin/pull/6))
- `POST /outputs` batch endpoint and updated `GET /output/:outpoint` JSON response ([#7](https://github.com/mvdnbrk/ord-pepecoin/pull/7))
- `POST /inscriptions` batch endpoint with upstream-compatible response format ([#9](https://github.com/mvdnbrk/ord-pepecoin/pull/9))
- Integration tests for all JSON API endpoints ([#11](https://github.com/mvdnbrk/ord-pepecoin/pull/11))
- Wallet commands talk to running ord server via HTTP instead of opening index directly ([#15](https://github.com/mvdnbrk/ord-pepecoin/pull/15))
- `server_url`, `http_port`, `address` config options in `ord.yaml` ([#15](https://github.com/mvdnbrk/ord-pepecoin/pull/15))
- `ord.yaml.example` with documented config options ([#15](https://github.com/mvdnbrk/ord-pepecoin/pull/15))
- `/update` test-only endpoint for synchronous index updates ([#15](https://github.com/mvdnbrk/ord-pepecoin/pull/15))
- Batch inscribe: `ordpep wallet inscribe --batch batch.yaml` for multiple files in one operation ([#17](https://github.com/mvdnbrk/ord-pepecoin/pull/17))
- Index size on status page (human-readable HTML, raw bytes in JSON API)
- Address index with inscription-aware UTXO selection
- YAML config file support
- Index export command and `index update` subcommand (replaces deprecated `index run`)
- Reorg resistance with redb

### Fixed
- Legacy RPC compatibility (signrawtransaction, getnewaddress, validateaddress, dumpprivkey)
- Local signing for reveal transactions
- Graceful shutdown to prevent redb corruption
- RPC fetcher timeout and retry to prevent deadlock
- Default data dir changed to `ordpep` to avoid collision with bitcoin ord
- Hostname leak in og:image meta tag
- All tests passing (283 lib + 98 integration)
- Handle RPC error code -5 for unknown transactions (Pepecoin Core compatibility)

### Changed
- Binary renamed to `ordpep` (was `ord-pepecoin`) ([#16](https://github.com/mvdnbrk/ord-pepecoin/pull/16))
- Adapted from Dogecoin to Pepecoin chain parameters
- Fee rate and postage defaults tuned for Pepecoin
- Use `Durability::Immediate` for redb writes
- CI workflow simplified (test only, macOS + Ubuntu)
- In-process TestServer aligned with upstream (channel-based readiness, no subprocess polling) ([#15](https://github.com/mvdnbrk/ord-pepecoin/pull/15))
- Extract API types into `src/api.rs` module matching upstream pattern ([#8](https://github.com/mvdnbrk/ord-pepecoin/pull/8))
- Inscription API fields renamed to match upstream (`fee`, `height`, `id`, `satpoint`, `value`) ([#9](https://github.com/mvdnbrk/ord-pepecoin/pull/9))
- Upgrade axum 0.6 → 0.8 with ecosystem deps to match upstream ([#12](https://github.com/mvdnbrk/ord-pepecoin/pull/12))
- Upgrade redb 1.0 → 3.1 ([#14](https://github.com/mvdnbrk/ord-pepecoin/pull/14))

### Removed
- Upstream docs, examples, benchmark, contrib, fuzz folders
- BIP document, Vagrantfile, justfile
- Ordinals handbook link from nav
- Broken tag parsing and `--parent` flag (temporarily)

## Upstream History

For the changelog of the original ord project (versions 0.0.1 through 0.5.1),
see [ordinals/ord releases](https://github.com/ordinals/ord/releases).
