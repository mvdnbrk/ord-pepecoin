use {
  super::*,
  bitcoin::{
    blockdata::script::Script,
    Txid,
  },
  std::{
    collections::BTreeMap,
    sync::Arc,
  },
  reqwest::blocking::Client as OrdClient,
};

#[derive(Clone, Debug)]
pub(crate) struct Wallet {
  bitcoin_client: Arc<Client>,
  ord_client: OrdClient,
  rpc_url: Url,
  has_sat_index: bool,
  utxos: BTreeMap<OutPoint, TxOut>,
  inscriptions: BTreeMap<SatPoint, Vec<InscriptionId>>,
  inscription_info: BTreeMap<InscriptionId, api::Inscription>,
  output_info: BTreeMap<OutPoint, api::Output>,
  locked_utxos: BTreeMap<OutPoint, TxOut>,
  options: Options,
}

impl Wallet {
  pub(crate) fn load(options: &Options) -> Result<Self> {
    let bitcoin_client = options.pepecoin_rpc_client_for_wallet_command(false)?;

    let ord_client = OrdClient::builder()
      .default_headers({
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::ACCEPT, http::header::HeaderValue::from_static("application/json"));
        headers
      })
      .build()?;

    let rpc_url = options.server_url.clone();

    // Sync with server
    let bitcoin_block_count = bitcoin_client.get_block_count()? + 1;
    loop {
      let response = ord_client.get(rpc_url.join("/blockcount")?).send()?;
      if !response.status().is_success() {
        bail!("failed to get blockcount from ord server: {}", response.status());
      }
      let ord_block_count: u64 = response.text()?.parse()?;
      if ord_block_count >= bitcoin_block_count {
        break;
      }
      thread::sleep(Duration::from_millis(100));
    }

    let mut utxos = BTreeMap::new();

    #[derive(Deserialize)]
    struct UnspentEntry {
      txid: Txid,
      vout: u32,
      amount: f64,
      #[serde(rename = "scriptPubKey")]
      script_pub_key: String,
    }

    let unspent: Vec<UnspentEntry> = bitcoin_client
      .call("listunspent", &[])
      .context("failed to list unspent outputs")?;

    for entry in unspent {
      let outpoint = OutPoint::new(entry.txid, entry.vout);
      let amount = Amount::from_btc(entry.amount)
        .map_err(|e| anyhow!("invalid amount: {e}"))?;
      let script_pubkey = Script::from_str(&entry.script_pub_key)
        .context("failed to parse scriptPubKey")?;

      utxos.insert(outpoint, TxOut {
        value: amount.to_sat(),
        script_pubkey,
      });
    }

    #[derive(Deserialize)]
    struct JsonOutPoint {
      txid: Txid,
      vout: u32,
    }

    let locked_outpoints: Vec<JsonOutPoint> = bitcoin_client
      .call("listlockunspent", &[])
      .context("failed to list locked unspent outputs")?;

    let mut locked_utxos = BTreeMap::new();

    for JsonOutPoint { txid, vout } in locked_outpoints {
      let outpoint = OutPoint::new(txid, vout);
      let tx = bitcoin_client
        .get_raw_transaction(&txid)
        .context("failed to get raw transaction")?;

      let txout = tx.output.get(vout as usize)
        .ok_or_else(|| anyhow!("locked outpoint {outpoint} not found in transaction"))?;

      utxos.insert(outpoint, txout.clone());
      locked_utxos.insert(outpoint, txout.clone());
    }

    // Fetch output info
    let outpoints: Vec<OutPoint> = utxos.keys().cloned().collect();
    let response = ord_client.post(rpc_url.join("/outputs")?).json(&outpoints).send()?;
    if !response.status().is_success() {
      bail!("failed to get outputs from ord server: {}", response.status());
    }
    let response_outputs: Vec<api::Output> = response.json()?;
    let output_info: BTreeMap<OutPoint, api::Output> = outpoints.into_iter().zip(response_outputs).collect();

    for (outpoint, info) in &output_info {
      if !info.indexed {
        bail!("output in Pepecoin Core wallet but not in ord index: {outpoint}");
      }
    }

    // Fetch inscription details
    let inscription_ids: Vec<InscriptionId> = output_info.values().flat_map(|info| info.inscriptions.clone()).collect();
    let (inscriptions, inscription_info) = if !inscription_ids.is_empty() {
      let response = ord_client.post(rpc_url.join("/inscriptions")?).json(&inscription_ids).send()?;
      if !response.status().is_success() {
        bail!("failed to get inscriptions from ord server: {}", response.status());
      }
      let response_inscriptions: Vec<api::Inscription> = response.json()?;
      let mut inscriptions = BTreeMap::new();
      let mut inscription_info = BTreeMap::new();
      for info in response_inscriptions {
        inscriptions.entry(info.satpoint).or_insert_with(Vec::new).push(info.id);
        inscription_info.insert(info.id, info);
      }
      (inscriptions, inscription_info)
    } else {
      (BTreeMap::new(), BTreeMap::new())
    };

    // Fetch status
    let response = ord_client.get(rpc_url.join("/status")?).send()?;
    if !response.status().is_success() {
      bail!("failed to get status from ord server: {}", response.status());
    }
    let status: api::Status = response.json()?;

    Ok(Self {
      bitcoin_client: Arc::new(bitcoin_client),
      ord_client,
      rpc_url,
      has_sat_index: status.sat_index,
      utxos,
      inscriptions,
      inscription_info,
      output_info,
      locked_utxos,
      options: options.clone(),
    })
  }

  pub(crate) fn utxos(&self) -> &BTreeMap<OutPoint, TxOut> {
    &self.utxos
  }

  pub(crate) fn inscriptions(&self) -> &BTreeMap<SatPoint, Vec<InscriptionId>> {
    &self.inscriptions
  }

  pub(crate) fn inscription_info(&self) -> &BTreeMap<InscriptionId, api::Inscription> {
    &self.inscription_info
  }

  pub(crate) fn output_info(&self) -> &BTreeMap<OutPoint, api::Output> {
    &self.output_info
  }

  pub(crate) fn has_sat_index(&self) -> bool {
    self.has_sat_index
  }

  pub(crate) fn bitcoin_client(&self) -> &Client {
    &self.bitcoin_client
  }

  pub(crate) fn chain(&self) -> Chain {
    self.options.chain()
  }

  pub(crate) fn get_unspent_output_ranges(&self) -> Result<Vec<(OutPoint, Vec<(u128, u128)>)>> {
    if !self.has_sat_index {
      bail!("ord server does not have a sat index");
    }
    Ok(self.output_info
      .iter()
      .filter_map(|(outpoint, info)| {
        info.sat_ranges.as_ref().map(|ranges| {
          (*outpoint, ranges.iter().map(|(s, e)| (*s as u128, *e as u128)).collect())
        })
      })
      .collect())
  }
}
