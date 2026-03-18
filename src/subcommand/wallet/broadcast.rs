use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Broadcast;

impl Broadcast {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    let statuses = super::job::process_reveal_jobs(&settings, wallet_name)?;

    if statuses.is_empty() {
      println!("No pending jobs.");
      return Ok(());
    }

    // Group by batch name (None = single file jobs)
    let mut batches: BTreeMap<Option<String>, Vec<&super::job::JobStatus>> = BTreeMap::new();
    for status in &statuses {
      batches.entry(status.batch_name.clone()).or_default().push(status);
    }

    for (batch_name, jobs) in &batches {
      match batch_name {
        Some(name) => {
          let total = jobs.len();
          let completed = jobs.iter().filter(|s| s.completed).count();
          let broadcasting = jobs.iter().filter(|s| s.reveals_broadcast > 0 && !s.completed).count();
          println!("{name} — {completed}/{total} jobs complete, {broadcasting} broadcasting");
        }
        None => {
          for status in jobs {
            println!(
              "{} — {}/{} reveals confirmed, {}/{} broadcast{}",
              status.file_name,
              status.reveals_confirmed,
              status.reveals_total,
              status.reveals_broadcast,
              status.reveals_total,
              if status.completed { ", complete" } else { "" },
            );
          }
        }
      }
    }

    Ok(())
  }
}
