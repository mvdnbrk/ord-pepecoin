use super::*;

#[test]
fn run() {
  let rpc_server = test_bitcoincore_rpc::spawn();

  let port = TcpListener::bind("127.0.0.1:0")
    .unwrap()
    .local_addr()
    .unwrap()
    .port();

  let builder = CommandBuilder::new(format!("server --address 127.0.0.1 --http-port {port}"))
    .rpc_server(&rpc_server);

  let mut command = builder.command();

  let mut child = command.spawn().unwrap();

  for attempt in 0.. {
    if let Ok(response) = reqwest::blocking::get(format!("http://localhost:{port}/status")) {
      if response.status() == 200 {
        assert!(response.text().unwrap().contains("<h1>Status</h1>"));
        break;
      }
    }

    if attempt == 100 {
      panic!("Server did not respond to status check",);
    }

    thread::sleep(Duration::from_millis(50));
  }

  child.kill().unwrap();
}

#[test]
fn inscription_page() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe {
    inscription,
    reveal,
    ..
  } = inscribe(&rpc_server, &test_server);

  test_server.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      r"(?s).*<meta property=og:title content='Inscription 0'>.*
.*<meta property=og:image content='/static/favicon.png'>.*
.*<meta property=twitter:card content=summary>.*
<h1>Inscription 0</h1>
.*<iframe .* src=/preview/{inscription}></iframe>.*
<dl>
  <dt>id</dt>
  <dd class=monospace>{inscription}</dd>
  <dt>address</dt>
  <dd class=monospace>P.*</dd>
  <dt>output value</dt>
  <dd>100000</dd>
  <dt>preview</dt>
  <dd><a href=/preview/{inscription}>link</a></dd>
  <dt>content</dt>
  <dd><a href=/content/{inscription}>link</a></dd>
  <dt>content length</dt>
  <dd>3 bytes</dd>
  <dt>content type</dt>
  <dd>text/plain;charset=utf-8</dd>
  <dt>timestamp</dt>
  <dd><time>1970-01-01 00:00:02 UTC</time></dd>
  <dt>height</dt>
  <dd><a href=/block/2>2</a></dd>
  <dt>fee</dt>
  <dd>2310000</dd>
  <dt>reveal transaction</dt>
  <dd><a class=monospace href=/tx/{reveal}>{reveal}</a></dd>
  <dt>location</dt>
  <dd class=monospace>{reveal}:0:0</dd>
  <dt>output</dt>
  <dd><a class=monospace href=/output/{reveal}:0>{reveal}:0</a></dd>
  <dt>offset</dt>
  <dd>0</dd>
</dl>.*",
    ),
  );
}

#[test]
fn inscription_appears_on_reveal_transaction_page() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe { reveal, .. } = inscribe(&rpc_server, &test_server);

  rpc_server.mine_blocks(1);

  test_server.assert_response_regex(
    format!("/tx/{reveal}"),
    format!(
      r"(?s).*<h1>Transaction .*</h1>.*<a href=/inscription/{}i0>.*",
      reveal
    ),
  );
}

#[test]
fn inscription_appears_on_output_page() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe {
    reveal,
    inscription,
    ..
  } = inscribe(&rpc_server, &test_server);

  rpc_server.mine_blocks(1);

  test_server.assert_response_regex(
    format!("/output/{reveal}:0"),
    format!(".*<h1>Output <span class=monospace>{reveal}:0</span></h1>.*<a href=/inscription/{inscription}.*"),
  );
}

#[test]
fn inscription_page_after_send() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe {
    reveal,
    inscription,
    ..
  } = inscribe(&rpc_server, &test_server);

  rpc_server.mine_blocks(1);

  test_server.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      r".*<h1>Inscription 0</h1>.*<dt>location</dt>\s*<dd class=monospace>{reveal}:0:0</dd>.*",
    ),
  );

  let txid = CommandBuilder::new(format!(
    "wallet send --fee-rate 10000 bc1qcqgs2pps4u4yedfyl5pysdjjncs8et5utseepv {inscription}"
  ))
  .rpc_server(&rpc_server)
  .ord_server(&test_server)
  .data_dir(test_server.directory())
  .stdout_regex(".*")
  .run();

  rpc_server.mine_blocks(1);

  let send = txid.trim();

  test_server.assert_response_regex(
    format!("/inscription/{inscription}"),
    format!(
      r".*<h1>Inscription 0</h1>.*<dt>location</dt>\s*<dd class=monospace>{send}:0:0</dd>.*",
    ),
  )
}

