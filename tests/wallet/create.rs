use super::*;

#[test]
fn create() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let tempdir = TempDir::new().unwrap();

  CommandBuilder::new("wallet create")
    .rpc_server(&rpc_server)
    .data_dir(tempdir.path().to_owned())
    .output::<Create>();

  assert!(tempdir.path().join("wallets/ordpep/wallet.redb").exists());
}

#[test]
fn seed_phrases_are_twelve_words_long() {
  let Create { mnemonic } = CommandBuilder::new("wallet create")
    .rpc_server(&test_bitcoincore_rpc::spawn())
    .output::<Create>();

  assert_eq!(mnemonic.word_count(), 12);
}

#[test]
fn create_with_different_name() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let tempdir = TempDir::new().unwrap();

  CommandBuilder::new("wallet create")
    .wallet("inscription-wallet")
    .rpc_server(&rpc_server)
    .data_dir(tempdir.path().to_owned())
    .output::<Create>();

  assert!(tempdir.path().join("wallets/inscription-wallet/wallet.redb").exists());
}
