use crate::api::OutputInfo;
use std::io::Cursor;

use {
  self::{
    entry::{
      BlockHashValue, Entry, InscriptionEntry, InscriptionEntryValue, InscriptionIdValue,
      OutPointValue, SatPointValue, SatRange,
    },
    updater::Updater,
  },
  super::*,
  bitcoin::BlockHeader,
  bitcoincore_rpc::{json::GetBlockHeaderResult, Auth, Client},
  chrono::SubsecRound,
  indicatif::{ProgressBar, ProgressStyle},
  log::log_enabled,
  redb::{
    Database, MultimapTableDefinition, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    Table, TableDefinition, TableError, WriteTransaction,
  },
  std::collections::HashMap,
  std::io::{self, BufWriter, Write},
  std::sync::atomic::{self, AtomicBool},
};
mod entry;
mod fetcher;
pub(crate) mod reorg;
mod rtx;
mod updater;

macro_rules! define_table {
  ($name:ident, $key:ty, $value:ty) => {
    const $name: TableDefinition<$key, $value> = TableDefinition::new(stringify!($name));
  };
}

macro_rules! define_multimap_table {
  ($name:ident, $key:ty, $value:ty) => {
    const $name: MultimapTableDefinition<$key, $value> =
      MultimapTableDefinition::new(stringify!($name));
  };
}

define_table! { HEIGHT_TO_BLOCK_HASH, u32, &BlockHashValue }
define_table! { HEIGHT_TO_LAST_INSCRIPTION_NUMBER, u32, u32 }
define_table! { INSCRIPTION_ID_TO_INSCRIPTION_ENTRY, &InscriptionIdValue, InscriptionEntryValue }
define_table! { INSCRIPTION_ID_TO_SATPOINT, &InscriptionIdValue, &SatPointValue }
define_table! { INSCRIPTION_NUMBER_TO_INSCRIPTION_ID, u32, &InscriptionIdValue }
define_table! { INSCRIPTION_ID_TO_TXIDS, &InscriptionIdValue, &[u8] }
define_table! { INSCRIPTION_TXID_TO_TX, &[u8], &[u8] }
define_table! { PARTIAL_TXID_TO_INSCRIPTION_TXIDS, &[u8], &[u8] }
define_table! { OUTPOINT_TO_SAT_RANGES, &OutPointValue, &[u8] }
define_table! { OUTPOINT_TO_VALUE, &OutPointValue, u64}
define_table! { SATPOINT_TO_INSCRIPTION_ID, &SatPointValue, &InscriptionIdValue }
define_table! { SAT_TO_INSCRIPTION_ID, u128, &InscriptionIdValue }
define_table! { SAT_TO_SATPOINT, u128, &SatPointValue }
define_table! { STATISTIC_TO_COUNT, u64, u64 }
define_table! { WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP, u64, u128 }
define_multimap_table! { ADDRESS_TO_INSCRIPTION_IDS, &str, &InscriptionIdValue }
define_table! { INSCRIPTION_ID_TO_ADDRESS, &InscriptionIdValue, &str }

const SCHEMA_VERSION: u64 = 6;

pub struct Index {
  auth: Auth,
  client: Client,
  database: Database,
  path: PathBuf,
  first_inscription_height: u32,
  genesis_block_coinbase_transaction: Transaction,
  genesis_block_coinbase_txid: Txid,
  height_limit: Option<u32>,
  pub(crate) started: DateTime<Utc>,
  unrecoverably_reorged: AtomicBool,
  rpc_url: String,
  chain: Chain,
  pub(crate) settings: Settings,
}

#[derive(Debug, PartialEq)]
pub enum List {
  Spent,
  Unspent(Vec<(u128, u128)>),
}

#[derive(Copy, Clone)]
#[repr(u64)]
pub(crate) enum Statistic {
  Schema = 0,
  Commits = 1,
  LostSats = 2,
  OutputsTraversed = 3,
  SatRanges = 4,
  LastSavepointHeight = 5,
}

impl Statistic {
  fn key(self) -> u64 {
    self.into()
  }
}

impl From<Statistic> for u64 {
  fn from(statistic: Statistic) -> Self {
    statistic as u64
  }
}

#[derive(Serialize)]
pub(crate) struct Info {
  pub(crate) blocks_indexed: u64,
  pub(crate) branch_pages: usize,
  pub(crate) fragmented_bytes: usize,
  pub(crate) index_file_size: u64,
  pub(crate) index_path: PathBuf,
  pub(crate) leaf_pages: usize,
  pub(crate) metadata_bytes: usize,
  pub(crate) outputs_traversed: u64,
  pub(crate) page_size: usize,
  pub(crate) sat_ranges: u64,
  pub(crate) stored_bytes: usize,
  pub(crate) transactions: Vec<TransactionInfo>,
  pub(crate) tree_height: usize,
  pub(crate) utxos_indexed: usize,
}

#[derive(Serialize)]
pub(crate) struct TransactionInfo {
  pub(crate) starting_block_count: u64,
  pub(crate) starting_timestamp: u128,
}

trait BitcoinCoreRpcResultExt<T> {
  fn into_option(self) -> Result<Option<T>>;
}

impl<T> BitcoinCoreRpcResultExt<T> for Result<T, bitcoincore_rpc::Error> {
  fn into_option(self) -> Result<Option<T>> {
    match self {
      Ok(ok) => Ok(Some(ok)),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { code: -8, .. },
      )))
      | Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { code: -5, .. },
      ))) => Ok(None),
      Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::error::Error::Rpc(
        bitcoincore_rpc::jsonrpc::error::RpcError { message, .. },
      )))
        if message.ends_with("not found") =>
      {
        Ok(None)
      }
      Err(err) => Err(err.into()),
    }
  }
}

