use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Broadcast;

impl Broadcast {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    let statuses = super::job::process_reveal_jobs(&settings, wallet_name)?;
    print_json(&statuses)?;
    Ok(())
  }
}
