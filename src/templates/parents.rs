use super::*;

#[derive(Boilerplate)]
pub(crate) struct ParentsHtml {
  pub(crate) inscription_id: InscriptionId,
  pub(crate) inscription_number: u32,
  pub(crate) parents: Vec<InscriptionId>,
  pub(crate) prev_page: Option<usize>,
  pub(crate) next_page: Option<usize>,
}

impl PageContent for ParentsHtml {
  fn title(&self) -> String {
    format!("Inscription {} Parents", self.inscription_number)
  }
}
