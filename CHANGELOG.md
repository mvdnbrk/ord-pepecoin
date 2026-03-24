# Changelog

All notable changes to ordpep are documented in this file.

This project is forked from [apezord/ord-dogecoin](https://github.com/apezord/ord-dogecoin),
which is itself forked from [ordinals/ord](https://github.com/ordinals/ord) `0.5.1`.

## [0.11.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.11.0) - 2026-03-24

### Added
- PRC-721 traits support for inscriptions ([#57](https://github.com/mvdnbrk/ord-pepecoin/pull/57))
  - `--json-traits <file>` flag for single inscriptions
  - Batch YAML `traits` field for collection inscriptions
  - Trait values: strings, booleans, integers, null
  - Strict validation: duplicate names and unsupported types invalidate entire properties tag
  - Case-insensitive duplicate trait name check on inscribe
  - Order-preserving: traits displayed in the order the inscriber intended
  - Integer CBOR keys for properties (saves ~12 bytes per inscription)
  - JSON API: traits object on `/inscription/{id}`
  - HTML: traits displayed in inscription detail page
- PRC-721 support for body compression ([#59](https://github.com/mvdnbrk/ord-pepecoin/pull/59))
  - `--compress` flag for Brotli content compression
  - Smart comparison: only keeps compressed version if smaller than original
  - Content-type-aware Brotli mode selection (text, font, generic)
  - Roundtrip decompression verification after compression
  - Server serves `Content-Encoding: br` header for compressed inscriptions
  - Content encoding displayed on inscription detail page
  - Works with both single and batch inscription flows

### Changed
- Text preview fetches content via JS instead of server-side rendering ([#59](https://github.com/mvdnbrk/ord-pepecoin/pull/59))

## [0.10.2](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.10.2) - 2026-03-24

### Added
- Link address on inscription page to address page ([`49a02bc`](https://github.com/mvdnbrk/ord-pepecoin/commit/49a02bc1))

### Changed
- Sort inscriptions on address page by number descending ([`2a41447`](https://github.com/mvdnbrk/ord-pepecoin/commit/2a41447a))
- Change link color to green ([`40d9004`](https://github.com/mvdnbrk/ord-pepecoin/commit/40d90045))

### Fixed
- Fix search resolving Pepecoin addresses to `/sat` instead of `/address` ([`60e4338`](https://github.com/mvdnbrk/ord-pepecoin/commit/60e43386))

## [0.10.1](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.10.1) - 2026-03-23

### Fixed
- Fix PRC-721 tags lost in multi-tx reveal chains ([#56](https://github.com/mvdnbrk/ord-pepecoin/pull/56))

## [0.10.0](https://github.com/mvdnbrk/ord-pepecoin/releases/tag/0.10.0) - 2026-03-23

### Added
- PRC-721 title property support for inscriptions ([#53](https://github.com/mvdnbrk/ord-pepecoin/pull/53))
  - `inscribe --title <TITLE>` flag for single inscriptions
  - Batch YAML `title` field for structured collection metadata
  - Properties stored in `properties` (CBOR) or `properties;br` (Brotli-compressed) tags
  - Auto-compression: indexer automatically chooses smaller representation on-chain
  - JSON API: `properties` object with `title` on `/inscription/{id}`
  - HTML: title displayed as subtitle and in metadata list (with full escaping)
- PRC-721 parent/child inscription support ([#50](https://github.com/mvdnbrk/ord-pepecoin/pull/50))
  - `inscribe --parent <ID>` for single child inscriptions
  - Batch YAML `parents` field with multi-parent support and UTXO chaining
  - Indexer: parent/child relationship tracking with explicit parent-to-output assignment
  - JSON API: `parent_count`, `parents`, `child_count`, `children` on `/inscription/{id}`
  - Paginated JSON endpoints: `/children/{id}` and `/parents/{id}`
  - HTML: parent/child sections on inscription pages
- Delegate inscription support ([#52](https://github.com/mvdnbrk/ord-pepecoin/pull/52))
  - `inscribe --delegate <ID>` to reference another inscription's content
  - Batch YAML `delegate` field (mutually exclusive with `file`)
  - Delegate resolution in `/content` and `/preview` handlers
  - JSON API: `delegate`, `effective_content_type` on `/inscription/{id}`
  - HTML: delegate link on inscription pages
  - No-chaining validation: delegate target must be a content inscription

### Changed
- Extract `Properties` struct for PRC-721 properties encoding ([#54](https://github.com/mvdnbrk/ord-pepecoin/pull/54))
  - Enforce spec limits: max 4,000 bytes uncompressed, max 30:1 compression ratio
  - Skip Brotli compression for properties under 64 bytes
  - Trim whitespace from title on read and write
  - Builder pattern: `Properties::default().with_title("...")`
- Deduplicate inscription setup helpers in inscribe ([#55](https://github.com/mvdnbrk/ord-pepecoin/pull/55))
- Centralize wallet fee rate and postage defaults ([`6822366`](https://github.com/mvdnbrk/ord-pepecoin/commit/6822366c))
- Remove unused `take`/`take_all` from tag module ([`923cec2`](https://github.com/mvdnbrk/ord-pepecoin/commit/923cec23))
- Change positional `file` arg to `--file` flag ([#52](https://github.com/mvdnbrk/ord-pepecoin/pull/52))
- Extract `AcceptJson` into server submodule ([#50](https://github.com/mvdnbrk/ord-pepecoin/pull/50))
- Extract `InscriptionParser` into separate `parser` module ([#48](https://github.com/mvdnbrk/ord-pepecoin/pull/48))
- Add clippy and rustfmt CI checks ([#47](https://github.com/mvdnbrk/ord-pepecoin/pull/47))
- Restructure inscription modules into `inscriptions/` directory ([#46](https://github.com/mvdnbrk/ord-pepecoin/pull/46))

### Removed
- Remove `/bounties` and `/faq` routes ([#51](https://github.com/mvdnbrk/ord-pepecoin/issues/51))

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
