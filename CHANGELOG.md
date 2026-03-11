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
- Address index with inscription-aware UTXO selection
- YAML config file support
- Index export command and `index update` subcommand (replaces deprecated `index run`)
- Reorg resistance with redb 1.0

### Fixed
- Legacy RPC compatibility (signrawtransaction, getnewaddress, validateaddress, dumpprivkey)
- Local signing for reveal transactions
- Graceful shutdown to prevent redb corruption
- RPC fetcher timeout and retry to prevent deadlock
- Default data dir changed to `ord-pepecoin` to avoid collision with bitcoin ord
- Hostname leak in og:image meta tag
- All tests passing (281 lib + 81 integration)

### Changed
- Binary renamed to `ord-pepecoin`
- Adapted from Dogecoin to Pepecoin chain parameters
- Fee rate and postage defaults tuned for Pepecoin
- Use `Durability::Immediate` for redb writes
- CI workflow simplified (test only, macOS + Ubuntu)

### Removed
- Upstream docs, examples, benchmark, contrib, fuzz folders
- BIP document, Vagrantfile, justfile
- Ordinals handbook link from nav
- Broken tag parsing and `--parent` flag (temporarily)

## Upstream History

For the changelog of the original ord project (versions 0.0.1 through 0.5.1),
see [ordinals/ord releases](https://github.com/ordinals/ord/releases).
