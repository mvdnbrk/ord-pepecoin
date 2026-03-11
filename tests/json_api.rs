use super::*;

#[test]
fn status() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let response = test_server.json_request("/status");
  assert_eq!(response.status(), StatusCode::OK);

  let status: ord::api::Status = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(status.chain, rpc_server.network());
  assert!(status.height.is_some());
  pretty_assert_eq!(status.inscriptions, 0);
}

#[test]
fn inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  let inscribe = inscribe(&rpc_server);
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let response = test_server.json_request("/inscriptions");
  assert_eq!(response.status(), StatusCode::OK);

  let inscriptions: ord::api::Inscriptions = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(inscriptions.ids, vec![inscribe.inscription.parse().unwrap()]);
  assert!(!inscriptions.more);
  pretty_assert_eq!(inscriptions.page_index, 0);
}

#[test]
fn inscription() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  let inscribe = inscribe(&rpc_server);
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let response = test_server.json_request(format!("/inscription/{}", inscribe.inscription));
  assert_eq!(response.status(), StatusCode::OK);

  let inscription: ord::api::Inscription = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(inscription.id, inscribe.inscription.parse().unwrap());
  pretty_assert_eq!(inscription.number, 0);
  pretty_assert_eq!(inscription.height, 2);
}

#[test]
fn inscriptions_batch() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  let inscribe = inscribe(&rpc_server);
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let response = test_server.post_json("/inscriptions", &vec![inscribe.inscription.clone()]);
  assert_eq!(response.status(), StatusCode::OK);

  let inscriptions: Vec<ord::api::Inscription> = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(inscriptions.len(), 1);
  pretty_assert_eq!(inscriptions[0].id, inscribe.inscription.parse().unwrap());
}

#[test]
fn output() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  let inscribe = inscribe(&rpc_server);
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let outpoint = OutPoint::new(inscribe.reveal, 0);

  let response = test_server.json_request(format!("/output/{outpoint}"));
  assert_eq!(response.status(), StatusCode::OK);

  let output: ord::api::Output = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(output.outpoint, outpoint);
  pretty_assert_eq!(output.value, 100000);
}

#[test]
fn outputs_batch() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  create_wallet(&rpc_server);
  let inscribe = inscribe(&rpc_server);
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let outpoint = OutPoint::new(inscribe.reveal, 0);

  let response = test_server.post_json("/outputs", &vec![outpoint.to_string()]);
  assert_eq!(response.status(), StatusCode::OK);

  let outputs: Vec<ord::api::Output> = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(outputs.len(), 1);
  pretty_assert_eq!(outputs[0].outpoint, outpoint);
}

#[test]
fn outputs_batch_unknown() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let unknown_outpoint = "0000000000000000000000000000000000000000000000000000000000000000:0";
  let response = test_server.post_json("/outputs", &vec![unknown_outpoint]);
  assert_eq!(response.status(), StatusCode::OK);

  let outputs: Vec<ord::api::Output> = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(outputs.len(), 1);
  pretty_assert_eq!(outputs[0].outpoint, unknown_outpoint.parse().unwrap());
  assert!(!outputs[0].indexed);
}

#[test]
fn block() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn_with_args(&rpc_server, &[]);

  let blocks = rpc_server.mine_blocks(1);
  let hash = blocks[0].block_hash();

  let response = test_server.json_request("/block/1");
  assert_eq!(response.status(), StatusCode::OK);

  let block: ord::api::Block = serde_json::from_str(&response.text().unwrap()).unwrap();
  pretty_assert_eq!(block.hash, hash);
  pretty_assert_eq!(block.height, 1);
}
