use {super::*, ord::subcommand::wallet::send::Output};

#[test]
fn inscriptions_can_be_sent() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  let stdout = CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription}",
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .stdout_regex(r".*")
  .run();

  let txid = rpc_server.mempool()[0].txid();
  assert_eq!(format!("{txid}\n"), stdout);

  rpc_server.mine_blocks(1);

  let send_txid = stdout.trim();

  ord_server.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      ".*<h1>Inscription 0</h1>.*<dl>.*
  <dt>content length</dt>
  <dd>3 bytes</dd>
  <dt>content type</dt>
  <dd>text/plain;charset=utf-8</dd>
  .*
  <dt>location</dt>
  <dd class=monospace>{send_txid}:0:0</dd>
  .*
</dl>
.*",
    ),
  );
}

#[test]
fn send_unknown_inscription() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let txid = rpc_server.mine_blocks(1)[0].txdata[0].txid();

  CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {txid}i0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_stderr(format!("error: Inscription {txid}i0 not found\n"))
  .expected_exit_code(1)
  .run();
}

#[test]
fn send_inscribed_sat() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  let stdout = CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {inscription}",
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .stdout_regex("[[:xdigit:]]{64}\n")
  .run();

  rpc_server.mine_blocks(1);

  let send_txid = stdout.trim();

  ord_server.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      ".*<h1>Inscription 0</h1>.*<dt>location</dt>.*<dd class=monospace>{send_txid}:0:0</dd>.*",
    ),
  );
}

#[test]
fn send_on_mainnnet_works_with_wallet_named_foo() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_options(&rpc_server, Some(ord_server.directory()), Some("foo"));
  let txid = rpc_server.mine_blocks(1)[0].txdata[0].txid();

  CommandBuilder::new(format!(
    "wallet --name foo send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .stdout_regex(r"[[:xdigit:]]{64}\n")
  .run();
}

#[test]
fn send_addresses_must_be_valid_for_network() {
  let rpc_server = test_bitcoincore_rpc::builder().build();
  let ord_server = TestServer::spawn(&rpc_server);
  let txid = rpc_server.mine_blocks_with_subsidy(1, 1_000)[0].txdata[0].txid();
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 tb1q6en7qjxgw4ev8xwx94pzdry6a6ky7wlfeqzunz {txid}:0:0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_stderr(
    "error: Address `tb1q6en7qjxgw4ev8xwx94pzdry6a6ky7wlfeqzunz` is not valid for mainnet\n",
  )
  .expected_exit_code(1)
  .run();
}

#[test]
fn send_on_mainnnet_works_with_wallet_named_ord() {
  let rpc_server = test_bitcoincore_rpc::builder().build();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  let txid = rpc_server.mine_blocks_with_subsidy(1, 10_000_000)[0].txdata[0].txid();

  let stdout = CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .stdout_regex(r".*")
  .run();

  let txid = rpc_server.mempool()[0].txid();
  assert_eq!(format!("{txid}\n"), stdout);
}

#[test]
fn send_does_not_use_inscribed_sats_as_cardinal_utxos() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  // Only a tiny UTXO that can't cover fees at 10000 sat/vB
  let txid = rpc_server.mine_blocks_with_subsidy(1, 1_000)[0].txdata[0].txid();
  CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {txid}:0:0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_exit_code(1)
  .expected_stderr("error: wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.\n")
  .run();
}

#[test]
fn do_not_accidentally_send_an_inscription() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let Inscribe {
    reveal,
    inscription,
    ..
  } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  let output = OutPoint {
    txid: reveal,
    vout: 0,
  };

  CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {output}:55"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_exit_code(1)
  .expected_stderr(format!(
    "error: cannot send {output}:55 without also sending inscription {inscription} at {output}:0\n"
  ))
  .run();
}

#[test]
fn inscriptions_cannot_be_sent_by_satpoint() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks(1);

  CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {reveal}:0:0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_stderr("error: inscriptions must be sent by inscription ID\n")
  .expected_exit_code(1)
  .run();
}

