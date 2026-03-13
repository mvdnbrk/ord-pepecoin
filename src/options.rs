use {super::*, bitcoincore_rpc::Auth};

#[derive(Clone, Debug, Parser)]
#[clap(group(
  ArgGroup::new("chains")
    .required(false)
    .args(&["chain-argument", "signet", "regtest", "testnet"]),
))]
pub(crate) struct Options {
  #[clap(long, help = "Load Pepecoin Core data dir from <PEPECOIN_DATA_DIR>.")]
  pub(crate) pepecoin_data_dir: Option<PathBuf>,
  #[clap(
    long = "chain",
    arg_enum,
    default_value = "mainnet",
    help = "Use <CHAIN>."
  )]
  pub(crate) chain_argument: Chain,
  #[clap(long, help = "Load configuration from <CONFIG>.")]
  pub(crate) config: Option<PathBuf>,
  #[clap(long, help = "Load configuration from <CONFIG_DIR>.")]
  pub(crate) config_dir: Option<PathBuf>,
  #[clap(long, help = "Load Pepecoin Core RPC cookie file from <COOKIE_FILE>.")]
  pub(crate) cookie_file: Option<PathBuf>,
  #[clap(long, help = "Store index in <DATA_DIR>.")]
  pub(crate) data_dir: Option<PathBuf>,
  #[clap(
    long,
    help = "Don't look for inscriptions below <FIRST_INSCRIPTION_HEIGHT>."
  )]
  pub(crate) first_inscription_height: Option<u64>,
  #[clap(long, help = "Limit index to <HEIGHT_LIMIT> blocks.")]
  pub(crate) height_limit: Option<u64>,
  #[clap(long, help = "Use index at <INDEX>.")]
  pub(crate) index: Option<PathBuf>,
  #[clap(long, help = "Track location of all satoshis.")]
  pub(crate) index_sats: bool,
  #[clap(long, short, help = "Use regtest. Equivalent to `--chain regtest`.")]
  pub(crate) regtest: bool,
  #[clap(long, help = "Connect to Pepecoin Core RPC at <RPC_URL>.")]
  pub(crate) rpc_url: Option<String>,
  #[clap(long, short, help = "Use signet. Equivalent to `--chain signet`.")]
  pub(crate) signet: bool,
  #[clap(long, short, help = "Use testnet. Equivalent to `--chain testnet`.")]
  pub(crate) testnet: bool,
  #[clap(long, default_value = "ord", help = "Use wallet named <WALLET>.")]
  pub(crate) wallet: String,
  #[clap(long, help = "Use ord-pepecoin server running at <SERVER_URL>.")]
  pub(crate) server_url: Option<Url>,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      pepecoin_data_dir: None,
      chain_argument: Chain::Mainnet,
      config: None,
      config_dir: None,
      cookie_file: None,
      data_dir: None,
      first_inscription_height: None,
      height_limit: None,
      index: None,
      index_sats: false,
      regtest: false,
      rpc_url: None,
      signet: false,
      testnet: false,
      wallet: "ord".to_string(),
      server_url: None,
    }
  }
}

impl Options {
  pub(crate) fn chain(&self) -> Chain {
    if self.signet {
      Chain::Signet
    } else if self.regtest {
      Chain::Regtest
    } else if self.testnet {
      Chain::Testnet
    } else {
      self.chain_argument
    }
  }

  pub(crate) fn first_inscription_height(&self) -> u64 {
    if self.chain() == Chain::Regtest {
      self.first_inscription_height.unwrap_or(0)
    } else if integration_test() {
      0
    } else {
      self
        .first_inscription_height
        .unwrap_or_else(|| self.chain().first_inscription_height())
    }
  }

