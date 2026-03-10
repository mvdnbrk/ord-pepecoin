use super::*;

#[test]
fn create() {
  let rpc_server = test_bitcoincore_rpc::spawn();

  CommandBuilder::new("wallet create")
    .rpc_server(&rpc_server)
    .output::<Create>();

  let imported_privkeys = rpc_server.imported_privkeys();
  assert_eq!(imported_privkeys.len(), 40);
  
  for i in 0..20 {
    assert!(imported_privkeys.iter().any(|(_, label)| label.as_deref() == Some(&format!("ord-receive-{i}"))));
    assert!(imported_privkeys.iter().any(|(_, label)| label.as_deref() == Some(&format!("ord-change-{i}"))));
  }
}

#[test]
fn seed_phrases_are_twelve_words_long() {
  let Create { mnemonic } = CommandBuilder::new("wallet create")
    .rpc_server(&test_bitcoincore_rpc::spawn())
    .output::<Create>();

  assert_eq!(mnemonic.word_count(), 12);
}

#[test]
fn wallet_creates_correct_mainnet_keys() {
  let rpc_server = test_bitcoincore_rpc::spawn();

  CommandBuilder::new("wallet create")
    .rpc_server(&rpc_server)
    .output::<Create>();

  let imported_privkeys = rpc_server.imported_privkeys();
  assert_eq!(imported_privkeys.len(), 40);
  
  // Verify one known key if possible, or just verify they are valid WIFs for mainnet
  for (key, _) in imported_privkeys {
    bitcoin::PrivateKey::from_wif(&key).unwrap();
    assert_eq!(bitcoin::PrivateKey::from_wif(&key).unwrap().network, Network::Bitcoin);
  }
}

#[test]
fn wallet_creates_correct_test_network_keys() {
  let rpc_server = test_bitcoincore_rpc::builder()
    .network(Network::Signet)
    .build();

  CommandBuilder::new("--chain signet wallet create")
    .rpc_server(&rpc_server)
    .output::<Create>();

  let imported_privkeys = rpc_server.imported_privkeys();
  assert_eq!(imported_privkeys.len(), 40);
  
  for (key, _) in imported_privkeys {
    bitcoin::PrivateKey::from_wif(&key).unwrap();
    assert_eq!(bitcoin::PrivateKey::from_wif(&key).unwrap().network, Network::Testnet);
  }
}

#[test]
fn create_with_different_name() {
  let rpc_server = test_bitcoincore_rpc::spawn();

  CommandBuilder::new("--wallet inscription-wallet wallet create")
    .rpc_server(&rpc_server)
    .output::<Create>();

  let imported_privkeys = rpc_server.imported_privkeys();
  assert_eq!(imported_privkeys.len(), 40);
}
