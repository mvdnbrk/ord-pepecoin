use {super::*, bitcoincore_rpc::Auth};

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
  pub(crate) address: Option<String>,
  pub(crate) chain: Option<Chain>,
  pub(crate) commit_interval: Option<usize>,
  pub(crate) config: Option<PathBuf>,
  pub(crate) config_dir: Option<PathBuf>,
  pub(crate) cookie_file: Option<PathBuf>,
  pub(crate) data_dir: Option<PathBuf>,
  pub(crate) first_inscription_height: Option<u32>,
  pub(crate) height_limit: Option<u32>,
  pub(crate) hidden: Option<HashSet<InscriptionId>>,
  pub(crate) http_port: Option<u16>,
  pub(crate) index: Option<PathBuf>,
  pub(crate) index_sats: bool,
  pub(crate) integration_test: bool,
  pub(crate) max_savepoints: Option<usize>,
  pub(crate) pepecoin_data_dir: Option<PathBuf>,
  pub(crate) pepecoin_rpc_limit: Option<u32>,
  pub(crate) pepecoin_rpc_password: Option<String>,
  pub(crate) rpc_url: Option<String>,
  pub(crate) pepecoin_rpc_username: Option<String>,
  pub(crate) savepoint_interval: Option<usize>,
  pub(crate) server_url: Option<Url>,
}

impl Settings {
  pub fn load(options: Options) -> Result<Self> {
    let mut env = BTreeMap::<String, String>::new();

    for (var, value) in env::vars_os() {
      let Some(var) = var.to_str() else {
        continue;
      };

      if let Some(key) = var.strip_prefix("ORDPEP_") {
        env.insert(
          key.into(),
          value.into_string().map_err(|value| {
            anyhow!(
              "environment variable `{var}` not valid unicode: `{}`",
              value.to_string_lossy()
            )
          })?,
        );
      }
    }

    Self::merge(options, env)
  }

  pub(crate) fn merge(options: Options, env: BTreeMap<String, String>) -> Result<Self> {
    let settings = Self::from_options(options).or(Self::from_env(env)?);

    let config_path = if let Some(path) = &settings.config {
      Some(path.into())
    } else {
      let path = if let Some(dir) = settings.config_dir.clone().or(settings.data_dir.clone()) {
        dir
      } else {
        Self::default_data_dir()?
      }
      .join("ordpep.yaml");

      path.exists().then_some(path)
    };

    let config = if let Some(config_path) = config_path {
      serde_yaml::from_reader(File::open(&config_path).context(anyhow!(
        "failed to open config file `{}`",
        config_path.display()
      ))?)
      .context(anyhow!(
        "failed to deserialize config file `{}`",
        config_path.display()
      ))?
    } else {
      Self::default()
    };

    let settings = settings.or(config).or_defaults()?;

    Ok(settings)
  }

  pub(crate) fn or(self, source: Self) -> Self {
    Self {
      address: self.address.or(source.address),
      chain: self.chain.or(source.chain),
      commit_interval: self.commit_interval.or(source.commit_interval),
      config: self.config.or(source.config),
      config_dir: self.config_dir.or(source.config_dir),
      cookie_file: self.cookie_file.or(source.cookie_file),
      data_dir: self.data_dir.or(source.data_dir),
      first_inscription_height: self.first_inscription_height.or(source.first_inscription_height),
      height_limit: self.height_limit.or(source.height_limit),
      hidden: Some(
        self
          .hidden
          .iter()
          .flatten()
          .chain(source.hidden.iter().flatten())
          .cloned()
          .collect(),
      ),
      http_port: self.http_port.or(source.http_port),
      index: self.index.or(source.index),
      index_sats: self.index_sats || source.index_sats,
      integration_test: self.integration_test || source.integration_test,
      max_savepoints: self.max_savepoints.or(source.max_savepoints),
      pepecoin_data_dir: self.pepecoin_data_dir.or(source.pepecoin_data_dir),
      pepecoin_rpc_limit: self.pepecoin_rpc_limit.or(source.pepecoin_rpc_limit),
      pepecoin_rpc_password: self.pepecoin_rpc_password.or(source.pepecoin_rpc_password),
      rpc_url: self.rpc_url.or(source.rpc_url),
      pepecoin_rpc_username: self.pepecoin_rpc_username.or(source.pepecoin_rpc_username),
      savepoint_interval: self.savepoint_interval.or(source.savepoint_interval),
      server_url: self.server_url.or(source.server_url),
    }
  }