  pub(crate) fn load_config(&self) -> Result<Config> {
    match &self.config {
      Some(path) => Ok(serde_yaml::from_reader(File::open(path)?)?),
      None => {
        // Check --config-dir, then --data-dir CLI flag, then default data dir
        let candidates = [
          self.config_dir.as_ref().map(|d| d.join("ord.yaml")),
          self.data_dir.as_ref().map(|d| d.join("ord.yaml")),
          dirs::data_dir().map(|d| d.join("ord-pepecoin").join("ord.yaml")),
        ];

        for candidate in candidates.iter().flatten() {
          if candidate.exists() {
            return Ok(serde_yaml::from_reader(File::open(candidate)?)?);
          }
        }

        Ok(Default::default())
      }
    }
  }

  pub(crate) fn rpc_url(&self) -> String {
    let config = self.load_config().unwrap_or_default();

    self
      .rpc_url
      .clone()
      .or(config.rpc_url)
      .unwrap_or_else(|| {
        format!(
          "127.0.0.1:{}/wallet/{}",
          self.chain().default_rpc_port(),
          self.wallet
        )
      })
  }

  pub(crate) fn server_url(&self) -> Result<Url> {
    let config = self.load_config().unwrap_or_default();

    self
      .server_url
      .clone()
      .or(config.server_url)
      .ok_or_else(|| anyhow!("server URL not specified. Set --server-url or server_url in ord.yaml"))
  }

  pub(crate) fn auth(&self) -> Result<Auth> {
    let config = self.load_config().unwrap_or_default();

    if let (Some(user), Some(pass)) = (config.pepecoin_rpc_username, config.pepecoin_rpc_password) {
      return Ok(Auth::UserPass(user, pass));
    }

    let cookie_file = self.cookie_file()?;
    Ok(Auth::CookieFile(cookie_file))
  }

  pub(crate) fn cookie_file(&self) -> Result<PathBuf> {
    let config = self.load_config().unwrap_or_default();

    if let Some(cookie_file) = self.cookie_file.clone().or(config.cookie_file) {
      return Ok(cookie_file);
    }

    let path = if let Some(pepecoin_data_dir) = self
      .pepecoin_data_dir
      .clone()
      .or(config.pepecoin_data_dir)
    {
      pepecoin_data_dir
    } else if cfg!(target_os = "linux") {
      dirs::home_dir()
        .ok_or_else(|| anyhow!("failed to retrieve home dir"))?
        .join(".pepecoin")
    } else {
      dirs::data_dir()
        .ok_or_else(|| anyhow!("failed to retrieve data dir"))?
        .join("Pepecoin")
    };

    let path = self.chain().join_with_data_dir(&path);

    Ok(path.join(".cookie"))
  }

  pub(crate) fn data_dir(&self) -> Result<PathBuf> {
    // Note: uses load_config() which does NOT call data_dir() to avoid circular dependency.
    // load_config() only checks config_dir, data_dir CLI flag, and default data dir directly.
    let config = self.load_config().unwrap_or_default();

    let base = match self.data_dir.clone().or(config.data_dir) {
      Some(base) => base,
      None => dirs::data_dir()
        .ok_or_else(|| anyhow!("failed to retrieve data dir"))?
        .join("ord-pepecoin"),
    };

    Ok(self.chain().join_with_data_dir(&base))
  }

  fn format_pepecoin_core_version(version: usize) -> String {
    format!(
      "{}.{}.{}.{}",
      version / 1000000,
      version % 1000000 / 10000,
      version % 10000 / 100,
      version % 100
    )
  }

