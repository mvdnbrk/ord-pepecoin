use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Broadcast;

impl Broadcast {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    super::job::process_reveal_jobs(&settings, wallet_name)
  }
}
