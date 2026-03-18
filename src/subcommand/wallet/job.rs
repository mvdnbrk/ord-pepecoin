use {
  super::*,
  chrono::{DateTime, Utc},
  std::fs,
};

pub(crate) const MEMPOOL_CHAIN_LIMIT: usize = 23;

#[derive(Debug, Serialize)]
pub(crate) struct JobStatus {
  pub(crate) file_name: String,
  pub(crate) content_type: String,
  pub(crate) file_size: u64,
  pub(crate) commit_txid: Txid,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) destination: Address,
  pub(crate) total_fees: u64,
  pub(crate) reveals_confirmed: usize,
  pub(crate) reveals_broadcast: usize,
  pub(crate) reveals_total: usize,
  pub(crate) completed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RevealJob {
  pub(crate) file_name: String,
  pub(crate) content_type: String,
  pub(crate) file_size: u64,
  pub(crate) commit_txid: Txid,
  pub(crate) inscription_id: InscriptionId,
  pub(crate) destination: Address,
  pub(crate) total_fees: u64,
  pub(crate) batch_size: usize,
  pub(crate) created_at: DateTime<Utc>,
  pub(crate) reveals: Vec<RevealEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RevealEntry {
  pub(crate) index: usize,
  pub(crate) txid: Txid,
  pub(crate) raw_hex: String,
  pub(crate) broadcast: bool,
  pub(crate) confirmed: bool,
}

impl RevealJob {
  pub(crate) fn jobs_dir(settings: &Settings, wallet_name: &str) -> PathBuf {
    settings.data_dir().join("wallets").join(wallet_name).join("jobs")
  }

  pub(crate) fn save(&self, path: &Path) -> Result {
    let tmp_path = path.with_extension("json.tmp");
    serde_json::to_writer_pretty(fs::File::create(&tmp_path)?, self)?;
    fs::rename(tmp_path, path)?;
    Ok(())
  }

  pub(crate) fn broadcast_batch(&mut self, client: &Client) {
    for reveal in self.reveals.iter_mut().filter(|r| !r.broadcast).take(self.batch_size) {
      match client.call::<Txid>("sendrawtransaction", &[serde_json::to_value(&reveal.raw_hex).unwrap()]) {
        Ok(_) => {
          reveal.broadcast = true;
        }
        Err(e) => {
          let error_msg = e.to_string();
          if error_msg.contains("-27") || error_msg.contains("Transaction already in mempool") {
            reveal.broadcast = true;
          } else {
            if error_msg.contains("-26") && error_msg.contains("too-long-mempool-chain") {
              log::warn!("Mempool chain limit reached for job {}, stopping batch", self.commit_txid);
            } else {
              log::error!("Failed to broadcast reveal {} for job {}: {}", reveal.txid, self.commit_txid, e);
            }
            break;
          }
        }
      }
    }
  }

  fn check_confirmations(&mut self, client: &Client) -> bool {
    let mut changed = false;
    for reveal in self.reveals.iter_mut().filter(|r| r.broadcast && !r.confirmed) {
      match client.call::<serde_json::Value>(
        "getrawtransaction",
        &[serde_json::to_value(reveal.txid).unwrap(), serde_json::to_value(true).unwrap()],
      ) {
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
    changed
  }

  pub(crate) fn all_broadcast_confirmed(&self) -> bool {
    self.reveals.iter().filter(|r| r.broadcast).all(|r| r.confirmed)
  }

  pub(crate) fn all_confirmed(&self) -> bool {
    self.reveals.iter().all(|r| r.confirmed)
  }

  pub(crate) fn confirmed_count(&self) -> usize {
    self.reveals.iter().filter(|r| r.confirmed).count()
  }

  pub(crate) fn has_pending(&self) -> bool {
    self.reveals.iter().any(|r| !r.broadcast)
  }

  pub(crate) fn status(&self) -> JobStatus {
    JobStatus {
      file_name: self.file_name.clone(),
      content_type: self.content_type.clone(),
      file_size: self.file_size,
      commit_txid: self.commit_txid,
      inscription_id: self.inscription_id,
      destination: self.destination.clone(),
      total_fees: self.total_fees,
      reveals_confirmed: self.confirmed_count(),
      reveals_broadcast: self.reveals.iter().filter(|r| r.broadcast).count(),
      reveals_total: self.reveals.len(),
      completed: self.all_confirmed(),
    }
  }
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
        let _ = process_reveal_jobs(settings, &wallet_name)?;
      }
    }
  }
  Ok(())
}

pub(crate) fn process_reveal_jobs(settings: &Settings, wallet_name: &str) -> Result<Vec<JobStatus>> {
  let jobs_dir = RevealJob::jobs_dir(settings, wallet_name);
  if !jobs_dir.exists() {
    return Ok(Vec::new());
  }

  let client = settings.pepecoin_rpc_client_for_wallet_command()?;
  let mut statuses = Vec::new();

  for entry in fs::read_dir(&jobs_dir)? {
    let entry = entry?;
    let path = entry.path();
    if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
      continue;
    }

    let mut job: RevealJob = serde_json::from_reader(fs::File::open(&path)?)?;
    let mut changed = job.check_confirmations(&client);

    if job.all_broadcast_confirmed() && job.has_pending() {
      log::info!("Job {}: broadcasting batch of {} reveals", job.commit_txid,
        job.reveals.iter().filter(|r| !r.broadcast).take(job.batch_size).count());
      job.broadcast_batch(&client);
      changed = true;
    }

    if changed {
      job.save(&path)?;
    }

    if job.all_confirmed() {
      let complete_dir = jobs_dir.join("complete");
      fs::create_dir_all(&complete_dir)?;
      fs::rename(&path, complete_dir.join(path.file_name().unwrap()))?;
      log::info!("Job {} completed", job.commit_txid);
    }

    statuses.push(job.status());
  }

  Ok(statuses)
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
      file_name: "test.png".to_string(),
      content_type: "image/png".to_string(),
      file_size: 520,
      commit_txid,
      inscription_id,
      destination,
      total_fees: 1000,
      batch_size: 23,
      created_at: Utc::now(),
      reveals: vec![RevealEntry {
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
