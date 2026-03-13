use super::*;

#[derive(Deserialize, Default, PartialEq, Debug)]
pub(crate) struct Config {
  #[serde(default)]
  pub(crate) hidden: HashSet<InscriptionId>,
  pub(crate) pepecoin_rpc_username: Option<String>,
  pub(crate) pepecoin_rpc_password: Option<String>,
  pub(crate) pepecoin_data_dir: Option<PathBuf>,
  pub(crate) rpc_url: Option<String>,
  pub(crate) data_dir: Option<PathBuf>,
  pub(crate) index: Option<PathBuf>,
  pub(crate) index_sats: Option<bool>,
  pub(crate) cookie_file: Option<PathBuf>,
  pub(crate) server_url: Option<Url>,
  pub(crate) http_port: Option<u16>,
  pub(crate) address: Option<String>,
}

impl Config {
  pub(crate) fn is_hidden(&self, inscription_id: InscriptionId) -> bool {
    self.hidden.contains(&inscription_id)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn inscriptions_can_be_hidden() {
    let a = "8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi0"
      .parse::<InscriptionId>()
      .unwrap();

    let b = "8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi1"
      .parse::<InscriptionId>()
      .unwrap();

    let config = Config {
      hidden: iter::once(a).collect(),
      ..Default::default()
    };

    assert!(config.is_hidden(a));
    assert!(!config.is_hidden(b));
  }
}