  fn from_options(options: Options) -> Self {
    Self {
      address: None,
      chain: options
        .signet
        .then_some(Chain::Signet)
        .or(options.regtest.then_some(Chain::Regtest))
        .or(options.testnet.then_some(Chain::Testnet))
        .or(Some(options.chain_argument)),
      commit_interval: None,
      config: options.config,
      config_dir: options.config_dir,
      cookie_file: options.cookie_file,
      data_dir: options.data_dir,
      first_inscription_height: options.first_inscription_height,
      height_limit: options.height_limit,
      hidden: None,
      http_port: None,
      index: options.index,
      index_sats: options.index_sats,
      integration_test: integration_test(),
      max_savepoints: None,
      pepecoin_data_dir: options.pepecoin_data_dir,
      pepecoin_rpc_limit: None,
      pepecoin_rpc_password: None,
      rpc_url: options.rpc_url,
      pepecoin_rpc_username: None,
      savepoint_interval: None,
      server_url: None,
    }
  }

  fn from_env(env: BTreeMap<String, String>) -> Result<Self> {
    let get_bool = |key: &str| {
      env
        .get(key)
        .map(|value| !value.is_empty())
        .unwrap_or_default()
    };

    let get_string = |key: &str| env.get(key).cloned();

    let get_path = |key: &str| env.get(key).map(PathBuf::from);

    let get_chain = |key: &str| {
      env
        .get(key)
        .map(|chain| chain.parse::<Chain>())
        .transpose()
        .with_context(|| format!("failed to parse environment variable ORDPEP_{key} as chain"))
    };

    let inscriptions = |key: &str| {
      env
        .get(key)
        .map(|inscriptions| {
          inscriptions
            .split_whitespace()
            .map(|inscription_id| inscription_id.parse::<InscriptionId>())
            .collect::<Result<HashSet<InscriptionId>, inscription_id::ParseError>>()
        })
        .transpose()
        .with_context(|| {
          format!("failed to parse environment variable ORDPEP_{key} as inscription list")
        })
    };

    let get_u16 = |key: &str| {
      env
        .get(key)
        .map(|int| int.parse::<u16>())
        .transpose()
        .with_context(|| format!("failed to parse environment variable ORDPEP_{key} as u16"))
    };

    let get_u32 = |key: &str| {
      env
        .get(key)
        .map(|int| int.parse::<u32>())
        .transpose()
        .with_context(|| format!("failed to parse environment variable ORDPEP_{key} as u32"))
    };

    let get_usize = |key: &str| {
      env
        .get(key)
        .map(|int| int.parse::<usize>())
        .transpose()
        .with_context(|| format!("failed to parse environment variable ORDPEP_{key} as usize"))
    };

    let get_url = |key: &str| {
      env
        .get(key)
        .map(|url| url.parse::<Url>())
        .transpose()
        .with_context(|| format!("failed to parse environment variable ORDPEP_{key} as URL"))
    };

    Ok(Self {
      address: get_string("ADDRESS"),
      chain: get_chain("CHAIN")?,
      commit_interval: get_usize("COMMIT_INTERVAL")?,
      config: get_path("CONFIG"),
      config_dir: get_path("CONFIG_DIR"),
      cookie_file: get_path("COOKIE_FILE"),
      data_dir: get_path("DATA_DIR"),
      first_inscription_height: get_u32("FIRST_INSCRIPTION_HEIGHT")?,
      height_limit: get_u32("HEIGHT_LIMIT")?,
      hidden: inscriptions("HIDDEN")?,
      http_port: get_u16("HTTP_PORT")?,
      index: get_path("INDEX"),
      index_sats: get_bool("INDEX_SATS"),
      integration_test: get_bool("INTEGRATION_TEST"),
      max_savepoints: get_usize("MAX_SAVEPOINTS")?,
      pepecoin_data_dir: get_path("PEPECOIN_DATA_DIR"),
      pepecoin_rpc_limit: get_u32("PEPECOIN_RPC_LIMIT")?,
      pepecoin_rpc_password: get_string("PEPECOIN_RPC_PASSWORD"),
      rpc_url: get_string("RPC_URL"),
      pepecoin_rpc_username: get_string("PEPECOIN_RPC_USERNAME"),
      savepoint_interval: get_usize("SAVEPOINT_INTERVAL")?,
      server_url: get_url("SERVER_URL")?,
    })
  }