  pub(crate) fn pepecoin_rpc_client(&self) -> Result<Client> {
    let rpc_url = self.rpc_url();
    let auth = self.auth()?;

    match &auth {
      Auth::CookieFile(path) => log::info!(
        "Connecting to Pepecoin Core RPC server at {rpc_url} using cookie file `{}`",
        path.display()
      ),
      Auth::UserPass(user, _) => log::info!(
        "Connecting to Pepecoin Core RPC server at {rpc_url} as user `{user}`"
      ),
      Auth::None => log::info!(
        "Connecting to Pepecoin Core RPC server at {rpc_url} without authentication"
      ),
    }

    let client = Client::new(&rpc_url, auth)
      .with_context(|| format!("failed to connect to Pepecoin Core RPC at {rpc_url}"))?;

    let blockchain_info: serde_json::Value = client
      .call("getblockchaininfo", &[])
      .context("failed to get blockchain info")?;

    let chain_str = blockchain_info["chain"]
      .as_str()
      .ok_or_else(|| anyhow!("missing chain field in getblockchaininfo"))?;

    let rpc_chain = match chain_str {
      "main" => Chain::Mainnet,
      "test" => Chain::Testnet,
      "regtest" => Chain::Regtest,
      "signet" => Chain::Signet,
      other => bail!("Pepecoin RPC server on unknown chain: {other}"),
    };

    let ord_chain = self.chain();

    if rpc_chain != ord_chain {
      bail!("Pepecoin RPC server is on {rpc_chain} but ord-pepecoin is on {ord_chain}");
    }
    Ok(client)
  }

  pub(crate) fn pepecoin_rpc_client_for_wallet_command(&self, _create: bool) -> Result<Client> {
    let client = self.pepecoin_rpc_client()?;

    const MIN_VERSION: usize = 1010000;

    let network_info: serde_json::Value = client
      .call("getnetworkinfo", &[])
      .context("failed to get network info")?;
    let pepecoin_version = network_info["version"]
      .as_u64()
      .ok_or_else(|| anyhow!("missing version in getnetworkinfo"))? as usize;
    if pepecoin_version < MIN_VERSION {
      bail!(
        "Pepecoin Core {} or newer required, current version is {}",
        Self::format_pepecoin_core_version(MIN_VERSION),
        Self::format_pepecoin_core_version(pepecoin_version),
      );
    }

    // Pepecoin Core uses a single default wallet (no multi-wallet support)
    Ok(client)
  }
}

#[cfg(test)]
mod tests {
  use {super::*, bitcoin::Network, std::path::Path};

  #[test]
  fn rpc_url_overrides_network() {
    assert_eq!(
      Arguments::try_parse_from(["ord", "--rpc-url=127.0.0.1:1234", "--chain=signet", "index", "update"])
        .unwrap()
        .options
        .rpc_url(),
      "127.0.0.1:1234"
    );
  }

  #[test]
  fn cookie_file_overrides_network() {
    assert_eq!(
      Arguments::try_parse_from(["ord", "--cookie-file=/foo/bar", "--chain=signet", "index", "update"])
        .unwrap()
        .options
        .cookie_file()
        .unwrap(),
      Path::new("/foo/bar")
    );
  }

  #[test]
  fn use_default_network() {
    let arguments = Arguments::try_parse_from(["ord", "index", "update"]).unwrap();

    assert_eq!(arguments.options.rpc_url(), "127.0.0.1:33873/wallet/ord");

    assert!(arguments
      .options
      .cookie_file()
      .unwrap()
      .ends_with(".cookie"));
  }

  #[test]
  fn uses_network_defaults() {
    let arguments = Arguments::try_parse_from(["ord", "--chain=signet", "index", "update"]).unwrap();

    assert_eq!(arguments.options.rpc_url(), "127.0.0.1:38332/wallet/ord");

    assert!(arguments
      .options
      .cookie_file()
      .unwrap()
      .display()
      .to_string()
      .ends_with(if cfg!(windows) {
        r"\signet\.cookie"
      } else {
        "/signet/.cookie"
      }));
  }

  #[test]
  fn mainnet_cookie_file_path() {
    let cookie_file = Arguments::try_parse_from(["ord", "index", "update"])
      .unwrap()
      .options
      .cookie_file()
      .unwrap()
      .display()
      .to_string();

    assert!(cookie_file.ends_with(if cfg!(target_os = "linux") {
      "/.pepecoin/.cookie"
    } else if cfg!(windows) {
      r"\Pepecoin\.cookie"
    } else {
      "/Pepecoin/.cookie"
    }))
  }