impl Index {
  pub fn open(settings: &Settings) -> Result<Self> {
    let rpc_url = settings.rpc_url();
    let auth = settings.auth()?;

    match &auth {
      Auth::CookieFile(path) => log::info!(
        "Connecting to Pepecoin Core RPC server at {rpc_url} using cookie file `{}`",
        path.display()
      ),
      Auth::UserPass(user, _) => {
        log::info!("Connecting to Pepecoin Core RPC server at {rpc_url} as user `{user}`")
      }
      Auth::None => {
        log::info!("Connecting to Pepecoin Core RPC server at {rpc_url} without authentication")
      }
    }

    let client = Client::new(&rpc_url, auth.clone()).context("failed to connect to RPC URL")?;

    let data_dir = settings.data_dir();

    if let Err(err) = fs::create_dir_all(&data_dir) {
      bail!("failed to create data dir `{}`: {err}", data_dir.display());
    }

    let index_sats = settings.index_sats();

    let path = settings.index();

    let database: Database = if path.exists() {
      let database = Database::builder()
        .set_cache_size(1024 * 1024 * 1024)
        .open(&path)?;

      let schema_version = database
        .begin_read()?
        .open_table(STATISTIC_TO_COUNT)?
        .get(&Statistic::Schema.key())?
        .map(|x: redb::AccessGuard<u64>| x.value())
        .unwrap_or(0);

      match schema_version.cmp(&SCHEMA_VERSION) {
        cmp::Ordering::Less =>
          bail!(
            "index at `{}` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
            path.display()
          ),
        cmp::Ordering::Greater =>
          bail!(
            "index at `{}` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {schema_version}, ord schema {SCHEMA_VERSION}",
            path.display()
          ),
        cmp::Ordering::Equal => {
        }
      }

      database
    } else {
      let database = Database::builder()
        .set_cache_size(1024 * 1024 * 1024)
        .create(&path)?;
      let mut tx = database.begin_write()?;
      tx.set_quick_repair(true);

      tx.open_table(HEIGHT_TO_BLOCK_HASH)?;
      tx.open_table(HEIGHT_TO_LAST_INSCRIPTION_NUMBER)?;
      tx.open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?;
      tx.open_table(INSCRIPTION_ID_TO_SATPOINT)?;
      tx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;
      tx.open_table(INSCRIPTION_ID_TO_TXIDS)?;
      tx.open_table(INSCRIPTION_TXID_TO_TX)?;
      tx.open_table(PARTIAL_TXID_TO_INSCRIPTION_TXIDS)?;
      tx.open_table(OUTPOINT_TO_VALUE)?;
      tx.open_table(SATPOINT_TO_INSCRIPTION_ID)?;
      tx.open_table(SAT_TO_INSCRIPTION_ID)?;
      tx.open_table(SAT_TO_SATPOINT)?;
      tx.open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?;
      tx.open_multimap_table(ADDRESS_TO_INSCRIPTION_IDS)?;
      tx.open_table(INSCRIPTION_ID_TO_ADDRESS)?;

      tx.open_table(STATISTIC_TO_COUNT)?
        .insert(&Statistic::Schema.key(), &SCHEMA_VERSION)?;

      if index_sats {
        tx.open_table(OUTPOINT_TO_SAT_RANGES)?
          .insert(&OutPoint::null().store(), [].as_slice())?;
      }

      tx.commit()?;

      database
    };

    let genesis_block_coinbase_transaction =
      settings.chain().genesis_block().coinbase().unwrap().clone();

    Ok(Self {
      genesis_block_coinbase_txid: genesis_block_coinbase_transaction.txid(),
      auth,
      client,
      database,
      path,
      first_inscription_height: settings.first_inscription_height(),
      genesis_block_coinbase_transaction,
      height_limit: settings.height_limit(),
      started: Utc::now(),
      unrecoverably_reorged: AtomicBool::new(false),
      rpc_url,
      chain: settings.chain(),
      settings: settings.clone(),
    })
  }

  pub(crate) fn index_file_size(&self) -> u64 {
    fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
  }

  pub(crate) fn has_sat_index(&self) -> Result<bool> {
    match self.begin_read()?.0.open_table(OUTPOINT_TO_SAT_RANGES) {
      Ok(_) => Ok(true),
      Err(TableError::TableDoesNotExist(_)) => Ok(false),
      Err(err) => Err(err.into()),
    }
  }

  fn require_sat_index(&self, feature: &str) -> Result {
    if !self.has_sat_index()? {
      bail!("{feature} requires index created with `--index-sats` flag")
    }

    Ok(())
  }

