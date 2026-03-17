use super::*;

#[derive(Boilerplate)]
pub(crate) struct InscriptionsHtml {
  pub(crate) inscriptions: Vec<InscriptionId>,
  pub(crate) page_index: u32,
  pub(crate) more: bool,
}

impl PageContent for InscriptionsHtml {
  fn title(&self) -> String {
    "Inscription".into()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn without_prev_and_next() {
    assert_regex_match!(
      InscriptionsHtml {
        inscriptions: vec![inscription_id(1), inscription_id(2)],
        page_index: 0,
        more: false,
      },
      "
        <h1>Inscription</h1>
        <div class=thumbnails>
          <a href=/inscription/1{64}i1><iframe .* src=/preview/1{64}i1></iframe></a>
          <a href=/inscription/2{64}i2><iframe .* src=/preview/2{64}i2></iframe></a>
        </div>
        .*
        prev
        next
        .*
      "
      .unindent()
    );
  }

  #[test]
  fn with_prev_and_next() {
    assert_regex_match!(
      InscriptionsHtml {
        inscriptions: vec![inscription_id(1), inscription_id(2)],
        page_index: 1,
        more: true,
      },
      "
        <h1>Inscription</h1>
        <div class=thumbnails>
          <a href=/inscription/1{64}i1><iframe .* src=/preview/1{64}i1></iframe></a>
          <a href=/inscription/2{64}i2><iframe .* src=/preview/2{64}i2></iframe></a>
        </div>
        .*
        <a class=prev href=/inscriptions/0>prev</a>
        <a class=next href=/inscriptions/2>next</a>
        .*
      "
      .unindent()
    );
  }
}
