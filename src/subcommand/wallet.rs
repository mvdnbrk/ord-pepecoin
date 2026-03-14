use {
  super::*,
  fee_rate::FeeRate,
  transaction_builder::TransactionBuilder,
};

pub mod balance;
pub(crate) mod batch;
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
      Self::Balance
      | Self::Inscriptions
      | Self::Outputs
      | Self::Receive
      | Self::Sats(_)
      | Self::Send(_)
      | Self::Inscribe(_)
      | Self::Transactions(_) => {
        let wallet = crate::wallet::Wallet::load(&options)?;
        match self {
          Self::Balance => balance::run(wallet),
          Self::Inscriptions => inscriptions::run(wallet),
          Self::Outputs => outputs::run(wallet),
          Self::Receive => receive::run(wallet),
          Self::Sats(sats) => sats.run(wallet),
          Self::Send(send) => send.run(wallet),
          Self::Inscribe(inscribe) => inscribe.run(wallet),
          Self::Transactions(transactions) => transactions.run(wallet),
          _ => unreachable!(),
        }
      }
      Self::Create(create) => create.run(options),
      Self::Restore(restore) => restore.run(options),
    }
  }
}

fn get_change_address(client: &Client) -> Result<Address> {
  client
    .call("getrawchangeaddress", &[])
    .context("could not get change addresses from wallet")
}
