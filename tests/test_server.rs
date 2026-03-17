use {
  super::*,
  axum_server::Handle,
  bitcoincore_rpc::{Auth, Client, RpcApi},
  ord::{Index, parse_ord_server_args},
  reqwest::blocking::Response,
  std::{net::SocketAddr, path::PathBuf, sync::Arc},
};

pub(crate) struct TestServer {
  client: Client,
  ord_server_handle: Handle<SocketAddr>,
  port: u16,
  #[allow(unused)]
  tempdir: TempDir,
}

impl TestServer {
  pub(crate) fn spawn(rpc_server: &test_bitcoincore_rpc::Handle) -> Self {
    Self::spawn_with_args(rpc_server, &[])
  }

  pub(crate) fn spawn_with_args(rpc_server: &test_bitcoincore_rpc::Handle, args: &[&str]) -> Self {
    let tempdir = TempDir::new().unwrap();
    let cookiefile = tempdir.path().join("cookie");
    fs::write(&cookiefile, "username:password").unwrap();

    std::env::set_var("ORD_INTEGRATION_TEST", "1");

    let (settings, server) = parse_ord_server_args(&format!(
      "ordpep --chain {} --rpc-url {} --cookie-file {} --pepecoin-data-dir {} --data-dir {} {} server --http-port 0 --address 127.0.0.1",
      rpc_server.network(),
      rpc_server.url(),
      cookiefile.to_str().unwrap(),
      tempdir.path().display(),
      tempdir.path().display(),
      args.join(" "),
    ));

    let index = Arc::new(Index::open(&settings).unwrap());
    let ord_server_handle = Handle::new();
    let (tx, rx) = std::sync::mpsc::channel();

    {
      let index = index.clone();
      let ord_server_handle = ord_server_handle.clone();
      let settings = settings.clone();
      thread::spawn(move || {
        server.run(settings, index, ord_server_handle, Some(tx)).unwrap()
      });
    }

    let port = rx.recv().unwrap();
    let client = Client::new(&rpc_server.url(), Auth::None).unwrap();

    Self {
      client,
      ord_server_handle,
      port,
      tempdir,
    }
  }

  pub(crate) fn url(&self) -> Url {
    format!("http://127.0.0.1:{}", self.port).parse().unwrap()
  }

  pub(crate) fn directory(&self) -> PathBuf {
    self.tempdir.path().to_owned()
  }

  pub(crate) fn sync_server(&self) {
    let chain_block_count = self.client.get_block_count().unwrap() + 1;
    let response = reqwest::blocking::get(self.url().join("/update").unwrap()).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.text().unwrap().parse::<u64>().unwrap() >= chain_block_count);
  }

  pub(crate) fn assert_response_regex(&self, path: impl AsRef<str>, regex: impl AsRef<str>) {
    self.sync_server();
    let response = reqwest::blocking::get(self.url().join(path.as_ref()).unwrap()).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_regex_match!(response.text().unwrap(), regex.as_ref());
  }

  pub(crate) fn request(&self, path: impl AsRef<str>) -> Response {
    self.sync_server();
    reqwest::blocking::get(self.url().join(path.as_ref()).unwrap()).unwrap()
  }

  pub(crate) fn json_request(&self, path: impl AsRef<str>) -> Response {
    self.sync_server();
    reqwest::blocking::Client::new()
      .get(self.url().join(path.as_ref()).unwrap())
      .header(reqwest::header::ACCEPT, "application/json")
      .send()
      .unwrap()
  }

  pub(crate) fn post_json(&self, path: impl AsRef<str>, body: &impl serde::Serialize) -> Response {
    self.sync_server();
    reqwest::blocking::Client::new()
      .post(self.url().join(path.as_ref()).unwrap())
      .json(body)
      .send()
      .unwrap()
  }
}

impl Drop for TestServer {
  fn drop(&mut self) {
    self.ord_server_handle.shutdown();
  }
}