  pub(crate) fn info(&self) -> Result<Info> {
    let wtx = self.begin_write()?;

    let stats = wtx.stats()?;

    let info = {
      let statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
      let sat_ranges = statistic_to_count
        .get(&Statistic::SatRanges.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      let outputs_traversed = statistic_to_count
        .get(&Statistic::OutputsTraversed.key())?
        .map(|x| x.value())
        .unwrap_or(0);
      Info {
        index_path: self.path.clone(),
        blocks_indexed: u64::from(
          wtx
            .open_table(HEIGHT_TO_BLOCK_HASH)?
            .range(0..)?
            .next_back()
            .map(|result| {
              let (height, _hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
              height.value() + 1
            })
            .unwrap_or(0),
        ),
        branch_pages: usize::try_from(stats.branch_pages()).unwrap(),
        fragmented_bytes: usize::try_from(stats.fragmented_bytes()).unwrap(),
        index_file_size: fs::metadata(&self.path)?.len(),
        leaf_pages: usize::try_from(stats.leaf_pages()).unwrap(),
        metadata_bytes: usize::try_from(stats.metadata_bytes()).unwrap(),
        sat_ranges,
        outputs_traversed,
        page_size: stats.page_size(),
        stored_bytes: usize::try_from(stats.stored_bytes()).unwrap(),
        transactions: wtx
          .open_table(WRITE_TRANSACTION_STARTING_BLOCK_COUNT_TO_TIMESTAMP)?
          .range(0..)?
          .map(|result| {
            let (starting_block_count, starting_timestamp) =
              result.expect("Error reading from starting block count table");
            TransactionInfo {
              starting_block_count: starting_block_count.value(),
              starting_timestamp: starting_timestamp.value(),
            }
          })
          .collect(),
        tree_height: usize::try_from(stats.tree_height()).unwrap(),
        utxos_indexed: usize::try_from(wtx.open_table(OUTPOINT_TO_SAT_RANGES)?.len()?).unwrap(),
      }
    };

    Ok(info)
  }

  pub(crate) fn update(&self) -> Result {
    loop {
      match Updater::update(self) {
        Ok(()) => return Ok(()),
        Err(error) => {
          if let Some(reorg_error) = error.downcast_ref::<reorg::Error>() {
            match reorg_error {
              reorg::Error::Recoverable { height, depth } => {
                match reorg::Reorg::handle_reorg(self, *height, *depth) {
                  Ok(()) => {
                    log::info!("recovered from reorg, resuming indexing...");
                    continue;
                  }
                  Err(recovery_error) => {
                    self
                      .unrecoverably_reorged
                      .store(true, atomic::Ordering::Relaxed);
                    return Err(recovery_error);
                  }
                }
              }
              reorg::Error::Unrecoverable => {
                self
                  .unrecoverably_reorged
                  .store(true, atomic::Ordering::Relaxed);
                return Err(error);
              }
            }
          }
          return Err(error);
        }
      }
    }
  }

  pub(crate) fn export(&self, tsv: Option<PathBuf>, include_addresses: bool) -> Result {
    let mut writer: Box<dyn Write> = match tsv {
      Some(path) => Box::new(BufWriter::new(File::create(path)?)),
      None => Box::new(BufWriter::new(io::stdout())),
    };

    let rtx = self.database.begin_read()?;

    let blocks_indexed = rtx
      .open_table(HEIGHT_TO_BLOCK_HASH)?
      .range(0..)?
      .next_back()
      .map(|result| {
        let (height, _hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
        height.value() + 1
      })
      .unwrap_or(0);

    writeln!(writer, "# export at block height {}", blocks_indexed)?;

    for result in rtx
      .open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?
      .range(0..)?
    {
      let (number, id) = result.expect("Error reading from inscription number table");
      let inscription_id = InscriptionId::load(*id.value());

      let satpoint = self
        .get_inscription_satpoint_by_id(inscription_id)?
        .ok_or_else(|| anyhow!("inscription {inscription_id} has no satpoint"))?;

      write!(
        writer,
        "{}\t{}\t{}",
        number.value(),
        inscription_id,
        satpoint
      )?;

      if include_addresses {
        let address = if satpoint.outpoint == OutPoint::null() {
          "unbound".to_string()
        } else {
          match self.get_transaction(satpoint.outpoint.txid)? {
            Some(tx) => {
              if let Some(_output) = tx.output.get(satpoint.outpoint.vout as usize) {
                match self
                  .client
                  .get_raw_transaction_info(&satpoint.outpoint.txid)
                {
                  Ok(info) => {
                    if let Some(vout) = info.vout.get(satpoint.outpoint.vout as usize) {
                      vout
                        .script_pub_key
                        .address
                        .clone()
                        .map(|a| a.to_string())
                        .unwrap_or_else(|| "no-address".to_string())
                    } else {
                      "output-not-found-in-rpc".to_string()
                    }
                  }
                  Err(e) => e.to_string(),
                }
              } else {
                "output-not-found".to_string()
              }
            }
            None => "transaction-not-found".to_string(),
          }
        };
        write!(writer, "\t{}", address)?;
      }
      writeln!(writer)?;
    }

    writer.flush()?;
    Ok(())
  }

  pub(crate) fn compact(&mut self) -> Result {
    let wtx = self.database.begin_write()?;
    let savepoints: Vec<u64> = wtx.list_persistent_savepoints()?.collect();
    if !savepoints.is_empty() {
      log::info!(
        "Removing {} persistent savepoints before compaction",
        savepoints.len()
      );
      for id in savepoints {
        wtx.delete_persistent_savepoint(id)?;
      }
      wtx.commit()?;
    } else {
      drop(wtx);
    }

    if self.database.compact()? {
      log::info!("Database compacted successfully");
    } else {
      log::info!("Database is already compact");
    }
    Ok(())
  }

  #[allow(dead_code)]
  pub(crate) fn is_unrecoverably_reorged(&self) -> bool {
    self.unrecoverably_reorged.load(atomic::Ordering::Relaxed)
  }

  pub(crate) fn block_hash(&self, height: u32) -> Result<Option<BlockHash>> {
    let rtx = self.database.begin_read()?;
    let table = rtx.open_table(HEIGHT_TO_BLOCK_HASH)?;
    let hash = table
      .get(&height)?
      .map(|hash| BlockHash::from_inner(*hash.value()));
    Ok(hash)
  }

  fn begin_read(&self) -> Result<rtx::Rtx> {
    Ok(rtx::Rtx(self.database.begin_read()?))
  }

  fn begin_write(&self) -> Result<WriteTransaction> {
    if integration_test() {
      let mut tx = self.database.begin_write()?;
      tx.set_durability(redb::Durability::None)?;
      tx.set_quick_repair(true);
      Ok(tx)
    } else {
      let mut tx = self.database.begin_write()?;
      tx.set_durability(redb::Durability::Immediate)?;
      tx.set_quick_repair(true);
      Ok(tx)
    }
  }

  fn increment_statistic(wtx: &WriteTransaction, statistic: Statistic, n: u64) -> Result {
    let mut statistic_to_count = wtx.open_table(STATISTIC_TO_COUNT)?;
    let value = statistic_to_count
      .get(&(statistic.key()))?
      .map(|x| x.value())
      .unwrap_or(0)
      + n;
    statistic_to_count.insert(&statistic.key(), &value)?;
    Ok(())
  }

  #[cfg(test)]
  pub(crate) fn statistic(&self, statistic: Statistic) -> u64 {
    self
      .database
      .begin_read()
      .unwrap()
      .open_table(STATISTIC_TO_COUNT)
      .unwrap()
      .get(&statistic.key())
      .unwrap()
      .map(|x| x.value())
      .unwrap_or(0)
  }

  pub(crate) fn height(&self) -> Result<Option<Height>> {
    self.begin_read()?.height()
  }

  pub(crate) fn block_count(&self) -> Result<u32> {
    self.begin_read()?.block_count()
  }

  pub(crate) fn inscription_count(&self) -> Result<u64> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?
        .len()?,
    )
  }

  pub(crate) fn blocks(&self, take: usize) -> Result<Vec<(u32, BlockHash)>> {
    let mut blocks = Vec::new();

    let rtx = self.begin_read()?;

    let block_count = rtx.block_count()?;

    let height_to_block_hash = rtx.0.open_table(HEIGHT_TO_BLOCK_HASH)?;

    for next in height_to_block_hash.range(0..block_count)?.rev().take(take) {
      let (height, hash) = next.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
      blocks.push((height.value(), Entry::load(*hash.value())));
    }

    Ok(blocks)
  }

  pub(crate) fn rare_sat_satpoints(&self) -> Result<Option<Vec<(Sat, SatPoint)>>> {
    if self.has_sat_index()? {
      let mut result_vec = Vec::new();

      let rtx = self.database.begin_read()?;

      let sat_to_satpoint = rtx.open_table(SAT_TO_SATPOINT)?;

      for result in sat_to_satpoint.range(0..)? {
        let (sat, satpoint) = result.expect("Error reading from SAT_TO_SATPOINT table");
        result_vec.push((Sat(sat.value()), Entry::load(*satpoint.value())));
      }

      Ok(Some(result_vec))
    } else {
      Ok(None)
    }
  }

  pub(crate) fn rare_sat_satpoint(&self, sat: Sat) -> Result<Option<SatPoint>> {
    if self.has_sat_index()? {
      Ok(
        self
          .database
          .begin_read()?
          .open_table(SAT_TO_SATPOINT)?
          .get(&sat.n())?
          .map(|satpoint| Entry::load(*satpoint.value())),
      )
    } else {
      Ok(None)
    }
  }

  pub(crate) fn block_header(&self, hash: BlockHash) -> Result<Option<BlockHeader>> {
    self.client.get_block_header(&hash).into_option()
  }

  pub(crate) fn block_header_info(&self, hash: BlockHash) -> Result<Option<GetBlockHeaderResult>> {
    self.client.get_block_header_info(&hash).into_option()
  }

  pub(crate) fn get_block_by_height(&self, height: u32) -> Result<Option<Block>> {
    let tx = self.database.begin_read()?;

    let indexed = tx.open_table(HEIGHT_TO_BLOCK_HASH)?.get(&height)?.is_some();

    if !indexed {
      return Ok(None);
    }

    Ok(
      self
        .client
        .get_block_hash(u64::from(height))
        .into_option()?
        .map(|hash| self.client.get_block(&hash))
        .transpose()?,
    )
  }

  pub(crate) fn get_block_by_hash(&self, hash: BlockHash) -> Result<Option<Block>> {
    let tx = self.database.begin_read()?;

    // check if the given hash exists as a value in the database
    let indexed = tx
      .open_table(HEIGHT_TO_BLOCK_HASH)?
      .range(0..)?
      .rev()
      .any(|result| {
        let (_height, block_hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
        block_hash.value() == hash.as_inner()
      });

    if !indexed {
      return Ok(None);
    }

    self.client.get_block(&hash).into_option()
  }

  pub(crate) fn get_inscription_id_by_sat(&self, sat: Sat) -> Result<Option<InscriptionId>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(SAT_TO_INSCRIPTION_ID)?
        .get(&sat.n())?
        .map(|id: redb::AccessGuard<&InscriptionIdValue>| Entry::load(*id.value())),
    )
  }

