use super::*;

pub mod epochs;
pub mod find;
mod index;
pub mod info;
pub mod list;
pub mod parse;
pub mod server;
pub mod subsidy;
pub mod traits;
pub mod wallet;

fn print_json(output: impl Serialize) -> Result {
  serde_json::to_writer_pretty(io::stdout(), &output)?;
  println!();
  Ok(())
}

#[derive(Debug, Parser)]
pub(crate) enum Subcommand {
  #[clap(about = "List the first satoshis of each reward epoch")]
  Epochs,
  #[clap(about = "Find a satoshi's current location")]
  Find(find::Find),
  #[clap(subcommand, about = "Index commands")]
  Index(index::IndexSubcommand),
  #[clap(about = "Display index statistics")]
  Info(info::Info),
  #[clap(about = "List the satoshis in an output")]
  List(list::List),
  #[clap(about = "Parse a satoshi from ordinal notation")]
  Parse(parse::Parse),
  #[clap(about = "Display information about a block's subsidy")]
  Subsidy(subsidy::Subsidy),
  #[clap(about = "Run the explorer server")]
  Server(server::Server),
  #[clap(about = "Display satoshi traits")]
  Traits(traits::Traits),
  #[clap(about = "Wallet commands")]
  Wallet(wallet::WalletCommand),
}

impl Subcommand {
  pub(crate) fn run(self, settings: Settings) -> Result {
    match self {
      Self::Epochs => epochs::run(),
      Self::Find(find) => find.run(settings),
      Self::Index(index) => index.run(settings),
      Self::Info(info) => info.run(settings),
      Self::List(list) => list.run(settings),
      Self::Parse(parse) => parse.run(),
      Self::Subsidy(subsidy) => subsidy.run(),
      Self::Server(server) => {
        let index = Arc::new(Index::open(&settings)?);
        let handle = axum_server::Handle::new();
        LISTENERS.lock().unwrap().push(handle.clone());
        server.run(settings, index, handle, None)
      }
      Self::Traits(traits) => traits.run(),
      Self::Wallet(wallet) => wallet.run(settings),
    }
  }
}