  fn or_defaults(self) -> Result<Self> {
    let chain = self.chain.unwrap_or_default();

    let pepecoin_data_dir = match &self.pepecoin_data_dir {
      Some(pepecoin_data_dir) => pepecoin_data_dir.clone(),
      None => {
        if cfg!(target_os = "linux") {
          dirs::home_dir()
            .ok_or_else(|| anyhow!("failed to retrieve home dir"))?
            .join(".pepecoin")
        } else {
          dirs::data_dir()
            .ok_or_else(|| anyhow!("failed to retrieve data dir"))?
            .join("Pepecoin")
        }
      }
    };

    let cookie_file = match self.cookie_file {
      Some(cookie_file) => cookie_file,
      None => chain.join_with_data_dir(&pepecoin_data_dir).join(".cookie"),
    };

    let data_dir = match &self.data_dir {
      Some(data_dir) => chain.join_with_data_dir(data_dir),
      None => chain.join_with_data_dir(&Self::default_data_dir()?),
    };

    let index = match &self.index {
      Some(path) => path.clone(),
      None => data_dir.join("index.redb"),
    };

    Ok(Self {
      address: self.address,
      chain: Some(chain),
      commit_interval: Some(self.commit_interval.unwrap_or(5000)),
      config: None,
      config_dir: None,
      cookie_file: Some(cookie_file),
      data_dir: Some(data_dir),
      first_inscription_height: Some(
        self
          .first_inscription_height
          .unwrap_or_else(|| chain.first_inscription_height()),
      ),
      height_limit: self.height_limit,
      hidden: self.hidden,
      http_port: self.http_port,
      index: Some(index),
      index_sats: self.index_sats,
      integration_test: self.integration_test,
      max_savepoints: Some(self.max_savepoints.unwrap_or(3)),
      pepecoin_data_dir: Some(pepecoin_data_dir),
      pepecoin_rpc_limit: Some(self.pepecoin_rpc_limit.unwrap_or(16)),
      pepecoin_rpc_password: self.pepecoin_rpc_password,
      rpc_url: Some(
        self
          .rpc_url
          .clone()
          .unwrap_or_else(|| format!("127.0.0.1:{}", chain.default_rpc_port())),
      ),
      pepecoin_rpc_username: self.pepecoin_rpc_username,
      savepoint_interval: Some(self.savepoint_interval.unwrap_or(10)),
      server_url: self.server_url,
    })
  }

  fn default_data_dir() -> Result<PathBuf> {
    Ok(
      dirs::data_dir()
        .context("could not get data dir")?
        .join("ordpep"),
    )
  }

  pub(crate) fn chain(&self) -> Chain {
    self.chain.unwrap()
  }

  pub(crate) fn commit_interval(&self) -> usize {
    self.commit_interval.unwrap()
  }

  pub(crate) fn cookie_file(&self) -> Result<PathBuf> {
    Ok(self.cookie_file.clone().unwrap())
  }

  pub(crate) fn data_dir(&self) -> PathBuf {
    self.data_dir.clone().unwrap()
  }

