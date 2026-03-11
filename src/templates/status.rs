use super::*;

#[derive(Boilerplate)]
pub(crate) struct StatusHtml {
  pub(crate) address_index: bool,
  pub(crate) chain: Chain,
  pub(crate) height: Option<u64>,
  pub(crate) inscriptions: u64,
  pub(crate) sat_index: bool,
  pub(crate) unrecoverably_reorged: bool,
}

impl PageContent for StatusHtml {
  fn title(&self) -> String {
    "Status".into()
  }
}
