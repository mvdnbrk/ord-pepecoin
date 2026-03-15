use {super::*, ord::subcommand::wallet::outputs::Output};

#[test]
fn outputs() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let coinbase_tx = &rpc_server.mine_blocks_with_subsidy(1, 1_000_000)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.txid(), 0);
  let amount = coinbase_tx.output[0].value;

  let output = CommandBuilder::new("wallet outputs")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<Vec<Output>>();

  assert_eq!(output[0].output, outpoint);
  assert_eq!(output[0].amount, amount);
}

#[test]
fn outputs_includes_locked_outputs() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let coinbase_tx = &rpc_server.mine_blocks_with_subsidy(1, 1_000_000)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.txid(), 0);
  let amount = coinbase_tx.output[0].value;

  rpc_server.lock(outpoint);

  let output = CommandBuilder::new("wallet outputs")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<Vec<Output>>();

  assert_eq!(output[0].output, outpoint);
  assert_eq!(output[0].amount, amount);
}

#[test]
fn outputs_with_ranges() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn_with_args(&rpc_server, &["--index-sats"]);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let coinbase_tx = &rpc_server.mine_blocks_with_subsidy(1, 1_000_000)[0].txdata[0];
  let outpoint = OutPoint::new(coinbase_tx.txid(), 0);
  let amount = coinbase_tx.output[0].value;

  let output = CommandBuilder::new("wallet outputs --ranges")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<Vec<Output>>();

  assert_eq!(output[0].output, outpoint);
  assert_eq!(output[0].amount, amount);
  assert_eq!(output[0].sat_ranges, Some(vec!["100000000000000-100000001000000".to_string()]));
}
