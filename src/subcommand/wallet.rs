use {
  super::*,
  fee_rate::FeeRate,
  transaction_builder::TransactionBuilder,
};

pub mod addresses;
pub mod balance;
pub(crate) mod batch;
pub mod broadcast;
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
pub(crate) struct WalletCommand {
  #[clap(long, global = true, default_value = "ordpep", help = "Use wallet named <NAME>.")]
  pub(crate) name: String,
  #[clap(long, global = true, alias = "nosync", help = "Do not update index.")]
  pub(crate) no_sync: bool,
  #[clap(long, global = true, help = "Use ordpep server running at <SERVER_URL>.")]
  pub(crate) server_url: Option<Url>,
  #[clap(subcommand)]
  pub(crate) subcommand: WalletSubcommand,
}

#[derive(Debug, Parser)]
pub(crate) enum WalletSubcommand {
  #[clap(about = "List wallet addresses")]
  Addresses,
  #[clap(about = "Get wallet balance")]
  Balance,
  #[clap(about = "Broadcast reveal transactions")]
  Broadcast(broadcast::Broadcast),
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
  Outputs(outputs::Outputs),
}

impl WalletCommand {
  pub(crate) fn run(self, settings: Settings) -> Result {
    let wallet_name = self.name;
    let no_sync = self.no_sync;
    let server_url = self.server_url;
    match self.subcommand {
      WalletSubcommand::Addresses
      | WalletSubcommand::Balance
      | WalletSubcommand::Inscriptions
      | WalletSubcommand::Outputs(_)
      | WalletSubcommand::Receive
      | WalletSubcommand::Sats(_)
      | WalletSubcommand::Send(_)
      | WalletSubcommand::Inscribe(_)
      | WalletSubcommand::Transactions(_) => {
        if let WalletSubcommand::Inscribe(ref inscribe) = self.subcommand {
          inscribe.validate_files()?;
        }
        let wallet = crate::wallet::Wallet::load(&settings, &wallet_name, server_url, no_sync)?;
        match self.subcommand {
          WalletSubcommand::Addresses => addresses::run(wallet),
          WalletSubcommand::Balance => balance::run(wallet),
          WalletSubcommand::Inscriptions => inscriptions::run(wallet),
          WalletSubcommand::Outputs(outputs) => outputs.run(wallet),
          WalletSubcommand::Receive => receive::run(wallet),
          WalletSubcommand::Sats(sats) => sats.run(wallet),
          WalletSubcommand::Send(send) => send.run(wallet),
          WalletSubcommand::Inscribe(inscribe) => inscribe.run(wallet),
          WalletSubcommand::Transactions(transactions) => transactions.run(wallet),
          _ => unreachable!(),
        }
      }
      WalletSubcommand::Create(create) => create.run(settings, &wallet_name),
      WalletSubcommand::Restore(restore) => restore.run(settings, &wallet_name),
      WalletSubcommand::Broadcast(broadcast) => broadcast.run(settings, &wallet_name),
    }
  }
}

fn get_change_address(client: &Client) -> Result<Address> {
  client
    .call("getrawchangeaddress", &[])
    .context("could not get change addresses from wallet")
}
