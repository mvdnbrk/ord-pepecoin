use super::*;

#[test]
fn inscribe_creates_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  assert_eq!(rpc_server.descriptors().len(), 0);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &ord_server);

  assert_eq!(rpc_server.descriptors().len(), 0);

  let request = ord_server.request(format!("/content/{inscription}"));

  assert_eq!(request.status(), 200);
  assert_eq!(
    request.headers().get("content-type").unwrap(),
    "text/plain;charset=utf-8"
  );
  assert_eq!(request.text().unwrap(), "FOO");
}

#[test]
fn inscribe_works_with_huge_expensive_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  let txid = rpc_server.mine_blocks(1)[0].txdata[0].txid();

  CommandBuilder::new(format!(
    "wallet inscribe foo.txt --satpoint {txid}:0:0 --fee-rate 10000"
  ))
  .write("foo.txt", [0; 350_000])
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .output::<Inscribe>();
}

#[test]
fn inscribe_fails_if_pepecoin_core_is_too_old() {
  let rpc_server = test_bitcoincore_rpc::builder().version(1140500).build();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  CommandBuilder::new("wallet inscribe hello.txt")
    .write("hello.txt", "HELLOWORLD")
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: wallet contains no cardinal utxos\n")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .run();
}

#[test]
fn inscribe_no_backup() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe hello.txt --no-backup")
    .write("hello.txt", "HELLOWORLD")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();
}

#[test]
fn inscribe_unknown_file_extension() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe pepe.xyz")
    .write("pepe.xyz", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex(r"error: unsupported file extension `\.xyz`, supported extensions: apng .*\n")
    .run();
}

#[test]
fn inscribe_exceeds_chain_limit() {
  let rpc_server = test_bitcoincore_rpc::builder()
    .network(Network::Signet)
    .build();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("--chain signet wallet inscribe degenerate.png")
    .write("degenerate.png", [1; 1025])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex(
      "error: content size of 1025 bytes exceeds 1024 byte limit for signet inscriptions\n",
    )
    .run();
}

#[test]
fn regtest_has_no_content_size_limit() {
  let rpc_server = test_bitcoincore_rpc::builder()
    .network(Network::Regtest)
    .build();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("--chain regtest wallet inscribe degenerate.png")
    .write("degenerate.png", [1; 1025])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .stdout_regex(".*")
    .run();
}

#[test]
fn mainnet_has_no_content_size_limit() {
  let rpc_server = test_bitcoincore_rpc::builder()
    .network(Network::Bitcoin)
    .build();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe degenerate.png")
    .write("degenerate.png", [1; 1025])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .stdout_regex(".*")
    .run();
}

#[test]
fn inscribe_does_not_use_inscribed_sats_as_cardinal_utxos() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks_with_subsidy(1, 100);

  CommandBuilder::new(
    "wallet inscribe degenerate.png"
  )
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .write("degenerate.png", [1; 100])
  .expected_exit_code(1)
  .stderr_regex("error: not enough cardinal UTXOs: need .* sat \\(.* PEP\\) but only .* sat \\(.* PEP\\) available\n")
  .run();
}

#[test]
fn refuse_to_reinscribe_sats() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  rpc_server.mine_blocks(1);

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &ord_server);

  rpc_server.mine_blocks_with_subsidy(1, 100);

  CommandBuilder::new(format!("wallet inscribe --satpoint {reveal}:0:0 hello.txt"))
    .write("hello.txt", "HELLOWORLD")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: sat at [[:xdigit:]]{64}:0:0 already inscribed\n")
    .run();
}

#[test]
fn refuse_to_inscribe_already_inscribed_utxo() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &ord_server);

  let output = OutPoint {
    txid: reveal,
    vout: 0,
  };

  CommandBuilder::new(format!(
    "wallet inscribe --satpoint {output}:55555 hello.txt"
  ))
  .write("hello.txt", "HELLOWORLD")
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .expected_exit_code(1)
  .stderr_regex("error: utxo [[:xdigit:]]{64}:0 already inscribed with inscription [[:xdigit:]]{64}i0 on sat [[:xdigit:]]{64}:0:0\n")
  .run();
}

#[test]
fn inscribe_with_optional_satpoint_arg() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn_with_args(&rpc_server, &["--index-sats"]);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  let txid = rpc_server.mine_blocks(1)[0].txdata[0].txid();

  let Inscribe { inscription, .. } =
    CommandBuilder::new(format!("wallet inscribe foo.txt --satpoint {txid}:0:0"))
      .write("foo.txt", "FOO")
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .data_dir(ord_server.directory())
      .output();

  rpc_server.mine_blocks(1);

  ord_server.assert_response_regex(
    "/sat/100000000000000",
    format!(".*<a href=/inscription/{inscription}>.*"),
  );

  ord_server.assert_response_regex(format!("/content/{inscription}",), "FOO");
}

