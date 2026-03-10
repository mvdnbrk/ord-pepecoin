use {
  super::*,
  bitcoin::secp256k1::{
    rand::{self, RngCore},
    All, Secp256k1,
  },
  bitcoin::{
    util::bip32::{ChildNumber, DerivationPath, ExtendedPrivKey},
    PrivateKey,
  },
  fee_rate::FeeRate,
  transaction_builder::TransactionBuilder,
};

pub mod balance;
pub mod create;
pub(crate) mod inscribe;
pub mod inscriptions;
pub mod outputs;
pub mod receive;
mod restore;
pub mod sats;
pub mod send;
pub(crate) mod transaction_builder;
pub mod transactions;

#[derive(Debug, Parser)]
pub(crate) enum Wallet {
  #[clap(about = "Get wallet balance")]
  Balance,
  #[clap(about = "Create new wallet")]
  Create(create::Create),
  #[clap(about = "Create inscription")]
  Inscribe(inscribe::Inscribe),
  #[clap(about = "List wallet inscriptions")]
  Inscriptions,
  #[clap(about = "Generate receive address")]
  Receive,
  #[clap(about = "Restore wallet")]
  Restore(restore::Restore),
  #[clap(about = "List wallet satoshis")]
  Sats(sats::Sats),
  #[clap(about = "Send sat or inscription")]
  Send(send::Send),
  #[clap(about = "See wallet transactions")]
  Transactions(transactions::Transactions),
  #[clap(about = "List wallet outputs")]
  Outputs,
}

impl Wallet {
  pub(crate) fn run(self, options: Options) -> Result {
    match self {
      Self::Balance => balance::run(options),
      Self::Create(create) => create.run(options),
      Self::Inscribe(inscribe) => inscribe.run(options),
      Self::Inscriptions => inscriptions::run(options),
      Self::Receive => receive::run(options),
      Self::Restore(restore) => restore.run(options),
      Self::Sats(sats) => sats.run(options),
      Self::Send(send) => send.run(options),
      Self::Transactions(transactions) => transactions.run(options),
      Self::Outputs => outputs::run(options),
    }
  }
}

fn get_change_address(client: &Client) -> Result<Address> {
  client
    .call("getrawchangeaddress", &[])
    .context("could not get change addresses from wallet")
}

// BIP-44 derivation path for Pepecoin: m/44'/3434'/0'
// SLIP-0044 coin type 3434 for Pepecoin
const PEPECOIN_COIN_TYPE: u32 = 3434;
const NUM_DERIVE_KEYS: u32 = 20;

pub(crate) fn initialize_wallet(options: &Options, seed: [u8; 64]) -> Result {
  let client = options.pepecoin_rpc_client_for_wallet_command(true)?;
  let network = options.chain().network();

  let secp = Secp256k1::new();

  let master_private_key = ExtendedPrivKey::new_master(network, &seed)?;

  // m/44'/3434'/0'
  let derivation_path = DerivationPath::master()
    .child(ChildNumber::Hardened { index: 44 })
    .child(ChildNumber::Hardened { index: PEPECOIN_COIN_TYPE })
    .child(ChildNumber::Hardened { index: 0 });

  let account_key = master_private_key.derive_priv(&secp, &derivation_path)?;

  // Import receive keys (m/44'/3434'/0'/0/i) and change keys (m/44'/3434'/0'/1/i)
  for change in [false, true] {
    let chain_key = account_key.derive_priv(
      &secp,
      &DerivationPath::master().child(ChildNumber::Normal {
        index: u32::from(change),
      }),
    )?;

    for i in 0..NUM_DERIVE_KEYS {
      let child_key = chain_key.derive_priv(
        &secp,
        &DerivationPath::master().child(ChildNumber::Normal { index: i }),
      )?;

      let private_key = PrivateKey::new(child_key.private_key, network);

      let label = if change {
        format!("ord-change-{i}")
      } else {
        format!("ord-receive-{i}")
      };

      client.import_private_key(&private_key, Some(&label), Some(false))?;
    }
  }

  Ok(())
}
