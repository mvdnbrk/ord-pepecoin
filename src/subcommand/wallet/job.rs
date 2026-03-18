use {
  super::*,
  chrono::{DateTime, Utc},
  std::fs,
  std::path::{Path, PathBuf},
};

pub(crate) const MEMPOOL_CHAIN_LIMIT: usize = 23;
pub(crate) const MAX_ACTIVE_JOBS: usize = 100;

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
  pub(crate) batch_name: Option<String>,
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
  pub(crate) reveals: Vec<RevealTx>,
}

impl RevealJob {
  pub(crate) fn jobs_dir(settings: &Settings, wallet_name: &str) -> PathBuf {
    settings.data_dir().join("wallets").join(wallet_name).join("jobs")
  }

  pub(crate) fn broadcast_batch(&mut self, client: &Client) -> bool {
    let mut changed = false;
    for reveal in self.reveals.iter_mut().filter(|r| !r.broadcast).take(self.batch_size) {
      match client.call::<Txid>("sendrawtransaction", &[serde_json::to_value(&reveal.raw_hex).unwrap()]) {
        Ok(_) => {
          reveal.broadcast = true;
          changed = true;
        }
        Err(e) => {
          let error_msg = e.to_string();
          if error_msg.contains("-26") && error_msg.contains("too-long-mempool-chain") {
            break;
          } else if error_msg.contains("-27") || error_msg.contains("Transaction already in mempool") {
            reveal.broadcast = true;
            changed = true;
          } else {
            log::error!("Failed to broadcast reveal {}: {}", reveal.txid, e);
            break;
          }
        }
      }
    }
    changed
  }

  pub(crate) fn check_confirmations(&mut self, client: &Client) -> bool {
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

  pub(crate) fn all_confirmed(&self) -> bool {
    self.reveals.iter().all(|r| r.confirmed)
  }

  pub(crate) fn all_broadcast_confirmed(&self) -> bool {
    self.reveals.iter().filter(|r| r.broadcast).all(|r| r.confirmed)
  }

  pub(crate) fn has_pending(&self) -> bool {
    self.reveals.iter().any(|r| !r.broadcast)
  }

  pub(crate) fn status(&self, batch_name: Option<String>) -> JobStatus {
    JobStatus {
      file_name: self.file_name.clone(),
      content_type: self.content_type.clone(),
      file_size: self.file_size,
      commit_txid: self.commit_txid,
      inscription_id: self.inscription_id,
      destination: self.destination.clone(),
      total_fees: self.total_fees,
      reveals_confirmed: self.reveals.iter().filter(|r| r.confirmed).count(),
      reveals_broadcast: self.reveals.iter().filter(|r| r.broadcast).count(),
      reveals_total: self.reveals.len(),
      completed: self.all_confirmed(),
      batch_name,
    }
  }

  pub(crate) fn save(&self, path: &Path) -> Result {
    let tmp_path = path.with_extension("json.tmp");
    serde_json::to_writer_pretty(fs::File::create(&tmp_path)?, self)?;
    fs::rename(tmp_path, path)?;
    Ok(())
  }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RevealTx {
  pub(crate) index: usize,
  pub(crate) txid: Txid,
  pub(crate) raw_hex: String,
  pub(crate) broadcast: bool,
  pub(crate) confirmed: bool,
}

pub(crate) fn sanitize_batch_name(name: &str) -> String {
  name.chars()
    .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
    .collect()
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

pub(crate) fn process_reveal_jobs(settings: &Settings, wallet_name: &str) -> Result<Vec<JobStatus>> {
  let jobs_dir = settings.data_dir().join("wallets").join(wallet_name).join("jobs");
  if !jobs_dir.exists() {
    return Ok(Vec::new());
  }

  let client = settings.pepecoin_rpc_client_for_wallet_command()?;
  let mut statuses = Vec::new();

  // 1. Process flat files (single inscriptions)
  statuses.extend(process_job_files(&client, &jobs_dir, &jobs_dir.join("complete"), None)?);

  // 2. Process batch directories
  for entry in fs::read_dir(&jobs_dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.is_dir() && entry.file_name() != "complete" {
      let batch_name = entry.file_name().to_string_lossy().to_string();
      statuses.extend(process_job_files(&client, &path, &path.join("complete"), Some(batch_name))?);
      
      // Check if batch is complete
      if is_batch_complete(&path) {
        let complete_dir = jobs_dir.join("complete");
        fs::create_dir_all(&complete_dir)?;
        fs::rename(&path, complete_dir.join(entry.file_name()))?;
        log::info!("Batch {} completed and moved to complete/", entry.file_name().to_string_lossy());
      }
    }
  }

  Ok(statuses)
}

fn process_job_files(client: &Client, dir: &Path, complete_dir: &Path, batch_name: Option<String>) -> Result<Vec<JobStatus>> {
  let mut active_count = 0;
  let mut statuses = Vec::new();
  let is_batch = batch_name.is_some();

  for entry in fs::read_dir(dir)? {
    let entry = entry?;
    let path = entry.path();
    
    if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
      if is_batch && active_count >= MAX_ACTIVE_JOBS {
        break;
      }
      active_count += 1;

      let mut job: RevealJob = serde_json::from_reader(fs::File::open(&path)?)?;
      let mut changed = job.check_confirmations(client);

      if job.all_broadcast_confirmed() && job.has_pending() {
        if job.broadcast_batch(client) {
          changed = true;
        }
      }

      if changed {
        job.save(&path)?;
      }

      // Check if job completed
      if job.all_confirmed() {
        fs::create_dir_all(complete_dir)?;
        fs::rename(&path, complete_dir.join(path.file_name().unwrap()))?;
        log::info!("Job {} completed", job.commit_txid);
      }

      statuses.push(job.status(batch_name.clone()));
    }
  }
  Ok(statuses)
}

fn is_batch_complete(batch_dir: &Path) -> bool {
  if let Ok(entries) = fs::read_dir(batch_dir) {
    for entry in entries {
      if let Ok(entry) = entry {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
          return false;
        }
      }
    }
  }
  true
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
    assert_eq!(deserialized.file_name, "test.png");
  }
}
