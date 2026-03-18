use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Restore {
  #[clap(help = "Restore wallet from <MNEMONIC>")]
  mnemonic: Mnemonic,
  #[clap(
    long,
    default_value = "",
    help = "Use <PASSPHRASE> when deriving wallet"
  )]
  pub(crate) passphrase: String,
}

impl Restore {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    crate::wallet::Wallet::initialize(&settings, wallet_name, self.mnemonic.to_seed(self.passphrase))?;

    crate::wallet::Wallet::import_addresses(&settings, wallet_name, true)?;

    Ok(())
  }
}
