use {
  super::*,
  bitcoin::secp256k1::rand::{self, RngCore},
};

#[derive(Serialize)]
struct Output {
  mnemonic: Mnemonic,
  passphrase: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct Create {
  #[clap(
    long,
    default_value = "",
    help = "Use <PASSPHRASE> to derive wallet seed."
  )]
  pub(crate) passphrase: String,
}

impl Create {
  pub(crate) fn run(self, settings: Settings, wallet_name: &str) -> Result {
    let mut entropy = [0; 16];
    rand::thread_rng().fill_bytes(&mut entropy);

    let mnemonic = Mnemonic::from_entropy(&entropy)?;

    crate::wallet::Wallet::initialize(
      &settings,
      wallet_name,
      mnemonic.to_seed(self.passphrase.clone()),
    )?;

    crate::wallet::Wallet::import_addresses(&settings, wallet_name, false)?;

    print_json(Output {
      mnemonic,
      passphrase: Some(self.passphrase),
    })?;

    Ok(())
  }
}
