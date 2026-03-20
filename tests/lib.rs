#![allow(clippy::type_complexity)]

use {
  self::{command_builder::CommandBuilder, expected::Expected, test_server::TestServer},
  bip39::Mnemonic,
  bitcoin::{
    blockdata::constants::COIN_VALUE,
    secp256k1::Secp256k1,
    util::bip32::{ChildNumber, DerivationPath, ExtendedPrivKey},
    Address, Network, OutPoint, PrivateKey, Txid,
  },
  executable_path::executable_path,
  pretty_assertions::assert_eq as pretty_assert_eq,
  regex::Regex,
  reqwest::{StatusCode, Url},
  serde::{de::DeserializeOwned, Deserialize},
  std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::{self, FromStr},
    thread,
    time::Duration,
  },
  tempfile::TempDir,
};

macro_rules! assert_regex_match {
  ($string:expr, $pattern:expr $(,)?) => {
    let regex = Regex::new(&format!("^(?s){}$", $pattern)).unwrap();
    let string = $string;

    if !regex.is_match(string.as_ref()) {
      panic!(
        "Regex:\n\n{}\n\n…did not match string:\n\n{}",
        regex, string
      );
    }
  };
}

#[derive(Deserialize, Debug)]
struct Inscribe {
  #[allow(dead_code)]
  commit: Txid,
  inscription: String,
  reveal: Txid,
  #[allow(dead_code)]
  destination: String,
  fees: u64,
}

#[derive(Deserialize, Debug)]
struct BatchInscribe {
  #[allow(dead_code)]
  commit: Txid,
  inscriptions: Vec<BatchInscription>,
  #[allow(dead_code)]
  total_fees: u64,
}

#[derive(Deserialize, Debug)]
struct BatchInscription {
  inscription: String,
  #[allow(dead_code)]
  reveal: Txid,
  destination: String,
}

fn inscribe(rpc_server: &test_bitcoincore_rpc::Handle, ord_server: &TestServer) -> Inscribe {
  rpc_server.mine_blocks(1);

  let output = CommandBuilder::new("wallet inscribe foo.txt")
    .write("foo.txt", "FOO")
    .rpc_server(rpc_server)
    .ord_server(ord_server)
    .data_dir(ord_server.directory())
    .output();

  rpc_server.mine_blocks(1);

  output
}

#[derive(Deserialize)]
struct Create {
  mnemonic: Mnemonic,
}

fn create_wallet_with_data_dir(
  rpc_server: &test_bitcoincore_rpc::Handle,
  data_dir: Option<PathBuf>,
) {
  create_wallet_with_options(rpc_server, data_dir, None)
}

fn create_wallet_with_options(
  rpc_server: &test_bitcoincore_rpc::Handle,
  data_dir: Option<PathBuf>,
  wallet_name: Option<&str>,
) {
  let mut builder = CommandBuilder::new(format!("--chain {} wallet create", rpc_server.network()));

  if let Some(wallet_name) = wallet_name {
    builder = builder.wallet(wallet_name);
  }

  if let Some(ref data_dir) = data_dir {
    builder = builder.data_dir(data_dir.clone());
  }
  let Create { mnemonic } = builder.rpc_server(rpc_server).output::<Create>();

  let master_private_key =
    ExtendedPrivKey::new_master(rpc_server.network_enum(), &mnemonic.to_seed("")).unwrap();
  let secp = Secp256k1::new();

  // m/44'/3434'/0'/0/0
  let derivation_path = DerivationPath::master()
    .child(ChildNumber::Hardened { index: 44 })
    .child(ChildNumber::Hardened { index: 3434 })
    .child(ChildNumber::Hardened { index: 0 })
    .child(ChildNumber::Normal { index: 0 })
    .child(ChildNumber::Normal { index: 0 });

  let child_key = master_private_key
    .derive_priv(&secp, &derivation_path)
    .unwrap();
  let privkey = PrivateKey::new(child_key.private_key, rpc_server.network_enum());
  let address = Address::p2pkh(&privkey.public_key(&secp), privkey.network);

  rpc_server.set_coinbase_address(&address);
}

mod command_builder;
mod epochs;
mod expected;
mod find;
mod index;
mod info;
mod json_api;
mod list;
mod parse;
mod server;
mod subsidy;
mod test_server;
mod traits;
mod version;
mod wallet;
