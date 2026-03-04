use super::*;

#[derive(Boilerplate)]
pub(crate) struct AddressHtml {
  pub(crate) address: String,
  pub(crate) inscriptions: Vec<InscriptionId>,
}

impl PageContent for AddressHtml {
  fn title(&self) -> String {
    format!("Address {}", self.address)
  }
}
