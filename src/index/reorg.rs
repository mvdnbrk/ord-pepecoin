use {super::updater::BlockData, super::*};

#[derive(Debug, PartialEq)]
pub(crate) enum Error {
  Recoverable { height: u32, depth: u32 },
  Unrecoverable,
}

impl Display for Error {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Recoverable { height, depth } => {
        write!(f, "{depth} block deep reorg detected at height {height}")
      }
      Self::Unrecoverable => write!(f, "unrecoverable reorg detected"),
    }
  }
}

impl std::error::Error for Error {}

pub(crate) struct Reorg {}

impl Reorg {
  pub(crate) fn detect_reorg(block: &BlockData, height: u32, index: &Index) -> Result {
    let prev_height = match height.checked_sub(1) {
      Some(h) => h,
      None => return Ok(()),
    };

    let index_prev_blockhash = match index.block_hash(prev_height)? {
      Some(hash) => hash,
      None => return Ok(()),
    };

    if index_prev_blockhash == block.header.prev_blockhash {
      return Ok(());
    }

    let savepoint_interval = u32::try_from(index.settings.savepoint_interval()).unwrap();
    let max_savepoints = u32::try_from(index.settings.max_savepoints()).unwrap();
    let max_recoverable_reorg_depth =
      (max_savepoints - 1) * savepoint_interval + height % savepoint_interval;

    for depth in 1..max_recoverable_reorg_depth {
      let index_block_hash = index.block_hash(height.saturating_sub(depth))?;
      let bitcoind_block_hash = index
        .client
        .get_block_hash(u64::from(height.saturating_sub(depth)))
        .into_option()?;

      if index_block_hash == bitcoind_block_hash {
        return Err(anyhow!(Error::Recoverable { height, depth }));
      }
    }

    Err(anyhow!(Error::Unrecoverable))
  }

  pub(crate) fn handle_reorg(index: &Index, height: u32, depth: u32) -> Result {
    log::info!("rolling back database after reorg of depth {depth} at height {height}");

    let mut wtx = index.begin_write()?;

    let savepoints: Vec<u64> = wtx.list_persistent_savepoints()?.collect();

    if savepoints.is_empty() {
      log::error!("no savepoints available for reorg recovery");
      return Err(anyhow!(Error::Unrecoverable));
    }

    let oldest_savepoint = wtx.get_persistent_savepoint(*savepoints.iter().min().unwrap())?;

    wtx.restore_savepoint(&oldest_savepoint)?;

    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
    wtx.commit()?;

    let read_height = index
      .database
      .begin_read()?
      .open_table(HEIGHT_TO_BLOCK_HASH)?
      .range(0..)?
      .next_back()
      .transpose()?
      .map(|(height, _hash)| height.value())
      .unwrap_or(0);

    log::info!(
      "successfully rolled back database to height {}",
      read_height
    );

    Ok(())
  }

  pub(crate) fn is_savepoint_required(index: &Index, height: u32) -> Result<bool> {
    if integration_test() {
      return Ok(false);
    }

    let savepoint_interval = index.settings.savepoint_interval() as u64;

    let last_savepoint_height = index
      .database
      .begin_read()?
      .open_table(STATISTIC_TO_COUNT)?
      .get(&Statistic::LastSavepointHeight.key())?
      .map(|v| v.value())
      .unwrap_or(0);

    let height = u64::from(height);
    let chain_height = index.client.get_block_count()?;

    let result = (height < savepoint_interval
      || height.saturating_sub(last_savepoint_height) >= savepoint_interval)
      && chain_height.saturating_sub(height)
        <= savepoint_interval * index.settings.max_savepoints() as u64 + 1;

    Ok(result)
  }

  pub(crate) fn update_savepoints(index: &Index, height: u32) -> Result {
    if !Self::is_savepoint_required(index, height)? {
      return Ok(());
    }

    let max_savepoints = index.settings.max_savepoints();

    let wtx = index.begin_write()?;

    let savepoints = wtx.list_persistent_savepoints()?.collect::<Vec<u64>>();

    if savepoints.len() >= max_savepoints {
      log::info!("cleaning up savepoints, keeping max {}", max_savepoints);
      wtx.delete_persistent_savepoint(savepoints.into_iter().min().unwrap())?;
    }

    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
    wtx.commit()?;

    let wtx = index.begin_write()?;

    log::info!("creating savepoint at height {height}");

    wtx.persistent_savepoint()?;

    wtx
      .open_table(STATISTIC_TO_COUNT)?
      .insert(&Statistic::LastSavepointHeight.key(), &u64::from(height))?;

    Index::increment_statistic(&wtx, Statistic::Commits, 1)?;
    wtx.commit()?;

    Ok(())
  }
}