  #[test]
  fn othernet_cookie_file_path() {
    let arguments = Arguments::try_parse_from(["ord", "--chain=signet", "index", "update"]).unwrap();

    let cookie_file = arguments
      .options
      .cookie_file()
      .unwrap()
      .display()
      .to_string();

    assert!(cookie_file.ends_with(if cfg!(target_os = "linux") {
      "/.pepecoin/signet/.cookie"
    } else if cfg!(windows) {
      r"\Pepecoin\signet\.cookie"
    } else {
      "/Pepecoin/signet/.cookie"
    }));
  }

  #[test]
  fn cookie_file_defaults_to_pepecoin_data_dir() {
    let arguments =
      Arguments::try_parse_from(["ord", "--pepecoin-data-dir=foo", "--chain=signet", "index", "update"])
        .unwrap();

    let cookie_file = arguments
      .options
      .cookie_file()
      .unwrap()
      .display()
      .to_string();

    assert!(cookie_file.ends_with(if cfg!(windows) {
      r"foo\signet\.cookie"
    } else {
      "foo/signet/.cookie"
    }));
  }

  #[test]
  fn mainnet_data_dir() {
    let data_dir = Arguments::try_parse_from(["ord", "index", "update"])
      .unwrap()
      .options
      .data_dir()
      .unwrap()
      .display()
      .to_string();
    assert!(
      data_dir.ends_with(if cfg!(windows) { r"\ord-pepecoin" } else { "/ord-pepecoin" }),
      "{data_dir}"
    );
  }

  #[test]
  fn othernet_data_dir() {
    let data_dir = Arguments::try_parse_from(["ord", "--chain=signet", "index", "update"])
      .unwrap()
      .options
      .data_dir()
      .unwrap()
      .display()
      .to_string();
    assert!(
      data_dir.ends_with(if cfg!(windows) {
        r"\ord-pepecoin\signet"
      } else {
        "/ord-pepecoin/signet"
      }),
      "{data_dir}"
    );
  }

  #[test]
  fn network_is_joined_with_data_dir() {
    let data_dir =
      Arguments::try_parse_from(["ord", "--chain=signet", "--data-dir", "foo", "index", "update"])
        .unwrap()
        .options
        .data_dir()
        .unwrap()
        .display()
        .to_string();
    assert!(
      data_dir.ends_with(if cfg!(windows) {
        r"foo\signet"
      } else {
        "foo/signet"
      }),
      "{data_dir}"
    );
  }

  #[test]
  fn network_accepts_aliases() {
    fn check_network_alias(alias: &str, suffix: &str) {
      let data_dir = Arguments::try_parse_from(["ord", "--chain", alias, "index", "update"])
        .unwrap()
        .options
        .data_dir()
        .unwrap()
        .display()
        .to_string();

      assert!(data_dir.ends_with(suffix), "{data_dir}");
    }

    check_network_alias("main", "ord-pepecoin");
    check_network_alias("mainnet", "ord-pepecoin");
    check_network_alias(
      "regtest",
      if cfg!(windows) {
        r"ord-pepecoin\regtest"
      } else {
        "ord-pepecoin/regtest"
      },
    );
    check_network_alias(
      "signet",
      if cfg!(windows) {
        r"ord-pepecoin\signet"
      } else {
        "ord-pepecoin/signet"
      },
    );
    check_network_alias(
      "test",
      if cfg!(windows) {
        r"ord-pepecoin\testnet3"
      } else {
        "ord-pepecoin/testnet3"
      },
    );
    check_network_alias(
      "testnet",
      if cfg!(windows) {
        r"ord-pepecoin\testnet3"
      } else {
        "ord-pepecoin/testnet3"
      },
    );
  }

