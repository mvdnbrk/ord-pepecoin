use {
  anyhow::{anyhow, Result},
  bitcoin::{Transaction, Txid},
  bitcoincore_rpc::Auth,
  hyper::{client::HttpConnector, Body, Client, Method, Request, Uri},
  serde::Deserialize,
  serde_json::{json, Value},
  std::time::Duration,
  tokio::time::timeout,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RETRIES: usize = 5;

pub(crate) struct Fetcher {
  client: Client<HttpConnector>,
  url: Uri,
  auth: String,
}

#[derive(Deserialize, Debug)]
struct JsonResponse<T> {
  result: Option<T>,
  error: Option<JsonError>,
  id: usize,
}

#[derive(Deserialize, Debug)]
struct JsonError {
  code: i32,
  message: String,
}

impl Fetcher {
  pub(crate) fn new(url: &str, auth: Auth) -> Result<Self> {
    if auth == Auth::None {
      return Err(anyhow!("No rpc authentication provided"));
    }

    let client = Client::new();

    let url = if url.starts_with("http://") {
      url.to_string()
    } else {
      "http://".to_string() + url
    };

    let url = Uri::try_from(&url).map_err(|e| anyhow!("Invalid rpc url {url}: {e}"))?;

    let (user, password) = auth.get_user_pass()?;
    let auth = format!("{}:{}", user.unwrap(), password.unwrap());
    let auth = format!("Basic {}", &base64::encode(auth));
    Ok(Fetcher { client, url, auth })
  }

  pub(crate) async fn get_transactions(&self, txids: Vec<Txid>) -> Result<Vec<Transaction>> {
    if txids.is_empty() {
      return Ok(Vec::new());
    }

    let mut reqs = Vec::with_capacity(txids.len());
    for (i, txid) in txids.iter().enumerate() {
      let req = json!({
        "jsonrpc": "2.0",
        "id": i, // Use the index as id, so we can quickly sort the response
        "method": "getrawtransaction",
        "params": [ txid ]
      });
      reqs.push(req);
    }

    let body = Value::Array(reqs).to_string();

    for attempt in 0..MAX_RETRIES {
      let req = Request::builder()
        .method(Method::POST)
        .uri(&self.url)
        .header(hyper::header::AUTHORIZATION, &self.auth)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.clone()))?;

      let result = timeout(REQUEST_TIMEOUT, async {
        let response = self.client.request(req).await?;
        hyper::body::to_bytes(response).await.map_err(|e| anyhow!(e))
      })
      .await;

      let buf = match result {
        Ok(Ok(buf)) => buf,
        Ok(Err(e)) => {
          log::warn!("RPC request failed (attempt {}/{}): {e}", attempt + 1, MAX_RETRIES);
          if attempt + 1 < MAX_RETRIES {
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt as u32))).await;
            continue;
          }
          return Err(anyhow!("RPC request failed after {MAX_RETRIES} attempts: {e}"));
        }
        Err(_) => {
          log::warn!("RPC request timed out (attempt {}/{})", attempt + 1, MAX_RETRIES);
          if attempt + 1 < MAX_RETRIES {
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt as u32))).await;
            continue;
          }
          return Err(anyhow!("RPC request timed out after {MAX_RETRIES} attempts"));
        }
      };

      let mut results: Vec<JsonResponse<String>> = serde_json::from_slice(&buf)?;

      // Return early on any error, because we need all results to proceed
      if let Some(err) = results.iter().find_map(|res| res.error.as_ref()) {
        return Err(anyhow!(
          "Failed to fetch raw transaction: code {} message {}",
          err.code,
          err.message
        ));
      }

      // Results from batched JSON-RPC requests can come back in any order, so we must sort them by id
      results.sort_by(|a, b| a.id.cmp(&b.id));

      let txs = results
        .into_iter()
        .map(|res| {
          res
            .result
            .ok_or_else(|| anyhow!("Missing result for batched JSON-RPC response"))
            .and_then(|str| {
              hex::decode(str)
                .map_err(|e| anyhow!("Result for batched JSON-RPC response not valid hex: {e}"))
            })
            .and_then(|hex| {
              bitcoin::consensus::deserialize(&hex).map_err(|e| {
                anyhow!("Result for batched JSON-RPC response not valid pepecoin tx: {e}")
              })
            })
        })
        .collect::<Result<Vec<Transaction>>>()?;
      return Ok(txs);
    }

    unreachable!()
  }
}