#[test]
fn send_btc() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks(1);

  let output =
    CommandBuilder::new("wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>();

  // Transaction should be in the mempool
  let mempool = rpc_server.mempool();
  assert_eq!(mempool.len(), 1);
  assert_eq!(output.transaction, mempool[0].txid());

  // Destination should receive 1 BTC
  let dest_script = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
    .parse::<Address>()
    .unwrap()
    .script_pubkey();
  let dest_output = mempool[0].output.iter().find(|o| o.script_pubkey == dest_script).unwrap();
  assert_eq!(dest_output.value, 100_000_000);
}

#[test]
fn send_btc_locks_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks(1);

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &ord_server);

  let output =
    CommandBuilder::new("wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>();

  // Transaction should be in the mempool
  let mempool = rpc_server.mempool();
  assert_eq!(mempool.len(), 1);
  assert_eq!(output.transaction, mempool[0].txid());

  // The inscription UTXO should NOT be spent as an input
  let inscribed_outpoint = OutPoint { txid: reveal, vout: 0 };
  assert!(
    !mempool[0].input.iter().any(|i| i.previous_output == inscribed_outpoint),
    "inscription UTXO should not be spent when sending BTC"
  );
}

#[test]
fn send_btc_insufficient_funds() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  // Mine a small amount — not enough to send 1 BTC
  rpc_server.mine_blocks_with_subsidy(1, 100);

  CommandBuilder::new("wallet send --fee-rate 10000 bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 1btc")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .expected_stderr("error: wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.\n")
    .expected_exit_code(1)
    .run();
}

#[test]
fn wallet_send_with_fee_rate() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &ord_server);

  CommandBuilder::new(format!(
    "wallet send bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription} --fee-rate 20000.0"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .stdout_regex("[[:xdigit:]]{64}\n")
  .run();

  let tx = &rpc_server.mempool()[0];
  let mut fee = 0;
  for input in &tx.input {
    fee += rpc_server
      .get_utxo_amount(&input.previous_output)
      .unwrap()
      .to_sat();
  }
  for output in &tx.output {
    fee -= output.value;
  }

  let fee_rate = fee as f64 / tx.vsize() as f64;

  assert!(fee_rate >= 20000.0, "fee rate {fee_rate} should be at least 20000");
}

#[test]
fn user_must_provide_fee_rate_to_send() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &ord_server);

  CommandBuilder::new(format!(
    "wallet send bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4 {inscription}"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .expected_exit_code(0)
  .stdout_regex("[[:xdigit:]]{64}\n")
  .run();
}

#[test]
fn send_max_sends_all_cardinal_utxos() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks(3);

  let output =
    CommandBuilder::new("wallet send --fee-rate 10000 --max bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>();

  let mempool = rpc_server.mempool();
  assert_eq!(mempool.len(), 1);
  assert_eq!(output.transaction, mempool[0].txid());

  // Should have exactly one output (no change)
  assert_eq!(mempool[0].output.len(), 1, "send --max should have no change output");

  // All 3 mined UTXOs should be inputs
  assert_eq!(mempool[0].input.len(), 3, "send --max should spend all cardinal UTXOs");
}

#[test]
fn send_max_does_not_spend_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks(1);

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &ord_server);

  // Mine another block to give us a cardinal UTXO
  rpc_server.mine_blocks(1);

  let output =
    CommandBuilder::new("wallet send --fee-rate 10000 --max bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .output::<Output>();

  let mempool = rpc_server.mempool();
  assert_eq!(mempool.len(), 1);
  assert_eq!(output.transaction, mempool[0].txid());

  // The inscription UTXO should NOT be spent
  let inscribed_outpoint = OutPoint { txid: reveal, vout: 0 };
  assert!(
    !mempool[0].input.iter().any(|i| i.previous_output == inscribed_outpoint),
    "inscription UTXO must not be spent when using --max"
  );
}

#[test]
fn send_max_with_no_utxos() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  // No UTXOs in wallet at all
  CommandBuilder::new("wallet send --fee-rate 10000 --max bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .expected_stderr("error: wallet contains no cardinal UTXOs to send\n")
    .expected_exit_code(1)
    .run();
}