#[test]
fn inscription_content() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  rpc_server.mine_blocks(1);

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &test_server);

  rpc_server.mine_blocks(1);

  let response = test_server.request(format!("/content/{inscription}"));

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response.headers().get("content-type").unwrap(),
    "text/plain;charset=utf-8"
  );
  assert_eq!(
    response.headers().get("content-security-policy").unwrap(),
    "default-src 'self' 'unsafe-eval' 'unsafe-inline' data: blob:"
  );
  assert_eq!(response.bytes().unwrap(), "FOO");
}

#[test]
fn home_page_includes_latest_inscriptions() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &test_server);

  test_server.assert_response_regex(
    "/",
    format!(
      ".*<h2>Latest Inscriptions</h2>
<div class=thumbnails>
  <a href=/inscription/{inscription}><iframe .*></a>
</div>.*",
    ),
  );
}

#[test]
fn home_page_inscriptions_are_sorted() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let mut inscriptions = String::new();

  for _ in 0..8 {
    let Inscribe { inscription, .. } = inscribe(&rpc_server, &test_server);
    inscriptions.insert_str(
      0,
      &format!("\n  <a href=/inscription/{inscription}><iframe .*></a>"),
    );
  }

  test_server.assert_response_regex(
    "/",
    format!(
      ".*<h2>Latest Inscriptions</h2>
<div class=thumbnails>{inscriptions}
</div>.*"
    ),
  );
}

#[test]
fn inscriptions_page() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe { inscription, .. } = inscribe(&rpc_server, &test_server);

  test_server.assert_response_regex(
    "/inscriptions",
    format!(
      ".*<h1>Inscription</h1>
<div class=thumbnails>
  <a href=/inscription/{inscription}>.*</a>
</div>
.*",
    ),
  );
}

#[test]
fn inscriptions_page_is_sorted() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let mut inscriptions = String::new();

  for _ in 0..8 {
    let Inscribe { inscription, .. } = inscribe(&rpc_server, &test_server);
    inscriptions.insert_str(0, &format!(".*<a href=/inscription/{inscription}>.*"));
  }

  test_server.assert_response_regex("/inscriptions", &inscriptions);
}

#[test]
fn inscriptions_page_has_next_and_previous() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let Inscribe { inscription: a, .. } = inscribe(&rpc_server, &test_server);
  let Inscribe { inscription: b, .. } = inscribe(&rpc_server, &test_server);
  let Inscribe { inscription: c, .. } = inscribe(&rpc_server, &test_server);

  test_server.assert_response_regex(
    format!("/inscription/{b}"),
    format!(
      ".*<h1>Inscription 1</h1>.*
<div class=inscription>
<a class=prev href=/inscription/{a}>❮</a>
<iframe .* src=/preview/{b}></iframe>
<a class=next href=/inscription/{c}>❯</a>
</div>.*",
    ),
  );
}

#[test]
fn parent_child() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);
  create_wallet_with_data_dir(&rpc_server, Some(test_server.directory()));

  let parent = inscribe(&rpc_server, &test_server);
  rpc_server.mine_blocks(1);

  let child = CommandBuilder::new(format!(
    "wallet inscribe child.txt --parent {}",
    parent.inscription
  ))
  .write("child.txt", "CHILD")
  .rpc_server(&rpc_server)
  .ord_server(&test_server)
  .data_dir(test_server.directory())
  .output::<Inscribe>();

  rpc_server.mine_blocks(1);

  // Parent inscription location must survive child reveal spending its UTXO
  let parent_api: ord::api::Inscription = test_server
    .json_request(format!("/inscription/{}", parent.inscription))
    .json()
    .unwrap();
  assert!(parent_api.value.unwrap() > 0, "parent UTXO value lost");
  assert_eq!(parent_api.child_count, 1);
  assert_eq!(parent_api.children.len(), 1);

  let child_api: ord::api::Inscription = test_server
    .json_request(format!("/inscription/{}", child.inscription))
    .json()
    .unwrap();
  assert_eq!(child_api.parents.len(), 1);
  assert_eq!(child_api.parents[0].to_string(), parent.inscription);

  // Inscription page links to parent
  test_server.assert_response_regex(
    format!("/inscription/{}", child.inscription),
    format!(
      r"(?s).*<dt>parents</dt>.*<a href=/inscription/{}>.*</a>.*",
      parent.inscription
    ),
  );

  // Inscription page links to children
  test_server.assert_response_regex(
    format!("/inscription/{}", parent.inscription),
    format!(
      r"(?s).*<dt>children</dt>.*<a href=/inscription/{}>.*</a>.*all \(1\).*",
      child.inscription
    ),
  );
}

#[test]
fn expected_sat_time_is_rounded() {
  let rpc_server = test_bitcoincore_rpc::spawn();
  let test_server = TestServer::spawn(&rpc_server);

  test_server.assert_response_regex(
    "/sat/2099999997689999",
    r".*<dt>timestamp</dt><dd><time>.* \d+:\d+:\d+ UTC</time> \(expected\)</dd>.*",
  );
}