#[test]
fn inscribe_with_fee_rate() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("--index-sats wallet inscribe degenerate.png --fee-rate 20000.0")
    .write("degenerate.png", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();

  let tx1 = &rpc_server.mempool()[0];
  let mut fee = 0;
  for input in &tx1.input {
    fee += rpc_server
      .get_utxo_amount(&input.previous_output)
      .unwrap()
      .to_sat();
  }
  for output in &tx1.output {
    fee -= output.value;
  }

  let fee_rate = fee as f64 / tx1.vsize() as f64;

  assert!(fee_rate >= 1000.0);

  let tx2 = &rpc_server.mempool()[1];
  let mut fee = 0;
  for input in &tx2.input {
    fee += &tx1.output[input.previous_output.vout as usize].value;
  }
  for output in &tx2.output {
    fee -= output.value;
  }

  let fee_rate = fee as f64 / tx2.vsize() as f64;

  assert!(fee_rate >= 1000.0);
}

#[test]
fn inscribe_with_commit_fee_rate() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("--index-sats wallet inscribe degenerate.png --commit-fee-rate 20000.0")
    .write("degenerate.png", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();

  let tx1 = &rpc_server.mempool()[0];
  let mut fee = 0;
  for input in &tx1.input {
    fee += rpc_server
      .get_utxo_amount(&input.previous_output)
      .unwrap()
      .to_sat();
  }
  for output in &tx1.output {
    fee -= output.value;
  }

  let fee_rate = fee as f64 / tx1.vsize() as f64;

  assert!(fee_rate >= 1000.0);

  let tx2 = &rpc_server.mempool()[1];
  let mut fee = 0;
  for input in &tx2.input {
    fee += &tx1.output[input.previous_output.vout as usize].value;
  }
  for output in &tx2.output {
    fee -= output.value;
  }

  let fee_rate = fee as f64 / tx2.vsize() as f64;

  assert!(fee_rate >= 500.0);
}

#[test]
fn inscribe_with_wallet_named_foo() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_options(&rpc_server, Some(ord_server.directory()), Some("foo"));

  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe degenerate.png")
    .wallet("foo")
    .write("degenerate.png", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();
}

#[test]
fn inscribe_with_dry_run_flag() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --dry-run degenerate.png")
    .write("degenerate.png", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();

  assert!(rpc_server.mempool().is_empty());

  CommandBuilder::new("wallet inscribe degenerate.png")
    .write("degenerate.png", [1; 520])
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();

  assert_eq!(rpc_server.mempool().len(), 2);
}

#[test]
fn inscribe_with_dry_run_flag_fees_inscrease() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let total_fee_dry_run =
    CommandBuilder::new("wallet inscribe --dry-run degenerate.png --fee-rate 10000.0")
      .write("degenerate.png", [1; 520])
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .data_dir(ord_server.directory())
      .output::<Inscribe>()
      .fees;

  let total_fee_normal =
    CommandBuilder::new("wallet inscribe --dry-run degenerate.png --fee-rate 50000.0")
      .write("degenerate.png", [1; 520])
      .rpc_server(&rpc_server)
      .ord_server(&ord_server)
      .data_dir(ord_server.directory())
      .output::<Inscribe>()
      .fees;

  assert!(total_fee_dry_run < total_fee_normal);
}

#[test]
fn inscribe_to_specific_destination() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let destination = CommandBuilder::new("wallet receive")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<ord::subcommand::wallet::receive::Output>()
    .address;

  let txid = CommandBuilder::new(format!(
    "wallet inscribe --destination {destination} degenerate.png"
  ))
  .write("degenerate.png", [1; 520])
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .output::<Inscribe>()
  .reveal;

  let reveal_tx = &rpc_server.mempool()[1]; // item 0 is the commit, item 1 is the reveal.
  assert_eq!(reveal_tx.txid(), txid);
  assert_eq!(
    reveal_tx.output.first().unwrap().script_pubkey,
    destination.script_pubkey()
  );
}

#[test]
fn inscribe_with_no_limit() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks_with_subsidy(1, 100_000_000_000_000);

  let four_megger = std::iter::repeat(0).take(4_000_000).collect::<Vec<u8>>();
  CommandBuilder::new("wallet inscribe --no-limit --fee-rate 10000.0 degenerate.png")
    .write("degenerate.png", four_megger)
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<Inscribe>();
}

#[test]
fn batch_inscribe_creates_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  // Create batch YAML with 2 files
  let output = CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write(
      "batch.yaml",
      "inscriptions:\n  - file: foo.txt\n  - file: bar.txt\n",
    )
    .write("foo.txt", "FOO")
    .write("bar.txt", "BAR")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<BatchInscribe>();

  assert_eq!(output.inscriptions.len(), 2);

  rpc_server.mine_blocks(1);

  // Verify both inscriptions are indexed and have correct content
  let response1 = ord_server.request(format!("/content/{}", output.inscriptions[0].inscription));
  assert_eq!(response1.status(), 200);
  assert_eq!(response1.text().unwrap(), "FOO");

  let response2 = ord_server.request(format!("/content/{}", output.inscriptions[1].inscription));
  assert_eq!(response2.status(), 200);
  assert_eq!(response2.text().unwrap(), "BAR");
}

