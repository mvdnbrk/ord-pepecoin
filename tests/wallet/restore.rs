use {
  super::*,
  clap::Parser as ClapParser,
  ord::options::Options,
  redb::{ReadableDatabase, TableDefinition},
  std::collections::BTreeMap,
};

const DESCRIPTORS: TableDefinition<&str, &str> = TableDefinition::new("DESCRIPTORS");

fn read_descriptors(data_dir: &std::path::Path) -> BTreeMap<String, String> {
  let options = Options::parse_from(["ordpep", "--data-dir", data_dir.to_str().unwrap()]);
  let settings = ord::settings::Settings::load(options).unwrap();
  let db = ord::wallet::Wallet::open_database(&settings, "ordpep").unwrap();
  let rtx = db.begin_read().unwrap();
  let table = rtx.open_table(DESCRIPTORS).unwrap();
  let mut descs = BTreeMap::new();
  if let Some(v) = table.get("receive").unwrap() {
    descs.insert("receive".to_string(), v.value().to_string());
  }
  if let Some(v) = table.get("change").unwrap() {
    descs.insert("change".to_string(), v.value().to_string());
  }
  descs
}

#[test]
fn restore_generates_same_descriptors() {
  let tempdir = TempDir::new().unwrap();
  let rpc_server = test_bitcoincore_rpc::spawn();

  let Create { mnemonic } = CommandBuilder::new("wallet create")
    .rpc_server(&rpc_server)
    .data_dir(tempdir.path().to_owned())
    .output::<Create>();

  let descriptors = read_descriptors(tempdir.path());

  let tempdir2 = TempDir::new().unwrap();
  CommandBuilder::new(["wallet", "restore", &mnemonic.to_string()])
    .rpc_server(&rpc_server)
    .data_dir(tempdir2.path().to_owned())
    .run();

  let restored_descriptors = read_descriptors(tempdir2.path());

  assert_eq!(restored_descriptors, descriptors);
}

#[test]
fn restore_generates_same_descriptors_with_passphrase() {
  let passphrase = "foo";
  let tempdir = TempDir::new().unwrap();
  let rpc_server = test_bitcoincore_rpc::spawn();

  let Create { mnemonic } = CommandBuilder::new(["wallet", "create", "--passphrase", passphrase])
    .rpc_server(&rpc_server)
    .data_dir(tempdir.path().to_owned())
    .output::<Create>();

  let descriptors = read_descriptors(tempdir.path());

  let tempdir2 = TempDir::new().unwrap();
  CommandBuilder::new([
    "wallet",
    "restore",
    "--passphrase",
    passphrase,
    &mnemonic.to_string(),
  ])
  .rpc_server(&rpc_server)
  .data_dir(tempdir2.path().to_owned())
  .run();

  let restored_descriptors = read_descriptors(tempdir2.path());

  assert_eq!(restored_descriptors, descriptors);
}