  pub(crate) fn first_inscription_height(&self) -> u32 {
    if self.integration_test {
      0
    } else {
      self.first_inscription_height.unwrap()
    }
  }

  pub(crate) fn height_limit(&self) -> Option<u32> {
    self.height_limit
  }

  pub(crate) fn index(&self) -> PathBuf {
    self.index.clone().unwrap()
  }

  pub(crate) fn index_sats(&self) -> bool {
    self.index_sats
  }

  pub(crate) fn integration_test(&self) -> bool {
    self.integration_test
  }

  pub(crate) fn is_hidden(&self, inscription_id: InscriptionId) -> bool {
    self
      .hidden
      .as_ref()
      .map(|hidden| hidden.contains(&inscription_id))
      .unwrap_or_default()
  }

  pub(crate) fn max_savepoints(&self) -> usize {
    self.max_savepoints.unwrap()
  }

  #[allow(dead_code)]
  pub(crate) fn pepecoin_rpc_limit(&self) -> u32 {
    self.pepecoin_rpc_limit.unwrap()
  }

  pub(crate) fn rpc_url(&self) -> String {
    self.rpc_url.clone().unwrap()
  }

  pub(crate) fn savepoint_interval(&self) -> usize {
    self.savepoint_interval.unwrap()
  }

  pub(crate) fn server_url(&self) -> Result<Url> {
    self
      .server_url
      .clone()
      .ok_or_else(|| anyhow!("server URL not specified"))
  }

