use {super::*, ord::subcommand::wallet::transactions::Output};

#[test]
fn transactions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { commit, reveal, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  let output = CommandBuilder::new("wallet transactions")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Vec<Output>>();

  assert!(output.iter().any(|tx| tx.transaction == reveal));
  assert!(output.iter().any(|tx| tx.transaction == commit));
}

#[test]
fn transactions_with_limit() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { commit, reveal, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  let output = CommandBuilder::new("wallet transactions")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Vec<Output>>();

  assert!(output.len() >= 2);
  assert!(output.iter().any(|tx| tx.transaction == reveal));
  assert!(output.iter().any(|tx| tx.transaction == commit));

  let output = CommandBuilder::new("wallet transactions --limit 1")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Vec<Output>>();

  assert_eq!(output.len(), 1);
  assert!(output[0].transaction == reveal || output[0].transaction == commit);
}
