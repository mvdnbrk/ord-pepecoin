use super::*;

#[derive(Clone, Debug, Parser)]
#[clap(group(
  ArgGroup::new("chains")
    .required(false)
    .args(&["chain-argument", "signet", "regtest", "testnet"]),
))]
pub struct Options {
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
  pub(crate) first_inscription_height: Option<u32>,
  #[clap(long, help = "Limit index to <HEIGHT_LIMIT> blocks.")]
  pub(crate) height_limit: Option<u32>,
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
    }
  }
}