  pub(crate) fn auth(&self) -> Result<Auth> {
    if let (Some(user), Some(pass)) = (
      &self.pepecoin_rpc_username,
      &self.pepecoin_rpc_password,
    ) {
      Ok(Auth::UserPass(user.clone(), pass.clone()))
    } else {
      Ok(Auth::CookieFile(self.cookie_file()?))
    }
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
      bail!("Pepecoin RPC server is on {rpc_chain} but ordpep is on {ord_chain}");
    }
    Ok(client)
  }

  // Pepecoin Core 1.1.0 doesn't support /wallet/<name> HTTP endpoint.
  // Keep for future use when multi-wallet HTTP routing is available.
  #[allow(dead_code)]
  pub(crate) fn pepecoin_rpc_client_for_wallet(&self, wallet_name: &str) -> Result<Client> {
    let rpc_url = format!("{}/wallet/{}", self.rpc_url(), wallet_name);
    let auth = self.auth()?;

    log::info!("Connecting to Pepecoin Core RPC server for wallet `{wallet_name}` at {rpc_url}");

    Client::new(&rpc_url, auth)
      .with_context(|| format!("failed to connect to Pepecoin Core RPC at {rpc_url}"))
  }

  pub(crate) fn pepecoin_rpc_client_for_wallet_command(&self) -> Result<Client> {
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
        format!(
          "{}.{}.{}.{}",
          MIN_VERSION / 1000000,
          MIN_VERSION % 1000000 / 10000,
          MIN_VERSION % 10000 / 100,
          MIN_VERSION % 100
        ),
        format!(
          "{}.{}.{}.{}",
          pepecoin_version / 1000000,
          pepecoin_version % 1000000 / 10000,
          pepecoin_version % 10000 / 100,
          pepecoin_version % 100
        ),
      );
    }

    Ok(client)
  }

  pub(crate) fn runtime(&self) -> Result<Runtime> {
    if cfg!(test) || self.integration_test() {
      tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    } else {
      Runtime::new()
    }
    .context("failed to initialize runtime")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Settings {
    let mut args = args.to_vec();
    args.insert(0, "ordpep");
    let settings = Settings::from_options(Options::try_parse_from(args).unwrap());
    settings.or_defaults().unwrap()
  }

  #[test]
  fn rpc_url_overrides_network() {
    assert_eq!(
      parse(&["--rpc-url=127.0.0.1:1234", "--chain=signet"]).rpc_url(),
      "127.0.0.1:1234"
    );
  }

  #[test]
  fn cookie_file_overrides_network() {
    assert_eq!(
      parse(&["--cookie-file=/foo/bar", "--chain=signet"])
        .cookie_file()
        .unwrap(),
      Path::new("/foo/bar")
    );
  }

  #[test]
  fn use_default_network() {
    let settings = parse(&[]);

    assert_eq!(settings.rpc_url(), "127.0.0.1:33873");

    assert!(settings.cookie_file().unwrap().ends_with(".cookie"));
  }

  #[test]
  fn mainnet_cookie_file_path() {
    let cookie_file = parse(&[]).cookie_file().unwrap().display().to_string();

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
    let cookie_file = parse(&["--chain=signet"])
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
    let cookie_file = parse(&["--pepecoin-data-dir=foo", "--chain=signet"])
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
    let data_dir = parse(&[]).data_dir().display().to_string();
    assert!(
      data_dir.ends_with(if cfg!(windows) { r"\ordpep" } else { "/ordpep" }),
      "{data_dir}"
    );
  }

  #[test]
  fn othernet_data_dir() {
    let data_dir = parse(&["--chain=signet"]).data_dir().display().to_string();
    assert!(
      data_dir.ends_with(if cfg!(windows) {
        r"\ordpep\signet"
      } else {
        "/ordpep/signet"
      }),
      "{data_dir}"
    );
  }

  #[test]
  fn network_is_joined_with_data_dir() {
    let data_dir = parse(&["--chain=signet", "--data-dir", "foo"])
      .data_dir()
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
    #[track_caller]
    fn check_network_alias(alias: &str, suffix: &str) {
      let data_dir = parse(&["--chain", alias]).data_dir().display().to_string();

      assert!(data_dir.ends_with(suffix), "{data_dir}");
    }

    check_network_alias("main", "ordpep");
    check_network_alias("mainnet", "ordpep");
    check_network_alias(
      "regtest",
      if cfg!(windows) {
        r"ordpep\regtest"
      } else {
        "ordpep/regtest"
      },
    );
    check_network_alias(
      "signet",
      if cfg!(windows) {
        r"ordpep\signet"
      } else {
        "ordpep/signet"
      },
    );
    check_network_alias(
      "test",
      if cfg!(windows) {
        r"ordpep\testnet3"
      } else {
        "ordpep/testnet3"
      },
    );
    check_network_alias(
      "testnet",
      if cfg!(windows) {
        r"ordpep\testnet3"
      } else {
        "ordpep/testnet3"
      },
    );
  }

  #[test]
  fn chain_flags() {
    assert_eq!(parse(&["--signet"]).chain(), Chain::Signet);
    assert_eq!(parse(&["-s"]).chain(), Chain::Signet);
    assert_eq!(parse(&["--regtest"]).chain(), Chain::Regtest);
    assert_eq!(parse(&["-r"]).chain(), Chain::Regtest);
    assert_eq!(parse(&["--testnet"]).chain(), Chain::Testnet);
    assert_eq!(parse(&["-t"]).chain(), Chain::Testnet);
  }

  #[test]
  fn from_env() {
    let mut env = BTreeMap::new();
    env.insert("CHAIN".into(), "signet".into());
    env.insert("RPC_URL".into(), "127.0.0.1:1234".into());
    env.insert("INDEX_SATS".into(), "1".into());
    env.insert("PEPECOIN_RPC_LIMIT".into(), "10".into());

    let settings = Settings::from_env(env).unwrap();
    assert_eq!(settings.chain, Some(Chain::Signet));
    assert_eq!(settings.rpc_url, Some("127.0.0.1:1234".into()));
    assert!(settings.index_sats);
    assert_eq!(settings.pepecoin_rpc_limit, Some(10));
  }

  #[test]
  fn merge_precedence() {
    let mut env = BTreeMap::new();
    env.insert("RPC_URL".into(), "env".into());

    let options = Options {
      rpc_url: Some("cli".into()),
      ..Default::default()
    };

    let settings = Settings::merge(options, env).unwrap();
    assert_eq!(settings.rpc_url(), "cli");
  }
}
