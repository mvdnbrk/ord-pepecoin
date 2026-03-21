# Changelog

All notable changes to ordpep are documented in this file.

This project is forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin),
which is itself forked from [ordinals/ord](https://github.com/ordinals/ord) `0.5.1`.

## [Unreleased]

### Changed
- Extract `InscriptionParser` into separate `parser` module ([#48](https://github.com/mvdnbrk/ord-pepecoin/pull/48))
- Add clippy and rustfmt CI checks ([#47](https://github.com/mvdnbrk/ord-pepecoin/pull/47))
- Restructure inscription modules into `inscriptions/` directory ([#46](https://github.com/mvdnbrk/ord-pepecoin/pull/46))

### Fixed
- Fix macOS build failure caused by type inference overflow ([#46](https://github.com/mvdnbrk/ord-pepecoin/pull/46))

## [0.9.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.9.0) - 2026-03-20

### Added
- Batch chunking for large collections with deferred commit broadcasting ([#41](https://github.com/mvdnbrk/ord-pepecoin/pull/41))
- Batched reveal broadcast for large inscriptions exceeding mempool chain limit ([#36](https://github.com/mvdnbrk/ord-pepecoin/pull/36))
- Job file persistence for reveal broadcasts with automatic server-side processing every 60s ([#36](https://github.com/mvdnbrk/ord-pepecoin/pull/36))
- `wallet broadcast` command for manual job processing ([#36](https://github.com/mvdnbrk/ord-pepecoin/pull/36))
- Batch directory structure for collection inscriptions with sliding window (max 100 active jobs) ([#36](https://github.com/mvdnbrk/ord-pepecoin/pull/36))
- Dry-run works without requiring wallet balance ([#36](https://github.com/mvdnbrk/ord-pepecoin/pull/36))
- `wallet list` command to show all wallets ([`c2bddb2`](https://github.com/mvdnbrk/ord-pepecoin/commit/c2bddb2b))
- Allow cross-inscription references via CSP (e.g. `<script src='/content/...'>`) ([#34](https://github.com/mvdnbrk/ord-pepecoin/pull/34))
- Align media types with upstream: `Code(Language)`, `Font`, `Markdown`, `Model`, `Image(ImageRendering)` ([#29](https://github.com/mvdnbrk/ord-pepecoin/pull/29))
- Preview templates for code (highlight.js), fonts, markdown, and 3D models ([#29](https://github.com/mvdnbrk/ord-pepecoin/pull/29))
- Unified `Settings` struct with `ORDPEP_*` env var support ([#32](https://github.com/mvdnbrk/ord-pepecoin/pull/32))
- Configurable `savepoint_interval`, `max_savepoints`, `commit_interval`, `pepecoin_rpc_limit` ([#32](https://github.com/mvdnbrk/ord-pepecoin/pull/32))
- PRC-721 extended inscription envelope specification ([docs/prc-721.md](docs/prc-721.md))

### Changed
- Restructure inscription modules into `inscriptions/` directory ([#46](https://github.com/mvdnbrk/ord-pepecoin/pull/46))

### Fixed
- Fix macOS build failure caused by type inference overflow ([#46](https://github.com/mvdnbrk/ord-pepecoin/pull/46))
- Filter wallet balance, UTXOs and transactions by wallet addresses ([#37](https://github.com/mvdnbrk/ord-pepecoin/pull/37))
- `index compact` failing when persistent savepoints exist ([`bd792cb`](https://github.com/mvdnbrk/ord-pepecoin/commit/bd792cbe))
- Flaky server tests with index update retry ([#31](https://github.com/mvdnbrk/ord-pepecoin/pull/31))
- Leftover Shibescribe references in help text ([`9fa6f1c`](https://github.com/mvdnbrk/ord-pepecoin/commit/9fa6f1cf))

### Changed
- Refactor `CommandBuilder` with `.wallet()` builder pattern for test wallet flags ([#30](https://github.com/mvdnbrk/ord-pepecoin/pull/30))
- Default `max_savepoints` bumped from 2 to 3 for safer reorg recovery ([#32](https://github.com/mvdnbrk/ord-pepecoin/pull/32))
- Extract `sign_reveal_chain` and add job unit tests ([`63131ec`](https://github.com/mvdnbrk/ord-pepecoin/commit/63131ec2))

## [0.8.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.8.0) - 2026-03-17

### Fixed
- `/block/{height}` JSON endpoint returning same inscriptions for every block ([#26](https://github.com/mvdnbrk/ord-pepecoin/pull/26))
- Inscription preview empty due to content type spacing mismatch ([`266d784`](https://github.com/mvdnbrk/ord-pepecoin/commit/266d784e))
- Inscription preview empty for content types with extra parameters ([`5cef247`](https://github.com/mvdnbrk/ord-pepecoin/commit/5cef247a))
- Savepoint check in indexing loop causing unnecessary RPC calls and slower sync ([`f810018`](https://github.com/mvdnbrk/ord-pepecoin/commit/f810018b))

### Added
- Block page shows featured inscriptions, `/inscriptions/block/{height}` paginated endpoint ([#26](https://github.com/mvdnbrk/ord-pepecoin/pull/26))
- `HEIGHT_TO_LAST_INSCRIPTION_NUMBER` lookup table for O(1) inscription range queries ([#26](https://github.com/mvdnbrk/ord-pepecoin/pull/26))

### Changed
- Home page: show 20 latest inscriptions (was 10) and 5 latest blocks (was 100)
- Align height and inscription number types from `u64` to `u32` to match upstream ([#28](https://github.com/mvdnbrk/ord-pepecoin/pull/28))

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
