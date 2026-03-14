use {super::*, ord::subcommand::wallet::balance::Output};

#[test]
fn wallet_balance() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>()
      .cardinal,
    0
  );

  rpc_server.mine_blocks(1);

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>()
      .cardinal,
    50 * COIN_VALUE
  );
}

#[test]
fn wallet_balance_only_counts_cardinal_utxos() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>()
      .cardinal,
    0
  );

  inscribe(&rpc_server, &ord_server);

  assert_eq!(
    CommandBuilder::new("wallet balance")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>()
      .cardinal,
    100 * COIN_VALUE - 100_000
  );
}