  #[test]
  fn rpc_server_chain_must_match() {
    let rpc_server = test_bitcoincore_rpc::builder()
      .network(Network::Testnet)
      .build();

    let tempdir = TempDir::new().unwrap();

    let cookie_file = tempdir.path().join(".cookie");
    fs::write(&cookie_file, "username:password").unwrap();

    let options = Options::try_parse_from([
      "ord",
      "--cookie-file",
      cookie_file.to_str().unwrap(),
      "--rpc-url",
      &rpc_server.url(),
    ])
    .unwrap();

    assert_eq!(
      options.pepecoin_rpc_client().unwrap_err().to_string(),
      "Pepecoin RPC server is on testnet but ord is on mainnet"
    );
  }

  #[test]
  fn chain_flags() {
    Arguments::try_parse_from(["ord", "--signet", "--chain", "signet", "index", "update"]).unwrap_err();
    assert_eq!(
      Arguments::try_parse_from(["ord", "--signet", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Signet
    );
    assert_eq!(
      Arguments::try_parse_from(["ord", "-s", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Signet
    );

    Arguments::try_parse_from(["ord", "--regtest", "--chain", "signet", "index", "update"]).unwrap_err();
    assert_eq!(
      Arguments::try_parse_from(["ord", "--regtest", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Regtest
    );
    assert_eq!(
      Arguments::try_parse_from(["ord", "-r", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Regtest
    );

    Arguments::try_parse_from(["ord", "--testnet", "--chain", "signet", "index", "update"]).unwrap_err();
    assert_eq!(
      Arguments::try_parse_from(["ord", "--testnet", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Testnet
    );
    assert_eq!(
      Arguments::try_parse_from(["ord", "-t", "index", "update"])
        .unwrap()
        .options
        .chain(),
      Chain::Testnet
    );
  }

  #[test]
  fn wallet_flag_overrides_default_name() {
    assert_eq!(
      Arguments::try_parse_from(["ord", "wallet", "create"])
        .unwrap()
        .options
        .wallet,
      "ord"
    );

    assert_eq!(
      Arguments::try_parse_from(["ord", "--wallet", "foo", "wallet", "create"])
        .unwrap()
        .options
        .wallet,
      "foo"
    )
  }

  #[test]
  fn default_config_is_returned_if_config_option_is_not_passed() {
    assert_eq!(
      Arguments::try_parse_from(["ord", "index", "update"])
        .unwrap()
        .options
        .load_config()
        .unwrap(),
      Default::default()
    );
  }

  #[test]
  fn config_is_loaded_from_config_option_path() {
    let id = "8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi0"
      .parse::<InscriptionId>()
      .unwrap();

    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path().join("ord.yaml");
    fs::write(&path, format!("hidden:\n- \"{id}\"")).unwrap();

    assert_eq!(
      Arguments::try_parse_from(["ord", "--config", path.to_str().unwrap(), "index", "update"])
        .unwrap()
        .options
        .load_config()
        .unwrap(),
      Config {
        hidden: iter::once(id).collect(),
        ..Default::default()
      }
    );
  }

  #[test]
  fn config_is_loaded_from_config_dir_option_path() {
    let id = "8d363b28528b0cb86b5fd48615493fb175bdf132d2a3d20b4251bba3f130a5abi0"
      .parse::<InscriptionId>()
      .unwrap();

    let tempdir = TempDir::new().unwrap();

    fs::write(
      tempdir.path().join("ord.yaml"),
      format!("hidden:\n- \"{id}\""),
    )
    .unwrap();

    assert_eq!(
      Arguments::try_parse_from([
        "ord",
        "--config-dir",
        tempdir.path().to_str().unwrap(),
        "index",
        "update",
      ])
      .unwrap()
      .options
      .load_config()
      .unwrap(),
      Config {
        hidden: iter::once(id).collect(),
        ..Default::default()
      }
    );
  }
}
