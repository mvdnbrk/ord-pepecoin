# Research Report: Parent/Child Inscriptions and Delegates for P2SH scriptSig

## 1. Fork Survey Results

We investigated several other scriptSig-based ord forks (such as `AstroxNetwork/ord_dogecoin`, `verydogelabs/wonux`, and various `ord-dogecoin` variants). Many of these forks either:
1. Do not support parent/child or delegates at all.
2. Implemented parent/child by breaking backwards compatibility (changing the expected sequence of pushes).
3. Hardcoded specific string delimiters before body chunks.

There is no universally accepted standard for adding tags to the legacy P2SH scriptSig countdown format. Most scriptSig ecosystems have lagged behind Bitcoin's Taproot implementation in advanced features like delegates and recursive collections.

## 2. Upstream Implementation Summary (ordinals/ord)

Upstream `ord` uses Bitcoin's Taproot witness scripts for its envelope:
`OP_FALSE OP_IF "ord" <TAG> <VALUE> <TAG> <VALUE> ... OP_ENDIF`

**Parent/Child:**
- **Tag:** `3` (Parent)
- **Encoding:** The tag is pushed, followed by the parent inscription ID (36 bytes: 32-byte txid + 4-byte little-endian index).
- **Transaction:** The child's reveal transaction **MUST** spend the parent inscription UTXO as one of its inputs. The `TransactionBuilder` and `batch::Plan` explicitly pull the parent UTXO into `reveal_inputs` and create a corresponding output in `reveal_outputs` to send the parent back to the user (or a specified destination).
- **Validation:** The indexer verifies that the transaction creating the child actually spent the parent inscription.

**Delegates:**
- **Tag:** `11` (Delegate)
- **Encoding:** Similar to parent, followed by the delegate inscription ID.
- **Mechanics:** When the server receives a request for `/content/<id>`, if the inscription has a delegate, it serves the content and content-type of the delegate inscription instead.
- **Usage:** This allows creating a 10,000-item PFP collection where all items are just a few bytes pointing to a high-resolution delegate, massively saving on chain space and fees.

## 3. Format Option Analysis

We must solve the collision between tag numbers (e.g., `3`) and the chunk countdown (e.g., an inscription with 4 chunks starts its countdown at `3`).

| Option | Unambiguous? | Backwards Compatible? (Old parsers) | Parser Complexity | Space Efficiency | Multi-tx Support |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **A: Tags BEFORE npieces** | **No**. If a parser sees `3`, it cannot definitively tell if it's `tag_3` or `npieces=3` without complex lookahead. | **Breaks**. Old parsers expect `npieces` immediately after `"ord"`. They will misinterpret tags as `npieces` and fail to index the inscription. | High | Excellent | Yes |
| **B: Explicit push encoding** (`OP_PUSH1 0x03` vs `OP_PUSHNUM_3`) | **Yes**, technically. | **Breaks wildly**. Old parsers don't distinguish between push opcodes; they extract the pushed bytes. An old parser will see `3` and treat it as a countdown, causing massive corruption and misparsing of the envelope. | Very High (requires tracking opcode types) | Excellent | Yes |
| **C: Tag section with delimiter** (e.g., `["tags"]`) | **Yes**. A clear string delimiter separates metadata from the countdown. | **Safe ignore**. Old parsers will try to parse `"tags"` as a countdown integer, get a massive number, realize it doesn't match `npieces - 1`, and abort. | Low | Good (costs a few extra bytes for delimiters) | Yes |
| **D: Negative/high tag numbers** (e.g., `0xFF03`) | **Yes**. As long as the tag ID exceeds the maximum possible number of chunks (e.g., > 65,000). | **Safe ignore**. Old parsers see a massive countdown number, it doesn't match `npieces - 1`, so they abort. | Low | Excellent | Yes |
| **E: Header-only tags** | **No**. If `npieces` is exactly 4, the first body chunk expects a countdown of `3`. This directly collides with `tag_3`, making it impossible to know if the push is a parent ID or file content. | **Breaks**. Old parsers will treat `tag_3` as the countdown and the `parent_id` as the body chunk, resulting in corrupted content. | Medium | Excellent | Yes |

## 4. Recommendation

**Recommended approach: Option D (High Tag Numbers / String Tags)** or a variant of **Option C (String Delimiters)**.

**Rationale:**
Option E and Option A are fundamentally ambiguous. Option B causes dangerous misparsing in old indexers. 
The safest and most robust method for scriptSig is to use **String Tags** instead of integer tags (effectively a variant of Option D). 

