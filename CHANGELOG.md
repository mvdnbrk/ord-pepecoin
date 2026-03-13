# Changelog

All notable changes to ord-pepecoin are documented in this file.

This project is forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin),
which is itself forked from [ordinals/ord](https://github.com/ordinals/ord) `0.5.1`.

## [Unreleased]

### Added
- P2SH scriptSig inscription support (commit, reveal, batch inscribe) ([#3](https://github.com/mvdnbrk/ord-pepecoin/pull/3))
- OP_PUSHDATA2/4 parser support ([#5](https://github.com/mvdnbrk/ord-pepecoin/pull/5))
- Wallet rewrite for Pepecoin P2PKH (BIP-44, coin type 3434)
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
- Address index with inscription-aware UTXO selection
- YAML config file support
- Index export command and `index update` subcommand (replaces deprecated `index run`)
- Reorg resistance with redb

### Fixed
- Legacy RPC compatibility (signrawtransaction, getnewaddress, validateaddress, dumpprivkey)
- Local signing for reveal transactions
- Graceful shutdown to prevent redb corruption
- RPC fetcher timeout and retry to prevent deadlock
- Default data dir changed to `ord-pepecoin` to avoid collision with bitcoin ord
- Hostname leak in og:image meta tag
- All tests passing (281 lib + 81 integration)
- Handle RPC error code -5 for unknown transactions (Pepecoin Core compatibility)

### Changed
- Binary renamed to `ord-pepecoin`
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
