use super::*;

#[derive(Boilerplate)]
pub(crate) struct ChildrenHtml {
  pub(crate) parent: InscriptionId,
  pub(crate) parent_number: u32,
  pub(crate) children: Vec<InscriptionId>,
  pub(crate) prev_page: Option<usize>,
  pub(crate) next_page: Option<usize>,
}

impl PageContent for ChildrenHtml {
  fn title(&self) -> String {
    format!("Inscription {} Children", self.parent_number)
  }
}