Instead of pushing `3` for parent, push the string `"parent"`. 
```
"ord" [npieces] [content_type] ["parent"] [parent_id] ["delegate"] [delegate_id] [n-1] [chunk] ...
```
- **Unambiguous**: A new parser simply checks if the data matches `"parent"`, `"delegate"`, or `"metadata"`. If it doesn't, it assumes it's the countdown number.
- **Backwards Compatibility**: An old v0.5.1 parser will try to convert `"parent"` into a `u64`. "parent" is 6 bytes (`0x70 0x61 0x72 0x65 0x6E 0x74`), which results in a massive integer. This will never match `npieces - 1`. The old parser will cleanly abort parsing, resulting in `ParsedInscription::None`. This is the safest failure mode.
- **Complexity**: Trivial to implement in `InscriptionParser::parse`.
- **Space**: Costs only a few bytes per tag (e.g., `"parent"` is 6 bytes vs 1 byte for `OP_PUSHNUM_3`), which is negligible compared to the payload size.

## 5. Parent UTXO Placement

In our multi-tx reveal chains (which differ from upstream's single-tx limits):
- The **parent inscription UTXO MUST be spent in the FIRST reveal transaction**. 
- Why? The child inscription ID is generated from the txid of the *first* reveal transaction (`txid:0`). If the parent UTXO is spent in a later transaction in the chain, it becomes extremely difficult for the indexer to cryptographically link the parent spend to the *start* of the child inscription.
- **Transaction Builder Impact**: `TransactionBuilder` must select the parent UTXO and place it as an input in the first reveal tx, alongside the commit UTXO.

## 6. Backwards Compatibility Assessment

Using String Tags or High Integer Tags (Option D/C):
- **Old parsers reading new tagged inscriptions**: They will safely fail to parse them and ignore them (`ParsedInscription::None`). This is good; we don't want old nodes serving corrupted files.
- **New parsers reading old inscriptions**: They will look for tags, find none (because they encounter the countdown integer immediately after the content type), and parse the body exactly as before. 100% compatible.
- **Database Migration**: No migration is needed for existing data. The new indexer will simply begin populating the new `PARENT_TO_CHILD` and `DELEGATE` tables for newly indexed blocks.

## 7. Critical Issue: Inscription Number Divergence

The string-tags-before-countdown approach (Section 4 recommendation) has a serious flaw: old parsers that encounter tags will fail to parse the inscription entirely (`ParsedInscription::None`). This means old indexers **won't assign an inscription number** to tagged inscriptions, causing every subsequent inscription number to diverge between old and new indexers. This cascading desync breaks the entire numbering scheme across the ecosystem.

**Requirement:** Old parsers must still be able to parse the body and index tagged inscriptions — just without understanding the tag metadata.

**Revised approach: Tags AFTER the body**

```
"ord" [npieces] [content_type] [n-1] [chunk] ... [0] [chunk] ["parent"] [parent_id] ["delegate"] [delegate_id]
```

- Old parsers stop reading after the countdown hits `0`, never see the tags, but still index the inscription with correct content and numbering.
- New parsers keep reading after countdown `0` to pick up optional tags.
- Inscription numbers stay consistent across all indexer versions.
- Backwards compatible in both directions: old parsers index new inscriptions, new parsers index old inscriptions.

**Opcode-level example — current broken approach (tags before countdown):**

A 4-chunk inscription with a parent would produce:

```
OP_PUSHBYTES_3 "ord"          # protocol ID
OP_PUSHNUM_4                  # npieces = 4
OP_PUSHBYTES_9 "image/png"    # content_type
OP_PUSHNUM_3                  # tag 3 (parent) — COLLIDES with countdown 3!
OP_PUSHBYTES_36 <parent_id>   # old parser thinks this is a body chunk
OP_PUSHNUM_3                  # countdown 3 — old parser sees duplicate countdown
OP_PUSHDATA1 0xF0 <chunk>     # 240-byte chunk
OP_PUSHNUM_2                  # countdown 2
OP_PUSHDATA1 0xF0 <chunk>
OP_PUSHNUM_1                  # countdown 1
OP_PUSHDATA1 0xF0 <chunk>
OP_PUSHBYTES_0                # countdown 0
OP_PUSHDATA1 0xF0 <chunk>
```

Old parser sees the first `OP_PUSHNUM_3` after content_type and thinks the countdown started — treats parent_id as a body chunk. Completely broken.

**Opcode-level example — tags AFTER body (proposed):**

```
OP_PUSHBYTES_3 "ord"          # protocol ID
OP_PUSHNUM_4                  # npieces = 4
OP_PUSHBYTES_9 "image/png"    # content_type
OP_PUSHNUM_3                  # countdown 3 (unambiguous)
OP_PUSHDATA1 0xF0 <chunk>     # 240-byte chunk
OP_PUSHNUM_2                  # countdown 2
OP_PUSHDATA1 0xF0 <chunk>
OP_PUSHNUM_1                  # countdown 1
OP_PUSHDATA1 0xF0 <chunk>
OP_PUSHBYTES_0                # countdown 0
OP_PUSHDATA1 0xF0 <chunk>
OP_PUSHBYTES_6 "parent"       # string tag (old parsers already stopped at countdown 0)
OP_PUSHBYTES_36 <parent_id>   # 36 bytes: 32-byte txid + 4-byte LE index
OP_PUSHBYTES_8 "delegate"     # string tag
OP_PUSHBYTES_36 <delegate_id> # 36 bytes
```

Old parsers stop reading after countdown hits 0 — they never see the tags. New parsers continue reading after countdown 0 to pick up optional tag/value pairs. No collision, no number divergence.

**Confirmed by existing code:** The `valid_with_extra_data` test in `src/inscription.rs` already proves old parsers ignore data after countdown 0. The parser stops reading body chunks when the countdown finishes, and any trailing data is silently discarded. This is exactly the behavior we need.

**Delegate inscriptions (no body):** Old parsers handle `npieces=0` correctly — they return `ParsedInscription::Complete` with `body: None` (line 203-208 in `inscription.rs`). The delegate tags after the empty body are ignored. The inscription gets indexed and numbered correctly, it just serves empty content on old indexers. This is acceptable graceful degradation — no number divergence.

### Multi-tx reveal chain: tags in the LAST tx

Cross-checked with Grok — tags belong exclusively in the **final reveal transaction**, after countdown 0 and the last chunk:

**Example: 3-tx chain, npieces=10:**
```
Reveal tx1: "ord" OP_PUSHNUM_10 "image/png"
            OP_PUSHNUM_9 <chunk1> OP_PUSHNUM_8 <chunk2> OP_PUSHNUM_7 <chunk3>
            (also spends parent UTXO as input if this is a child inscription)

Reveal tx2: OP_PUSHNUM_6 <chunk4> OP_PUSHNUM_5 <chunk5> OP_PUSHNUM_4 <chunk6>

Reveal tx3: OP_PUSHNUM_3 <chunk7> OP_PUSHNUM_2 <chunk8> OP_PUSHNUM_1 <chunk9>
            OP_PUSHBYTES_0 <chunk10>
            OP_PUSHBYTES_6 "parent"   OP_PUSHBYTES_36 <parent_id>
            OP_PUSHBYTES_8 "delegate" OP_PUSHBYTES_36 <delegate_id>
```

**Rationale:**
- The full envelope is only complete in the last tx — tags naturally append where the body ends
- Indexers reconstruct inscriptions by following the chain and concatenating pushes → tags appear at the end of the reconstructed envelope
- Parent linkage is **proven by the first reveal tx spending the parent UTXO** (cryptographic proof) — the tag in the final tx just records the parent ID for metadata/UI
- Avoids putting tags in every tx (wasteful) or early txs (breaks chain reconstruction if partial)

**Parser logic:**
1. Track chain via prevouts, concatenate all pushes from tx1→txN
2. Parse countdown to 0 + last chunk → body complete, assign inscription number
3. If pushes remain → parse as tag/value pairs (string key → value bytes)
4. For parent tag: cross-check that the first reveal tx inputs include the parent's UTXO

**Tag format — extensible key/value pairs:**
```
OP_PUSHBYTES_6 "parent"   OP_PUSHBYTES_36 <parent_id>
OP_PUSHBYTES_8 "delegate" OP_PUSHBYTES_36 <delegate_id>
OP_PUSHBYTES_8 "metadata" OP_PUSHDATA1 <varlen JSON or CBOR>
```

Multiple parents supported by repeating the tag:
```
OP_PUSHBYTES_6 "parent" OP_PUSHBYTES_36 <id1>
OP_PUSHBYTES_6 "parent" OP_PUSHBYTES_36 <id2>
```

## 8. Implementation Sketch

1. **`src/inscription.rs`**: 
   - Update `Inscription` struct to include `parent: Option<InscriptionId>` and `delegate: Option<InscriptionId>`.
   - Update `get_inscription_script()` to push `"parent"` and `"delegate"` strings followed by their serialized IDs.
   - Update `InscriptionParser::parse` to check for these string tags before checking for the countdown number.
2. **`src/api.rs`**:
   - Update API structs to expose `parent` and `delegate`.
3. **`src/index.rs` & `src/index/updater.rs`**:
   - Add new Redb tables: `INSCRIPTION_ID_TO_PARENTS` and `INSCRIPTION_ID_TO_DELEGATE`.
   - In `updater.rs`, verify that the parent UTXO is present in the inputs of the transaction creating the child.
4. **`src/subcommand/server.rs`**:
   - Update the `/content/{id}` route to resolve delegates (if an inscription has a delegate, fetch the delegate's content instead).
   - Update the `/inscription/{id}` HTML template to display parent and delegate links.
5. **`src/subcommand/wallet/transaction_builder.rs` & `batch/plan.rs`**:
   - Add parameters to accept parent inscriptions.
   - Inject the parent UTXO into the inputs of the first reveal transaction.
   - Route the parent UTXO to a change output so it isn't lost.