#[test]
fn batch_inscribe_with_destinations() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let destination = CommandBuilder::new("wallet receive")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<ord::subcommand::wallet::receive::Output>()
    .address;

  let batch_yaml = format!(
    "inscriptions:\n  - file: foo.txt\n    destination: \"{destination}\"\n  - file: bar.txt\n"
  );

  let output = CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write("batch.yaml", batch_yaml)
    .write("foo.txt", "FOO")
    .write("bar.txt", "BAR")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<BatchInscribe>();

  // First inscription should go to specified destination
  assert_eq!(output.inscriptions[0].destination, destination.to_string());
}

#[test]
fn batch_inscribe_with_dry_run() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --batch batch.yaml --dry-run")
    .write(
      "batch.yaml",
      "inscriptions:\n  - file: foo.txt\n  - file: bar.txt\n",
    )
    .write("foo.txt", "FOO")
    .write("bar.txt", "BAR")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<BatchInscribe>();

  // No transactions should be broadcast
  assert!(rpc_server.mempool().is_empty());
}

#[test]
fn batch_inscribe_refuses_empty_batch() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write("batch.yaml", "inscriptions: []\n")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: batch file contains no inscriptions\n")
    .run();
}

#[test]
fn batch_inscribe_file_not_found() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write("batch.yaml", "inscriptions:\n  - file: nonexistent.txt\n")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: file not found: .*nonexistent.txt\n")
    .run();
}

#[test]
fn inscribe_with_parent() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  // Inscribe the parent
  let parent = inscribe(&rpc_server, &ord_server);

  // Need another block with funds for the child inscription
  rpc_server.mine_blocks(1);

  // Inscribe a child with --parent
  let child = CommandBuilder::new(format!(
    "wallet inscribe child.txt --parent {}",
    parent.inscription
  ))
  .write("child.txt", "CHILD")
  .rpc_server(&rpc_server)
  .ord_server(&ord_server)
  .data_dir(ord_server.directory())
  .output::<Inscribe>();

  rpc_server.mine_blocks(1);

  // Verify child content is indexed
  let response = ord_server.request(format!("/content/{}", child.inscription));
  assert_eq!(response.status(), 200);
  assert_eq!(response.text().unwrap(), "CHILD");
}

#[test]
fn inscribe_with_invalid_parent() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let fake_parent = "0000000000000000000000000000000000000000000000000000000000000000i0";

  CommandBuilder::new(format!("wallet inscribe child.txt --parent {fake_parent}"))
    .write("child.txt", "CHILD")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: parent inscription .*i0 not found in wallet\n")
    .run();
}

#[test]
fn batch_and_file_conflict() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);

  CommandBuilder::new("wallet inscribe --batch batch.yaml foo.txt")
    .write("batch.yaml", "inscriptions:\n  - file: foo.txt\n")
    .write("foo.txt", "FOO")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(2)  // clap argument conflict
    .stderr_regex(".*cannot be used with.*")
    .run();
}

#[test]
fn batch_inscribe_with_parent() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));

  let parent = inscribe(&rpc_server, &ord_server);
  rpc_server.mine_blocks(1);

  let batch_yaml = format!(
    "parents:\n  - {}\ninscriptions:\n  - file: child1.txt\n  - file: child2.txt\n",
    parent.inscription
  );

  let output = CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write("batch.yaml", &batch_yaml)
    .write("child1.txt", "CHILD1")
    .write("child2.txt", "CHILD2")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .output::<BatchInscribe>();

  assert_eq!(output.inscriptions.len(), 2);

  rpc_server.mine_blocks(1);

  let response1 = ord_server.request(format!("/content/{}", output.inscriptions[0].inscription));
  assert_eq!(response1.status(), 200);
  assert_eq!(response1.text().unwrap(), "CHILD1");

  let response2 = ord_server.request(format!("/content/{}", output.inscriptions[1].inscription));
  assert_eq!(response2.status(), 200);
  assert_eq!(response2.text().unwrap(), "CHILD2");
}

#[test]
fn batch_inscribe_with_invalid_parent() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let ord_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(ord_server.directory()));
  rpc_server.mine_blocks(1);

  let fake_parent = "0000000000000000000000000000000000000000000000000000000000000000i0";

  CommandBuilder::new("wallet inscribe --batch batch.yaml")
    .write(
      "batch.yaml",
      &format!(
        "parents:\n  - {}\ninscriptions:\n  - file: child.txt\n",
        fake_parent
      ),
    )
    .write("child.txt", "CHILD")
    .rpc_server(&rpc_server)
    .ord_server(&ord_server)
    .data_dir(ord_server.directory())
    .expected_exit_code(1)
    .stderr_regex("error: parent inscription .*i0 not found in wallet\n")
    .run();
}
