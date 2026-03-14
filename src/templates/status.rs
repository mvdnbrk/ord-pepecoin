use super::*;

#[derive(Boilerplate)]
pub(crate) struct StatusHtml {
  pub(crate) address_index: bool,
  pub(crate) chain: Chain,
  pub(crate) height: Option<u64>,
  pub(crate) index_size: u64,
  pub(crate) inscriptions: u64,
  pub(crate) sat_index: bool,
  pub(crate) started: DateTime<Utc>,
  pub(crate) unrecoverably_reorged: bool,
  pub(crate) uptime: Duration,
}

impl StatusHtml {
  pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
      format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
      format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
      format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
      format!("{bytes} B")
    }
  }
}

impl PageContent for StatusHtml {
  fn title(&self) -> String {
    "Status".into()
  }
}