  pub(crate) fn get_inscription_id_by_inscription_number(
    &self,
    n: u32,
  ) -> Result<Option<InscriptionId>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?
        .get(&n)?
        .map(|id| Entry::load(*id.value())),
    )
  }

  pub(crate) fn get_inscription_satpoint_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<SatPoint>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_SATPOINT)?
        .get(&inscription_id.store())?
        .map(|satpoint| Entry::load(*satpoint.value())),
    )
  }

  pub(crate) fn get_inscription_by_id(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<Inscription>> {
    if self
      .database
      .begin_read()?
      .open_table(INSCRIPTION_ID_TO_SATPOINT)?
      .get(&inscription_id.store())?
      .is_none()
    {
      return Ok(None);
    }

    let reader = self.database.begin_read()?;

    let table = reader.open_table(INSCRIPTION_ID_TO_TXIDS)?;
    let txids_result = table.get(&inscription_id.store())?;

    match txids_result {
      Some(txids) => {
        let mut txs = vec![];

        let txids = txids.value();

        for i in 0..txids.len() / 32 {
          let txid_buf = &txids[i * 32..i * 32 + 32];
          let table = reader.open_table(INSCRIPTION_TXID_TO_TX)?;
          let tx_result = table.get(txid_buf)?;

          match tx_result {
            Some(tx_result) => {
              let tx_buf = tx_result.value().to_vec();
              let mut cursor = Cursor::new(tx_buf);
              let tx = bitcoin::Transaction::consensus_decode(&mut cursor)?;
              txs.push(tx);
            }
            None => return Ok(None),
          }
        }

        let parsed_inscription = Inscription::from_transactions(&txs);

        match parsed_inscription {
          ParsedInscription::None => Ok(None),
          ParsedInscription::Partial => Ok(None),
          ParsedInscription::Complete(inscription) => Ok(Some(inscription)),
        }
      }

      None => Ok(None),
    }
  }

  pub(crate) fn get_inscriptions_on_output(
    &self,
    outpoint: OutPoint,
  ) -> Result<Vec<InscriptionId>> {
    Ok(
      Self::inscriptions_on_output(
        &self
          .database
          .begin_read()?
          .open_table(SATPOINT_TO_INSCRIPTION_ID)?,
        outpoint,
      )?
      .map(|(_satpoint, inscription_id)| inscription_id)
      .collect(),
    )
  }

  pub(crate) fn get_transaction(&self, txid: Txid) -> Result<Option<Transaction>> {
    if txid == self.genesis_block_coinbase_txid {
      Ok(Some(self.genesis_block_coinbase_transaction.clone()))
    } else {
      self.client.get_raw_transaction(&txid).into_option()
    }
  }

  pub(crate) fn get_transaction_blockhash(&self, txid: Txid) -> Result<Option<BlockHash>> {
    Ok(
      self
        .client
        .get_raw_transaction_info(&txid)
        .into_option()?
        .and_then(|info| {
          if info.in_active_chain.unwrap_or_default() {
            info.blockhash
          } else {
            None
          }
        }),
    )
  }

  pub(crate) fn is_transaction_in_active_chain(&self, txid: Txid) -> Result<bool> {
    Ok(
      self
        .client
        .get_raw_transaction_info(&txid)
        .into_option()?
        .and_then(|info| info.in_active_chain)
        .unwrap_or(false),
    )
  }

  pub(crate) fn find(&self, sat: u128) -> Result<Option<SatPoint>> {
    self.require_sat_index("find")?;

    let rtx = self.begin_read()?;

    if rtx.block_count()? <= Sat(sat).height().n() {
      return Ok(None);
    }

    let outpoint_to_sat_ranges = rtx.0.open_table(OUTPOINT_TO_SAT_RANGES)?;

    for result in outpoint_to_sat_ranges.range::<&[u8; 36]>(&[0; 36]..)? {
      let (key, value) = result.expect("Error reading from OUTPOINT_TO_SAT_RANGES table");
      let mut offset = 0;
      for chunk in value.value().chunks_exact(24) {
        let (start, end) = SatRange::load(chunk.try_into().unwrap());
        if start <= sat && sat < end {
          return Ok(Some(SatPoint {
            outpoint: Entry::load(*key.value()),
            offset: offset + u64::try_from(sat - start).unwrap(),
          }));
        }
        offset += u64::try_from(end - start).unwrap();
      }
    }

    Ok(None)
  }

  fn list_inner(&self, outpoint: OutPointValue) -> Result<Option<Vec<u8>>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(OUTPOINT_TO_SAT_RANGES)?
        .get(&outpoint)?
        .map(|outpoint| outpoint.value().to_vec()),
    )
  }

  pub(crate) fn list(&self, outpoint: OutPoint) -> Result<Option<List>> {
    self.require_sat_index("list")?;

    let array = outpoint.store();

    let sat_ranges = self.list_inner(array)?;

    match sat_ranges {
      Some(sat_ranges) => Ok(Some(List::Unspent(
        sat_ranges
          .chunks_exact(24)
          .map(|chunk| SatRange::load(chunk.try_into().unwrap()))
          .collect(),
      ))),
      None => {
        if self.is_transaction_in_active_chain(outpoint.txid)? {
          Ok(Some(List::Spent))
        } else {
          Ok(None)
        }
      }
    }
  }

  pub(crate) fn get_inscriptions_by_address(
    &self,
    address: &str,
  ) -> Result<(Vec<InscriptionId>, Vec<OutPoint>)> {
    let rtx = self.database.begin_read()?;
    let table = rtx.open_multimap_table(ADDRESS_TO_INSCRIPTION_IDS)?;
    let satpoint_table = rtx.open_table(INSCRIPTION_ID_TO_SATPOINT)?;

    let mut ids = Vec::new();
    let mut outputs = Vec::new();
    for result in table.get(address)? {
      let value = result?;
      let inscription_id = InscriptionId::load(*value.value());
      if let Some(satpoint) = satpoint_table.get(&inscription_id.store())? {
        let satpoint: SatPoint = Entry::load(*satpoint.value());
        if !outputs.contains(&satpoint.outpoint) {
          outputs.push(satpoint.outpoint);
        }
      }
      ids.push(inscription_id);
    }
    Ok((ids, outputs))
  }

  #[allow(dead_code)]
  pub(crate) fn get_inscription_address(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<String>> {
    let rtx = self.database.begin_read()?;
    let table = rtx.open_table(INSCRIPTION_ID_TO_ADDRESS)?;
    let address = table
      .get(&inscription_id.store())?
      .map(|v| v.value().to_string());
    Ok(address)
  }

  pub(crate) fn blocktime(&self, height: Height) -> Result<Blocktime> {
    let height = height.n();

    match self.get_block_by_height(height)? {
      Some(block) => Ok(Blocktime::confirmed(block.header.time)),
      None => {
        let tx = self.database.begin_read()?;

        let current = tx
          .open_table(HEIGHT_TO_BLOCK_HASH)?
          .range(0..)?
          .next_back()
          .map(|result| {
            let (height, _hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
            height.value()
          })
          .unwrap_or(0);

        let expected_blocks = height.checked_sub(current).with_context(|| {
          format!("current {current} height is greater than sat height {height}")
        })?;

        Ok(Blocktime::Expected(
          Utc::now()
            .round_subsecs(0)
            .checked_add_signed(chrono::Duration::seconds(
              10 * 60 * i64::from(expected_blocks),
            ))
            .ok_or_else(|| anyhow!("block timestamp out of range"))?,
        ))
      }
    }
  }

  /// Returns (txout, indexed, spent, confirmations, sat_ranges, inscriptions) for an outpoint.
  /// Returns None if the transaction is not found.
  pub(crate) fn get_output_info(&self, outpoint: OutPoint) -> Result<Option<OutputInfo>> {
    let sat_ranges = if self.has_sat_index()? {
      if let Some(List::Unspent(ranges)) = self.list(outpoint)? {
        Some(
          ranges
            .into_iter()
            .map(|(start, end)| (u64::try_from(start).unwrap(), u64::try_from(end).unwrap()))
            .collect::<Vec<(u64, u64)>>(),
        )
      } else {
        None
      }
    } else {
      None
    };

    let indexed = self
      .database
      .begin_read()?
      .open_table(OUTPOINT_TO_VALUE)?
      .get(&outpoint.store())?
      .is_some();

    let confirmations;
    let spent;
    let txout;

    if outpoint == OutPoint::null() {
      let mut value = 0;
      if let Some(ref ranges) = sat_ranges {
        for &(start, end) in ranges {
          value += end - start;
        }
      }
      confirmations = 0;
      spent = false;
      txout = TxOut {
        value,
        script_pubkey: Script::new(),
      };
    } else {
      let Some(info) = self
        .client
        .get_raw_transaction_info(&outpoint.txid)
        .into_option()?
      else {
        return Ok(None);
      };

      let Some(output) = info
        .transaction()?
        .output
        .into_iter()
        .nth(outpoint.vout as usize)
      else {
        return Ok(None);
      };

      confirmations = info.confirmations.unwrap_or(0);
      spent = !indexed;
      txout = output;
    }

    let inscriptions = self.get_inscriptions_on_output(outpoint)?;

    Ok(Some(OutputInfo {
      txout,
      indexed,
      spent,
      confirmations,
      sat_ranges,
      inscriptions,
    }))
  }

  pub(crate) fn get_homepage_inscriptions(&self) -> Result<Vec<InscriptionId>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?
        .iter()?
        .rev()
        .take(20)
        .map(|result| {
          let (_number, id) = result.expect("Error reading from inscription number table");
          Entry::load(*id.value())
        })
        .collect(),
    )
  }

  pub(crate) fn get_latest_inscriptions_with_prev_and_next(
    &self,
    n: usize,
    from: Option<u32>,
  ) -> Result<(Vec<InscriptionId>, Option<u32>, Option<u32>)> {
    let rtx = self.database.begin_read()?;

    let inscription_number_to_inscription_id =
      rtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;

    let latest = match inscription_number_to_inscription_id.iter()?.next_back() {
      Some(result) => {
        let (number, _id) = result.expect("Error reading from inscription number table");
        number.value()
      }
      None => return Ok(Default::default()),
    };

    let from = from.unwrap_or(latest);

    let prev = if let Some(prev) = from.checked_sub(n.try_into()?) {
      inscription_number_to_inscription_id
        .get(&prev)?
        .map(|_| prev)
    } else {
      None
    };

    let next = if from < latest {
      Some(
        from
          .checked_add(n.try_into()?)
          .unwrap_or(latest)
          .min(latest),
      )
    } else {
      None
    };

    let inscriptions = inscription_number_to_inscription_id
      .range(..=from)?
      .rev()
      .take(n)
      .map(|result| {
        let (_number, id) = result.expect("Error reading from inscription number table");
        Entry::load(*id.value())
      })
      .collect();

    Ok((inscriptions, prev, next))
  }

  pub(crate) fn get_feed_inscriptions(&self, n: usize) -> Result<Vec<(u32, InscriptionId)>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?
        .iter()?
        .rev()
        .take(n)
        .map(|result| {
          let (number, id) = result.expect("Error reading from inscription number table");
          (number.value(), Entry::load(*id.value()))
        })
        .collect(),
    )
  }

  pub(crate) fn get_inscriptions_in_block(&self, block_height: u32) -> Result<Vec<InscriptionId>> {
    let rtx = self.database.begin_read()?;

    let height_to_last_inscription_number = rtx.open_table(HEIGHT_TO_LAST_INSCRIPTION_NUMBER)?;
    let inscription_number_to_inscription_id =
      rtx.open_table(INSCRIPTION_NUMBER_TO_INSCRIPTION_ID)?;

    let Some(newest_inscription_number) = height_to_last_inscription_number
      .get(&block_height)?
      .map(|ag| ag.value())
    else {
      return Ok(Vec::new());
    };

    let oldest_inscription_number = height_to_last_inscription_number
      .get(block_height.saturating_sub(1))?
      .map(|ag| ag.value())
      .unwrap_or(0);

    (oldest_inscription_number..newest_inscription_number)
      .map(|num| match inscription_number_to_inscription_id.get(&num) {
        Ok(Some(inscription_id)) => Ok(InscriptionId::load(*inscription_id.value())),
        Ok(None) => Err(anyhow!(
          "could not find inscription for inscription number {num}"
        )),
        Err(err) => Err(anyhow!(err)),
      })
      .collect::<Result<Vec<InscriptionId>>>()
  }

  pub(crate) fn get_highest_paying_inscriptions_in_block(
    &self,
    block_height: u32,
    n: usize,
  ) -> Result<(Vec<InscriptionId>, usize)> {
    let inscription_ids = self.get_inscriptions_in_block(block_height)?;

    let mut inscription_to_fee: Vec<(InscriptionId, u64)> = Vec::new();
    for id in &inscription_ids {
      inscription_to_fee.push((
        *id,
        self
          .get_inscription_entry(*id)?
          .ok_or_else(|| anyhow!("could not get entry for inscription {id}"))?
          .fee,
      ));
    }

    inscription_to_fee.sort_by_key(|(_, fee)| *fee);

    Ok((
      inscription_to_fee
        .iter()
        .map(|(id, _)| *id)
        .rev()
        .take(n)
        .collect(),
      inscription_ids.len(),
    ))
  }

  pub(crate) fn get_inscription_entry(
    &self,
    inscription_id: InscriptionId,
  ) -> Result<Option<InscriptionEntry>> {
    Ok(
      self
        .database
        .begin_read()?
        .open_table(INSCRIPTION_ID_TO_INSCRIPTION_ENTRY)?
        .get(&inscription_id.store())?
        .map(|value| InscriptionEntry::load(value.value())),
    )
  }

  #[cfg(test)]
  fn assert_inscription_location(
    &self,
    inscription_id: InscriptionId,
    satpoint: SatPoint,
    sat: u128,
  ) {
    let rtx = self.database.begin_read().unwrap();

    let satpoint_to_inscription_id = rtx.open_table(SATPOINT_TO_INSCRIPTION_ID).unwrap();

    let inscription_id_to_satpoint = rtx.open_table(INSCRIPTION_ID_TO_SATPOINT).unwrap();

    assert_eq!(
      satpoint_to_inscription_id.len().unwrap(),
      inscription_id_to_satpoint.len().unwrap(),
    );

    assert_eq!(
      SatPoint::load(
        *inscription_id_to_satpoint
          .get(&inscription_id.store())
          .unwrap()
          .unwrap()
          .value()
      ),
      satpoint,
    );

    assert_eq!(
      InscriptionId::load(
        *satpoint_to_inscription_id
          .get(&satpoint.store())
          .unwrap()
          .unwrap()
          .value()
      ),
      inscription_id,
    );

    if self.has_sat_index().unwrap() {
      assert_eq!(
        InscriptionId::load(
          *rtx
            .open_table(SAT_TO_INSCRIPTION_ID)
            .unwrap()
            .get(&sat)
            .unwrap()
            .unwrap()
            .value()
        ),
        inscription_id,
      );

      assert_eq!(
        SatPoint::load(
          *rtx
            .open_table(SAT_TO_SATPOINT)
            .unwrap()
            .get(&sat)
            .unwrap()
            .unwrap()
            .value()
        ),
        satpoint,
      );
    }
  }

  fn inscriptions_on_output<'a: 'tx, 'tx>(
    satpoint_to_id: &'a impl ReadableTable<&'static SatPointValue, &'static InscriptionIdValue>,
    outpoint: OutPoint,
  ) -> Result<impl Iterator<Item = (SatPoint, InscriptionId)> + 'tx> {
    let start = SatPoint {
      outpoint,
      offset: 0,
    }
    .store();

    let end = SatPoint {
      outpoint,
      offset: u64::MAX,
    }
    .store();

    Ok(
      satpoint_to_id
        .range::<&[u8; 44]>(&start..=&end)?
        .map(|result| {
          let (satpoint, id) = result.expect("Error reading from satpoint to inscription id table");
          (Entry::load(*satpoint.value()), Entry::load(*id.value()))
        }),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  struct ContextBuilder {
    args: Vec<OsString>,
    tempdir: Option<TempDir>,
  }

  impl ContextBuilder {
    fn build(self) -> Context {
      self.try_build().unwrap()
    }

    fn try_build(self) -> Result<Context> {
      let rpc_server = test_bitcoincore_rpc::builder()
        .network(Network::Regtest)
        .build();

      let tempdir = self.tempdir.unwrap_or_else(|| TempDir::new().unwrap());
      let cookie_file = tempdir.path().join("cookie");
      fs::write(&cookie_file, "username:password").unwrap();

      let command: Vec<OsString> = vec![
        "ord".into(),
        "--rpc-url".into(),
        rpc_server.url().into(),
        "--data-dir".into(),
        tempdir.path().into(),
        "--cookie-file".into(),
        cookie_file.into(),
        "--regtest".into(),
      ];

      let options = Options::try_parse_from(command.into_iter().chain(self.args)).unwrap();
      let settings = Settings::merge(options.clone(), BTreeMap::new())?;
      let index = Index::open(&settings)?;
      index.update().unwrap();

      Ok(Context {
        rpc_server,
        tempdir,
        index,
      })
    }

    fn arg(mut self, arg: impl Into<OsString>) -> Self {
      self.args.push(arg.into());
      self
    }

    fn args<T: Into<OsString>, I: IntoIterator<Item = T>>(mut self, args: I) -> Self {
      self.args.extend(args.into_iter().map(|arg| arg.into()));
      self
    }

    fn tempdir(mut self, tempdir: TempDir) -> Self {
      self.tempdir = Some(tempdir);
      self
    }
  }

  struct Context {
    rpc_server: test_bitcoincore_rpc::Handle,
    #[allow(unused)]
    tempdir: TempDir,
    index: Index,
  }

  fn update_index_with_retry(index: &Index) {
    let mut attempt = 0;
    while let Err(err) = index.update() {
      attempt += 1;
      if attempt > 3 {
        panic!("Failed to update index after {attempt} attempts: {err}");
      }
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
  }

  impl Context {
    fn builder() -> ContextBuilder {
      ContextBuilder {
        args: Vec::new(),
        tempdir: None,
      }
    }

    fn mine_blocks(&self, n: u64) -> Vec<Block> {
      let blocks = self.rpc_server.mine_blocks(n);
      update_index_with_retry(&self.index);
      blocks
    }

    fn mine_blocks_with_subsidy(&self, n: u64, subsidy: u64) -> Vec<Block> {
      let blocks = self.rpc_server.mine_blocks_with_subsidy(n, subsidy);
      update_index_with_retry(&self.index);
      blocks
    }

    fn configurations() -> Vec<Context> {
      vec![
        Context::builder().build(),
        Context::builder().arg("--index-sats").build(),
      ]
    }
  }

  #[test]
  fn height_limit() {
    {
      let context = Context::builder().args(["--height-limit", "0"]).build();
      context.mine_blocks(1);
      assert_eq!(context.index.height().unwrap(), None);
      assert_eq!(context.index.block_count().unwrap(), 0);
    }

    {
      let context = Context::builder().args(["--height-limit", "1"]).build();
      context.mine_blocks(1);
      assert_eq!(context.index.height().unwrap(), Some(Height(0)));
      assert_eq!(context.index.block_count().unwrap(), 1);
    }

    {
      let context = Context::builder().args(["--height-limit", "2"]).build();
      context.mine_blocks(2);
      assert_eq!(context.index.height().unwrap(), Some(Height(1)));
      assert_eq!(context.index.block_count().unwrap(), 2);
    }
  }

  #[test]
  fn inscriptions_below_first_inscription_height_are_skipped() {
    let inscription = inscription("text/plain;charset=utf-8", "hello");
    let template = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      script_sig: inscription.to_p2sh_unlock(),
      ..Default::default()
    };

    {
      let context = Context::builder().build();
      context.mine_blocks(1);
      let txid = context.rpc_server.broadcast_tx(template.clone());
      let inscription_id = InscriptionId::from(txid);
      context.mine_blocks(1);

      assert_eq!(
        context.index.get_inscription_by_id(inscription_id).unwrap(),
        Some(inscription)
      );

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        Some(SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        })
      );
    }

    {
      let context = Context::builder()
        .arg("--first-inscription-height=3")
        .build();
      context.mine_blocks(1);
      let txid = context.rpc_server.broadcast_tx(template);
      let inscription_id = InscriptionId::from(txid);
      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscription_satpoint_by_id(inscription_id)
          .unwrap(),
        None,
      );
    }
  }

  #[test]
  #[ignore]
  fn list_first_coinbase_transaction() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context
        .index
        .list(
          "1a91e3dace36e2be3bf030a65679fe821aa1d6ef92e7c9902eb318182c355691:0"
            .parse()
            .unwrap()
        )
        .unwrap()
        .unwrap(),
      List::Unspent(vec![(0, 50 * u128::from(COIN_VALUE))])
    )
  }

  #[test]
  #[ignore]
  fn list_second_coinbase_transaction() {
    let context = Context::builder().arg("--index-sats").build();
    let txid = context.mine_blocks(1)[0].txdata[0].txid();
    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(vec![(
        50 * u128::from(COIN_VALUE),
        100 * u128::from(COIN_VALUE)
      )])
    )
  }

  #[test]
  #[ignore]
  fn list_split_ranges_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let split_coinbase_output = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      outputs: 2,
      fee: 0,
      ..Default::default()
    };
    let txid = context.rpc_server.broadcast_tx(split_coinbase_output);

    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(vec![(
        50 * u128::from(COIN_VALUE),
        75 * u128::from(COIN_VALUE)
      )])
    );

    assert_eq!(
      context.index.list(OutPoint::new(txid, 1)).unwrap().unwrap(),
      List::Unspent(vec![(
        75 * u128::from(COIN_VALUE),
        100 * u128::from(COIN_VALUE)
      )])
    );
  }

  #[test]
  #[ignore]
  fn list_merge_ranges_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(2);
    let merge_coinbase_outputs = TransactionTemplate {
      inputs: &[(1, 0, 0), (2, 0, 0)],
      fee: 0,
      ..Default::default()
    };

    let txid = context.rpc_server.broadcast_tx(merge_coinbase_outputs);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(vec![
        (50 * u128::from(COIN_VALUE), 100 * u128::from(COIN_VALUE)),
        (100 * u128::from(COIN_VALUE), 150 * u128::from(COIN_VALUE))
      ]),
    );
  }

  #[test]
  #[ignore]
  fn list_fee_paying_transaction_range() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let fee_paying_tx = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      outputs: 2,
      fee: 10,
      ..Default::default()
    };
    let txid = context.rpc_server.broadcast_tx(fee_paying_tx);
    let coinbase_txid = context.mine_blocks(1)[0].txdata[0].txid();

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(vec![(50 * u128::from(COIN_VALUE), 7499999995)]),
    );

    assert_eq!(
      context.index.list(OutPoint::new(txid, 1)).unwrap().unwrap(),
      List::Unspent(vec![(7499999995, 9999999990)]),
    );

    assert_eq!(
      context
        .index
        .list(OutPoint::new(coinbase_txid, 0))
        .unwrap()
        .unwrap(),
      List::Unspent(vec![(10000000000, 15000000000), (9999999990, 10000000000)])
    );
  }

  #[test]
  #[ignore]
  fn list_two_fee_paying_transaction_range() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(2);
    let first_fee_paying_tx = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      fee: 10,
      ..Default::default()
    };
    let second_fee_paying_tx = TransactionTemplate {
      inputs: &[(2, 0, 0)],
      fee: 10,
      ..Default::default()
    };
    context.rpc_server.broadcast_tx(first_fee_paying_tx);
    context.rpc_server.broadcast_tx(second_fee_paying_tx);

    let coinbase_txid = context.mine_blocks(1)[0].txdata[0].txid();

    assert_eq!(
      context
        .index
        .list(OutPoint::new(coinbase_txid, 0))
        .unwrap()
        .unwrap(),
      List::Unspent(vec![
        (15000000000, 20000000000),
        (9999999990, 10000000000),
        (14999999990, 15000000000)
      ])
    );
  }

  #[test]
  #[ignore]
  fn list_null_output() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let no_value_output = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      fee: 50 * COIN_VALUE,
      ..Default::default()
    };
    let txid = context.rpc_server.broadcast_tx(no_value_output);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(Vec::new())
    );
  }

  #[test]
  #[ignore]
  fn list_null_input() {
    let context = Context::builder().arg("--index-sats").build();

    context.mine_blocks(1);
    let no_value_output = TransactionTemplate {
      inputs: &[(1, 0, 0)],
      fee: 50 * COIN_VALUE,
      ..Default::default()
    };
    context.rpc_server.broadcast_tx(no_value_output);
    context.mine_blocks(1);

    let no_value_input = TransactionTemplate {
      inputs: &[(2, 1, 0)],
      fee: 0,
      ..Default::default()
    };
    let txid = context.rpc_server.broadcast_tx(no_value_input);
    context.mine_blocks(1);

    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Unspent(Vec::new())
    );
  }

  #[test]
  fn list_spent_output() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    context.rpc_server.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0)],
      fee: 0,
      ..Default::default()
    });
    context.mine_blocks(1);
    let txid = context.rpc_server.tx(1, 0).txid();
    assert_eq!(
      context.index.list(OutPoint::new(txid, 0)).unwrap().unwrap(),
      List::Spent,
    );
  }

  #[test]
  fn list_unknown_output() {
    let context = Context::builder().arg("--index-sats").build();

    assert_eq!(
      context
        .index
        .list(
          "0000000000000000000000000000000000000000000000000000000000000000:0"
            .parse()
            .unwrap()
        )
        .unwrap(),
      None
    );
  }

  #[test]
  #[ignore]
  fn find_first_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context.index.find(0).unwrap().unwrap(),
      SatPoint {
        outpoint: "1a91e3dace36e2be3bf030a65679fe821aa1d6ef92e7c9902eb318182c355691:0"
          .parse()
          .unwrap(),
        offset: 0,
      }
    )
  }

  #[test]
  #[ignore]
  fn find_second_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context.index.find(1).unwrap().unwrap(),
      SatPoint {
        outpoint: "1a91e3dace36e2be3bf030a65679fe821aa1d6ef92e7c9902eb318182c355691:0"
          .parse()
          .unwrap(),
        offset: 1,
      }
    )
  }

  #[test]
  #[ignore]
  fn find_first_sat_of_second_block() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    assert_eq!(
      context
        .index
        .find(50 * u128::from(COIN_VALUE))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: "30f2f037629c6a21c1f40ed39b9bd6278df39762d68d07f49582b23bcb23386a:0"
          .parse()
          .unwrap(),
        offset: 0,
      }
    )
  }

  #[test]
  #[ignore]
  fn find_unmined_sat() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(
      context.index.find(50 * u128::from(COIN_VALUE)).unwrap(),
      None
    );
  }

  #[test]
  #[ignore]
  fn find_first_sat_spent_in_second_block() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks(1);
    let spend_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
      inputs: &[(1, 0, 0)],
      fee: 0,
      ..Default::default()
    });
    context.mine_blocks(1);
    assert_eq!(
      context
        .index
        .find(50 * u128::from(COIN_VALUE))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: OutPoint::new(spend_txid, 0),
        offset: 0,
      }
    )
  }

  #[test]
  #[ignore]
  fn inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn unaligned_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      let send_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0), (2, 1, 0)],
        ..Default::default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: send_txid,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn merged_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(2);

      let first_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });

      let first_inscription_id = InscriptionId::from(first_txid);

      let second_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0)],
        witness: inscription("text/png", [1; 100]).to_witness(),
        ..Default::default()
      });
      let second_inscription_id = InscriptionId::from(second_txid);

      context.mine_blocks(1);

      let merged_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 0), (3, 2, 0)],
        ..Default::default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        first_inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: merged_txid,
            vout: 0,
          },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: merged_txid,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        100 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn inscriptions_that_are_sent_to_second_output_are_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      let send_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0), (2, 1, 0)],
        outputs: 2,
        ..Default::default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: send_txid,
            vout: 1,
          },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn missing_inputs_are_fetched_from_pepecoin_core() {
    for args in [
      ["--first-inscription-height", "2"].as_slice(),
      ["--first-inscription-height", "2", "--index-sats"].as_slice(),
    ] {
      let context = Context::builder().args(args).build();
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 0 },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      let send_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0), (2, 1, 0)],
        ..Default::default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: send_txid,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn fee_spent_inscriptions_are_tracked_correctly() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0)],
        fee: 50 * COIN_VALUE,
        ..Default::default()
      });

      let coinbase_tx = context.mine_blocks(1)[0].txdata[0].txid();

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: coinbase_tx,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn inscription_can_be_fee_spent_in_first_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        fee: 50 * COIN_VALUE,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      let coinbase_tx = context.mine_blocks(1)[0].txdata[0].txid();

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: coinbase_tx,
            vout: 0,
          },
          offset: 50 * COIN_VALUE,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn lost_inscriptions() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        fee: 50 * COIN_VALUE,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn multiple_inscriptions_can_be_lost() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let first_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        fee: 50 * COIN_VALUE,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let first_inscription_id = InscriptionId::from(first_txid);

      context.mine_blocks_with_subsidy(1, 0);
      context.mine_blocks(1);

      let second_txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 0, 0)],
        fee: 50 * COIN_VALUE,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let second_inscription_id = InscriptionId::from(second_txid);

      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        first_inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      context.index.assert_inscription_location(
        second_inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 50 * COIN_VALUE,
        },
        150 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn lost_sats_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();
    assert_eq!(context.index.statistic(Statistic::LostSats), 0);

    context.mine_blocks(1);
    assert_eq!(context.index.statistic(Statistic::LostSats), 0);

    context.mine_blocks_with_subsidy(1, 0);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      50 * COIN_VALUE
    );

    context.mine_blocks_with_subsidy(1, 0);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      100 * COIN_VALUE
    );

    context.mine_blocks(1);
    assert_eq!(
      context.index.statistic(Statistic::LostSats),
      100 * COIN_VALUE
    );
  }

  #[test]
  #[ignore]
  fn lost_sat_ranges_are_tracked_correctly() {
    let context = Context::builder().arg("--index-sats").build();

    let null_ranges = || match context.index.list(OutPoint::null()).unwrap().unwrap() {
      List::Unspent(ranges) => ranges,
      _ => panic!(),
    };

    assert!(null_ranges().is_empty());

    context.mine_blocks(1);

    assert!(null_ranges().is_empty());

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      null_ranges(),
      [(100 * u128::from(COIN_VALUE), 150 * u128::from(COIN_VALUE))]
    );

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      null_ranges(),
      [
        (100 * u128::from(COIN_VALUE), 150 * u128::from(COIN_VALUE)),
        (150 * u128::from(COIN_VALUE), 200 * u128::from(COIN_VALUE))
      ]
    );

    context.mine_blocks(1);

    assert_eq!(
      null_ranges(),
      [
        (100 * u128::from(COIN_VALUE), 150 * u128::from(COIN_VALUE)),
        (150 * u128::from(COIN_VALUE), 200 * u128::from(COIN_VALUE))
      ]
    );

    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      null_ranges(),
      [
        (100 * u128::from(COIN_VALUE), 150 * u128::from(COIN_VALUE)),
        (150 * u128::from(COIN_VALUE), 200 * u128::from(COIN_VALUE)),
        (250 * u128::from(COIN_VALUE), 300 * u128::from(COIN_VALUE))
      ]
    );
  }

  #[test]
  #[ignore]
  fn lost_inscriptions_get_lost_satpoints() {
    for context in Context::configurations() {
      context.mine_blocks_with_subsidy(1, 0);
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 0, 0)],
        outputs: 2,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);
      context.mine_blocks(1);

      context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(3, 1, 1), (3, 1, 0)],
        fee: 50 * COIN_VALUE,
        ..Default::default()
      });
      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 75 * COIN_VALUE,
        },
        100 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn inscription_skips_zero_value_first_output_of_inscribe_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        outputs: 2,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        output_values: &[0, 50 * COIN_VALUE],
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);
      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint { txid, vout: 1 },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn inscription_can_be_lost_in_first_transaction() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        fee: 50 * COIN_VALUE,
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);
      context.mine_blocks_with_subsidy(1, 0);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint::null(),
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );
    }
  }

  #[test]
  #[ignore]
  fn lost_rare_sats_are_tracked() {
    let context = Context::builder().arg("--index-sats").build();
    context.mine_blocks_with_subsidy(1, 0);
    context.mine_blocks_with_subsidy(1, 0);

    assert_eq!(
      context
        .index
        .rare_sat_satpoint(Sat(50 * u128::from(COIN_VALUE)))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: OutPoint::null(),
        offset: 0,
      },
    );

    assert_eq!(
      context
        .index
        .rare_sat_satpoint(Sat(100 * u128::from(COIN_VALUE)))
        .unwrap()
        .unwrap(),
      SatPoint {
        outpoint: OutPoint::null(),
        offset: 50 * COIN_VALUE,
      },
    );
  }

  #[test]
  fn old_schema_gives_correct_error() {
    let tempdir = {
      let context = Context::builder().build();

      let wtx = context.index.database.begin_write().unwrap();

      wtx
        .open_table(STATISTIC_TO_COUNT)
        .unwrap()
        .insert(&Statistic::Schema.key(), &0)
        .unwrap();

      wtx.commit().unwrap();

      context.tempdir
    };

    let path = tempdir.path().to_owned();

    let delimiter = if cfg!(windows) { '\\' } else { '/' };

    assert_eq!(
      Context::builder().tempdir(tempdir).try_build().err().unwrap().to_string(),
      format!("index at `{}{delimiter}regtest{delimiter}index.redb` appears to have been built with an older, incompatible version of ord, consider deleting and rebuilding the index: index schema 0, ord schema {SCHEMA_VERSION}", path.display()));
  }

  #[test]
  fn new_schema_gives_correct_error() {
    let tempdir = {
      let context = Context::builder().build();

      let wtx = context.index.database.begin_write().unwrap();

      wtx
        .open_table(STATISTIC_TO_COUNT)
        .unwrap()
        .insert(&Statistic::Schema.key(), &u64::MAX)
        .unwrap();

      wtx.commit().unwrap();

      context.tempdir
    };

    let path = tempdir.path().to_owned();

    let delimiter = if cfg!(windows) { '\\' } else { '/' };

    assert_eq!(
      Context::builder().tempdir(tempdir).try_build().err().unwrap().to_string(),
      format!("index at `{}{delimiter}regtest{delimiter}index.redb` appears to have been built with a newer, incompatible version of ord, consider updating ord: index schema {}, ord schema {SCHEMA_VERSION}", path.display(), u64::MAX));
  }

  #[test]
  fn inscriptions_on_output() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });

      let inscription_id = InscriptionId::from(txid);

      assert_eq!(
        context
          .index
          .get_inscriptions_on_output(OutPoint { txid, vout: 0 })
          .unwrap(),
        []
      );

      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscriptions_on_output(OutPoint { txid, vout: 0 })
          .unwrap(),
        [inscription_id]
      );

      let send_id = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0)],
        ..Default::default()
      });

      context.mine_blocks(1);

      assert_eq!(
        context
          .index
          .get_inscriptions_on_output(OutPoint { txid, vout: 0 })
          .unwrap(),
        []
      );

      assert_eq!(
        context
          .index
          .get_inscriptions_on_output(OutPoint {
            txid: send_id,
            vout: 0,
          })
          .unwrap(),
        [inscription_id]
      );
    }
  }

  #[test]
  #[ignore]
  fn inscriptions_on_same_sat_after_the_first_are_ignored() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let first = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });

      context.mine_blocks(1);

      let inscription_id = InscriptionId::from(first);

      assert_eq!(
        context
          .index
          .get_inscriptions_on_output(OutPoint {
            txid: first,
            vout: 0
          })
          .unwrap(),
        [inscription_id]
      );

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: first,
            vout: 0,
          },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      let second = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(2, 1, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });

      context.mine_blocks(1);

      context.index.assert_inscription_location(
        inscription_id,
        SatPoint {
          outpoint: OutPoint {
            txid: second,
            vout: 0,
          },
          offset: 0,
        },
        50 * u128::from(COIN_VALUE),
      );

      assert!(context
        .index
        .get_inscription_entry(second.into())
        .unwrap()
        .is_none());

      assert!(context
        .index
        .get_inscription_by_id(second.into())
        .unwrap()
        .is_none());
    }
  }

  #[test]
  fn get_latest_inscriptions_with_no_prev_and_next() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
        inputs: &[(1, 0, 0)],
        script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
        ..Default::default()
      });
      let inscription_id = InscriptionId::from(txid);

      context.mine_blocks(1);

      let (inscriptions, prev, next) = context
        .index
        .get_latest_inscriptions_with_prev_and_next(100, None)
        .unwrap();
      assert_eq!(inscriptions, &[inscription_id]);
      assert_eq!(prev, None);
      assert_eq!(next, None);
    }
  }

  #[test]
  fn get_latest_inscriptions_with_prev_and_next() {
    for context in Context::configurations() {
      context.mine_blocks(1);

      let mut ids = Vec::new();

      for i in 0..103 {
        let txid = context.rpc_server.broadcast_tx(TransactionTemplate {
          inputs: &[(i + 1, 0, 0)],
          script_sig: inscription("text/plain", "hello").to_p2sh_unlock(),
          ..Default::default()
        });
        ids.push(InscriptionId::from(txid));
        context.mine_blocks(1);
      }

      ids.reverse();

      let (inscriptions, prev, next) = context
        .index
        .get_latest_inscriptions_with_prev_and_next(100, None)
        .unwrap();
      assert_eq!(inscriptions, &ids[..100]);
      assert_eq!(prev, Some(2));
      assert_eq!(next, None);

      let (inscriptions, prev, next) = context
        .index
        .get_latest_inscriptions_with_prev_and_next(100, Some(101))
        .unwrap();
      assert_eq!(inscriptions, &ids[1..101]);
      assert_eq!(prev, Some(1));
      assert_eq!(next, Some(102));

      let (inscriptions, prev, next) = context
        .index
        .get_latest_inscriptions_with_prev_and_next(100, Some(0))
        .unwrap();
      assert_eq!(inscriptions, &ids[102..103]);
      assert_eq!(prev, None);
      assert_eq!(next, Some(100));
    }
  }

  #[test]
  fn unsynced_index_fails() {}
}
