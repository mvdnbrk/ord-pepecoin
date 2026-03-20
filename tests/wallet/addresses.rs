use {super::*, std::collections::BTreeMap};

#[derive(Deserialize, Debug)]
struct AddressesOutput {
  pub output: OutPoint,
  pub amount: u64,
  pub inscriptions: Vec<String>,
}

#[test]
fn addresses() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let coinbase_tx = &rpc_server.mine_blocks_with_subsidy(1, 1_000_000)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.txid(), 0);
  let amount = coinbase_tx.output[0].value;
  let address =
    Address::from_script(&coinbase_tx.output[0].script_pubkey, Network::Bitcoin).unwrap();

  let addresses = CommandBuilder::new("wallet addresses")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<BTreeMap<String, Vec<AddressesOutput>>>();

  assert_eq!(addresses.len(), 1);
  assert_eq!(
    addresses.get(&address.to_string()).unwrap()[0].output,
    outpoint
  );
  assert_eq!(
    addresses.get(&address.to_string()).unwrap()[0].amount,
    amount
  );
  assert!(addresses.get(&address.to_string()).unwrap()[0]
    .inscriptions
    .is_empty());

  let inscribe = inscribe(&rpc_server, &ord_server);

  let addresses = CommandBuilder::new("wallet addresses")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<BTreeMap<String, Vec<AddressesOutput>>>();

  let inscription_id = inscribe.inscription;

  let mut found = false;
  for outputs in addresses.values() {
    for output in outputs {
      if output.inscriptions.contains(&inscription_id) {
        found = true;
        break;
      }
    }
  }
  assert!(found);
}
