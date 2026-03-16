#![allow(
  clippy::too_many_arguments,
  clippy::type_complexity,
  clippy::result_large_err
)]
#![deny(
  clippy::cast_lossless,
  clippy::cast_possible_truncation,
  clippy::cast_possible_wrap,
  clippy::cast_sign_loss
)]

use {
  self::{
    arguments::Arguments,
    blocktime::Blocktime,
    config::Config,
    decimal::Decimal,
    deserialize_from_str::DeserializeFromStr,
    epoch::Epoch,
    height::Height,
    inscription::Inscription,
    inscription_id::InscriptionId,
    media::Media,
    options::Options,
    outgoing::Outgoing,
    representation::Representation,
    subcommand::Subcommand,
    tally::Tally,
  },
  anyhow::{anyhow, bail, Context, Error},
  bip39::Mnemonic,
  bitcoin::{
    blockdata::constants::COIN_VALUE,
    consensus::{self, Decodable, Encodable},
    hash_types::BlockHash,
    hashes::Hash,
    Address, Amount, Block, Network, OutPoint, Script, Sequence, Transaction, TxIn, TxOut, Txid,
  },
  bitcoincore_rpc::{Client, RpcApi},
  chain::Chain,
  chrono::{DateTime, TimeZone, Utc},
  clap::{ArgGroup, Parser},
  derive_more::{Display, FromStr},
  lazy_static::lazy_static,
  regex::Regex,
  serde::{Deserialize, Deserializer, Serialize, Serializer},
  std::{
    cmp,
    collections::{BTreeMap, HashSet, VecDeque},
    env,
    fmt::{self, Display, Formatter},
    fs::{self, File},
    io,
    net::ToSocketAddrs,
    ops::{Add, AddAssign, Sub},
    path::{Path, PathBuf},
    process,
    str::FromStr,
    sync::{
      atomic::{self, AtomicU64},
      Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
  },
  tokio::{runtime::Runtime, task},
  url::Url,
};

pub use crate::{
  fee_rate::FeeRate, index::{Index, List}, object::Object, rarity::Rarity, sat::Sat, sat_point::SatPoint,
  subcommand::wallet::transaction_builder::TransactionBuilder,
};

#[cfg(test)]
#[macro_use]
mod test;

#[cfg(test)]
use {self::test::*, std::ffi::OsString, tempfile::TempDir};

macro_rules! tprintln {
    ($($arg:tt)*) => {

      if cfg!(test) {
        eprint!("==> ");
        eprintln!($($arg)*);
      }
    };
}

pub mod api;
mod arguments;
mod blocktime;
mod chain;
mod config;
mod decimal;
mod deserialize_from_str;
mod epoch;
mod fee_rate;
mod height;
pub mod index;
mod inscription;
mod inscription_id;
mod media;
mod object;
pub mod options;
mod outgoing;
mod page_config;
mod rarity;
mod representation;
mod sat;
mod sat_point;
pub mod subcommand;
mod tally;
mod templates;
pub mod wallet;

type Result<T = (), E = Error> = std::result::Result<T, E>;

static INTERRUPTS: AtomicU64 = AtomicU64::new(0);
static LISTENERS: Mutex<Vec<axum_server::Handle<std::net::SocketAddr>>> = Mutex::new(Vec::new());

fn integration_test() -> bool {
  env::var_os("ORD_INTEGRATION_TEST")
    .map(|value| value.len() > 0)
    .unwrap_or(false)
}

fn timestamp(seconds: u32) -> DateTime<Utc> {
  Utc.timestamp_opt(seconds.into(), 0).unwrap()
}

pub fn parse_ord_server_args(args: &str) -> (Options, subcommand::server::Server) {
  match Arguments::try_parse_from(args.split_whitespace()) {
    Ok(arguments) => match arguments.subcommand {
      subcommand::Subcommand::Server(server) => (arguments.options, server),
      subcommand => panic!("unexpected subcommand: {subcommand:?}"),
    },
    Err(err) => panic!("error parsing arguments: {err}"),
  }
}

const INTERRUPT_LIMIT: u64 = 2;

pub fn main() {
  env_logger::init();

  ctrlc::set_handler(move || {
    LISTENERS
      .lock()
      .unwrap()
      .iter()
      .for_each(|handle| handle.graceful_shutdown(Some(Duration::from_secs(5))));

    println!("Detected Ctrl-C, attempting to shut down ordpep gracefully. Press Ctrl-C {INTERRUPT_LIMIT} times to force shutdown.");

    let interrupts = INTERRUPTS.fetch_add(1, atomic::Ordering::Relaxed);

    if interrupts > INTERRUPT_LIMIT {
      process::exit(1);
    }
  })
  .expect("Error setting ctrl-c handler");

  if let Err(err) = Arguments::parse().run() {
    eprintln!("error: {err}");
    for (i, cause) in err.chain().skip(1).enumerate() {
      if i == 0 {
        eprintln!();
        eprintln!("because:");
      }
      eprintln!("- {cause}");
    }
    if env::var_os("RUST_BACKTRACE")
      .map(|val| val == "1")
      .unwrap_or_default()
    {
      eprintln!("{}", err.backtrace());
    }
    process::exit(1);
  }
}
