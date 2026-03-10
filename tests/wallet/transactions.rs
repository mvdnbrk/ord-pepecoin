use {super::*, ord::subcommand::wallet::transactions::Output};

#[test]
fn transactions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  rpc_server.mine_blocks(1);

  let Inscribe { commit, reveal, .. } = inscribe(&rpc_server);

  rpc_server.mine_blocks(1);

  let output = CommandBuilder::new("wallet transactions")
    .rpc_server(&rpc_server)
    .output::<Vec<Output>>();

  assert!(output.iter().any(|tx| tx.transaction == reveal));
  assert!(output.iter().any(|tx| tx.transaction == commit));
}

#[test]
fn transactions_with_limit() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  rpc_server.mine_blocks(1);

  let Inscribe { commit, reveal, .. } = inscribe(&rpc_server);

  rpc_server.mine_blocks(1);

  let output = CommandBuilder::new("wallet transactions")
    .rpc_server(&rpc_server)
    .output::<Vec<Output>>();

  assert!(output.len() >= 2);
  assert!(output.iter().any(|tx| tx.transaction == reveal));
  assert!(output.iter().any(|tx| tx.transaction == commit));

  let output = CommandBuilder::new("wallet transactions --limit 1")
    .rpc_server(&rpc_server)
    .output::<Vec<Output>>();

  assert_eq!(output.len(), 1);
  assert!(output[0].transaction == reveal || output[0].transaction == commit);
}
