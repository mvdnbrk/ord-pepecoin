use {super::*, ord::subcommand::wallet::receive::Output};

#[test]
fn receive() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let output = CommandBuilder::new("wallet receive")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .output::<Output>();

  assert!(output.address.is_valid_for_network(Network::Bitcoin));
}
