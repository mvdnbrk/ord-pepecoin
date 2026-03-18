use {
  super::*,
  chrono::{DateTime, Utc},
  std::fs,
};

#[derive(Debug, Parser)]
pub(crate) struct Broadcast;

impl Broadcast {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    process_reveal_jobs(&settings, wallet_name)
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RevealJob {
  pub(crate) commit_txid: Txid,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) destination: Address,
  pub(crate) total_fees: u64,
  pub(crate) batch_size: usize,
  pub(crate) created_at: DateTime<Utc>,
  pub(crate) reveals: Vec<RevealTx>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RevealTx {
  pub(crate) index: usize,
  pub(crate) txid: Txid,
  pub(crate) raw_hex: String,
  pub(crate) broadcast: bool,
  pub(crate) confirmed: bool,
}

pub(crate) fn process_pending_jobs(settings: &Settings) -> Result {
  let wallets_dir = settings.data_dir().join("wallets");
  if !wallets_dir.exists() {
    return Ok(());
  }
  for entry in fs::read_dir(&wallets_dir)? {
    let entry = entry?;
    if entry.file_type()?.is_dir() {
      let wallet_name = entry.file_name().to_string_lossy().to_string();
      let jobs_dir = entry.path().join("jobs");
      if jobs_dir.exists() {
        process_reveal_jobs(settings, &wallet_name)?;
      }
    }
  }
  Ok(())
}

pub(crate) fn process_reveal_jobs(settings: &Settings, wallet_name: &str) -> Result {
  let jobs_dir = settings.data_dir().join("wallets").join(wallet_name).join("jobs");
  if !jobs_dir.exists() {
    return Ok(());
  }

  let client = settings.pepecoin_rpc_client_for_wallet_command()?;

  for entry in fs::read_dir(&jobs_dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
      let mut job: RevealJob = serde_json::from_reader(fs::File::open(&path)?)?;
      let mut changed = false;

      // 1. Check confirmations for already broadcasted txs
      for reveal in job.reveals.iter_mut().filter(|r| r.broadcast && !r.confirmed) {
        match client.call::<serde_json::Value>("getrawtransaction", &[serde_json::to_value(reveal.txid)?, serde_json::to_value(true)?]) {
          Ok(tx_info) => {
            if let Some(confirmations) = tx_info["confirmations"].as_u64() {
              if confirmations >= 1 {
                reveal.confirmed = true;
                changed = true;
              }
            }
          }
          Err(e) => {
            log::warn!("Failed to check confirmation for reveal {}: {}", reveal.txid, e);
          }
        }
      }

      // 2. Check if we can broadcast the next batch
      let all_broadcast_confirmed = job.reveals.iter().filter(|r| r.broadcast).all(|r| r.confirmed);
      if all_broadcast_confirmed {
        let next_batch: Vec<&mut RevealTx> = job.reveals.iter_mut()
          .filter(|r| !r.broadcast)
          .take(job.batch_size)
          .collect();

        if !next_batch.is_empty() {
          log::info!("Job {}: broadcasting batch of {} reveals", job.commit_txid, next_batch.len());
          for reveal in next_batch {
            match client.call::<Txid>("sendrawtransaction", &[serde_json::to_value(&reveal.raw_hex)?]) {
              Ok(_) => {
                reveal.broadcast = true;
                changed = true;
              }
              Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("-26") && error_msg.contains("too-long-mempool-chain") {
                  log::warn!("Mempool chain limit reached for job {}, stopping current batch", job.commit_txid);
                  break;
                } else if error_msg.contains("-27") || error_msg.contains("Transaction already in mempool") {
                  reveal.broadcast = true;
                  changed = true;
                } else {
                  log::error!("Failed to broadcast reveal {} for job {}: {}", reveal.txid, job.commit_txid, e);
                  break;
                }
              }
            }
          }
        }
      }

      if changed {
        let tmp_path = path.with_extension("json.tmp");
        serde_json::to_writer_pretty(fs::File::create(&tmp_path)?, &job)?;
        fs::rename(tmp_path, &path)?;
      }

      // 3. Check if completed
      if job.reveals.iter().all(|r| r.confirmed) {
        let complete_dir = jobs_dir.join("complete");
        fs::create_dir_all(&complete_dir)?;
        fs::rename(&path, complete_dir.join(path.file_name().unwrap()))?;
        log::info!("Job {} completed", job.commit_txid);
      } else {
        let confirmed = job.reveals.iter().filter(|r| r.confirmed).count();
        let total = job.reveals.len();
        log::info!("Job {}: {}/{} reveals confirmed", job.commit_txid, confirmed, total);
      }
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::str::FromStr;

  #[test]
  fn serialization() {
    let commit_txid = Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
    let inscription_id = InscriptionId { txid: commit_txid, index: 0 };
    let destination = Address::from_str("PXvn95h8m6x4oGorNVerA2F4FFRpqMqwAM").unwrap();

    let job = RevealJob {
      commit_txid,
      inscription_id,
      destination,
      total_fees: 1000,
      batch_size: 23,
      created_at: Utc::now(),
      reveals: vec![RevealTx {
        index: 0,
        txid: commit_txid,
        raw_hex: "0100000001".to_string(),
        broadcast: true,
        confirmed: false,
      }],
    };

    let serialized = serde_json::to_string(&job).unwrap();
    let deserialized: RevealJob = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.commit_txid, job.commit_txid);
    assert_eq!(deserialized.inscription_id, job.inscription_id);
    assert_eq!(deserialized.reveals.len(), 1);
    assert_eq!(deserialized.reveals[0].broadcast, true);
  }
}
