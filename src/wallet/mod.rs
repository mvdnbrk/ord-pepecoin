use {
  super::*,
  bitcoin::{
    util::bip32::{ChildNumber, DerivationPath, ExtendedPrivKey, ExtendedPubKey},
    secp256k1::Secp256k1,
    PrivateKey,
  },
  redb::{
    Database, ReadableTable,
    TableDefinition, ReadableDatabase,
  },
  reqwest::blocking::Client as OrdClient,
  std::sync::Arc,
};

pub(crate) mod signer;

macro_rules! define_table {
  ($name:ident, $key:ty, $value:ty) => {
    const $name: TableDefinition<$key, $value> = TableDefinition::new(stringify!($name));
  };
}

define_table! { METADATA, &str, &[u8] }
define_table! { DESCRIPTORS, &str, &str }
define_table! { ADDRESS_INDEX, &str, u32 }

#[derive(Clone, Debug)]
pub struct Wallet {
  bitcoin_client: Arc<Client>,
  _ord_client: OrdClient,
  _rpc_url: Url,
  _has_sat_index: bool,
  utxos: BTreeMap<OutPoint, TxOut>,
  inscriptions: BTreeMap<SatPoint, Vec<InscriptionId>>,
  inscription_info: BTreeMap<InscriptionId, api::Inscription>,
  output_info: BTreeMap<OutPoint, api::Output>,
  _locked_utxos: BTreeMap<OutPoint, TxOut>,
  options: Options,
  database: Arc<Database>,
}

impl Wallet {
  pub fn open_database(options: &Options, wallet_name: &str) -> Result<Arc<Database>> {
    let path = options.data_dir()?.join("wallets").join(wallet_name);
    fs::create_dir_all(&path)?;
    let db_path = path.join("wallet.redb");

    let db = Database::create(&db_path)?;

    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      fs::set_permissions(&db_path, fs::Permissions::from_mode(0o600))?;
    }

    let tx = db.begin_write()?;
    tx.open_table(METADATA)?;
    tx.open_table(DESCRIPTORS)?;
    tx.open_table(ADDRESS_INDEX)?;
    tx.commit()?;

    Ok(Arc::new(db))
  }

  pub(crate) fn initialize(options: &Options, wallet_name: &str, seed: [u8; 64]) -> Result {
    let database = Self::open_database(options, wallet_name)?;
    let network = options.chain().network();
    let secp = Secp256k1::new();
    let master_private_key = ExtendedPrivKey::new_master(network, &seed)?;
    let fingerprint = master_private_key.fingerprint(&secp);

    let tx = database.begin_write()?;
    {
      let mut metadata = tx.open_table(METADATA)?;
      metadata.insert("master_fingerprint", fingerprint.as_bytes().as_slice())?;
      metadata.insert("seed", seed.as_slice())?;
    }

    {
      let mut descriptors = tx.open_table(DESCRIPTORS)?;
      for change in [false, true] {
        let derivation_path = DerivationPath::master()
          .child(ChildNumber::Hardened { index: 44 })
          .child(ChildNumber::Hardened { index: 3434 }) // PEPECOIN_COIN_TYPE
          .child(ChildNumber::Hardened { index: 0 })
          .child(ChildNumber::Normal { index: u32::from(change) });
        
        let derived_xpriv = master_private_key.derive_priv(&secp, &derivation_path)?;
        let derived_xpub = ExtendedPubKey::from_priv(&secp, &derived_xpriv);
        
        let descriptor = format!("pkh({derived_xpub}/*)");
        descriptors.insert(if change { "change" } else { "receive" }, descriptor.as_str())?;
      }
    }
    tx.commit()?;
    Ok(())
  }

  pub(crate) fn load(options: &Options, wallet_name: &str, server_url: Option<Url>, no_sync: bool) -> Result<Self> {
    let bitcoin_client = options.pepecoin_rpc_client_for_wallet_command(false)?;
    let database = Self::open_database(options, wallet_name)?;

    let ord_client = OrdClient::builder()
      .default_headers({
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::ACCEPT, http::header::HeaderValue::from_static("application/json"));
        headers
      })
      .build()?;

    let rpc_url = options.server_url(server_url)?;

    // Sync with server
    if !no_sync {
      let bitcoin_block_count = bitcoin_client.get_block_count()? + 1;
      loop {
        let response = ord_client.get(rpc_url.join("/blockcount")?).send()
          .context("wallet failed to retrieve block count from server. Make sure `ordpep server` is running.")?;
        if !response.status().is_success() {
          bail!("failed to get blockcount from ordpep server: {}", response.status());
        }
        let ord_block_count: u64 = response.text()?.parse()?;
        if ord_block_count >= bitcoin_block_count {
          break;
        }
        thread::sleep(Duration::from_millis(100));
      }
    }

    let addresses = Self::get_addresses(&database, options.chain())?;

    // Import wallet addresses as watch-only into Core (no rescan) so that
    // listtransactions and other RPC calls can track our addresses.
    for address in &addresses {
      let _: Result<(), _> = bitcoin_client.call(
        "importaddress",
        &[
          serde_json::to_value(address.to_string())?,
          serde_json::to_value("")?,       // label
          serde_json::to_value(false)?,    // rescan
        ],
      );
    }

    #[derive(Deserialize)]
    struct UnspentEntry {
      txid: Txid,
      vout: u32,
      #[serde(rename = "scriptPubKey")]
      script_pub_key: String,
      amount: f64,
    }

    let mut utxos = BTreeMap::new();
    let unspent: Vec<UnspentEntry> = bitcoin_client
      .call("listunspent", &[])
      .context("failed to list unspent outputs")?;

    for utxo in unspent {
      let script_pubkey = Script::from_str(&utxo.script_pub_key)
        .context("failed to parse scriptPubKey")?;
      let outpoint = OutPoint::new(utxo.txid, utxo.vout);
      let amount = Amount::from_btc(utxo.amount)
        .map_err(|e| anyhow!("invalid amount: {e}"))?;
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

let locked_outpoints = bitcoin_client.call::<Vec<JsonOutPoint>>("listlockunspent", &[])?;
let mut locked_utxos = BTreeMap::new();

for outpoint in locked_outpoints {
  let outpoint = OutPoint::new(outpoint.txid, outpoint.vout);
  let tx = bitcoin_client.get_raw_transaction(&outpoint.txid)?;
  if let Some(txout) = tx.output.get(outpoint.vout as usize) {
    if addresses.contains(&Address::from_script(&txout.script_pubkey, options.chain().network()).unwrap()) {
      utxos.insert(outpoint, txout.clone());
      locked_utxos.insert(outpoint, txout.clone());
    }
  }
}

    // Fetch output info
    let outpoints: Vec<OutPoint> = utxos.keys().cloned().collect();
    let response = ord_client.post(rpc_url.join("/outputs")?).json(&outpoints).send()?;
    if !response.status().is_success() {
      bail!("failed to get outputs from ordpep server: {}", response.status());
    }
    let response_outputs: Vec<api::Output> = response.json()?;
    let output_info: BTreeMap<OutPoint, api::Output> = outpoints.into_iter().zip(response_outputs).collect();

    // Fetch inscription details
    let inscription_ids: Vec<InscriptionId> = output_info.values().flat_map(|info| info.inscriptions.clone()).collect();
    let (inscriptions, inscription_info) = if !inscription_ids.is_empty() {
      let response = ord_client.post(rpc_url.join("/inscriptions")?).json(&inscription_ids).send()?;
      if !response.status().is_success() {
        bail!("failed to get inscriptions from ordpep server: {}", response.status());
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

    let status: api::Status = ord_client.get(rpc_url.join("/status")?).send()?.json()?;

    Ok(Self {
      bitcoin_client: Arc::new(bitcoin_client),
      _ord_client: ord_client,
      _rpc_url: rpc_url,
      _has_sat_index: status.sat_index,
      utxos,
      inscriptions,
      inscription_info,
      output_info,
      _locked_utxos: locked_utxos,
      options: options.clone(),
      database,
    })
  }

  fn parse_xpub_from_descriptor(descriptor: &str) -> Result<ExtendedPubKey> {
    let xpub_str = descriptor
      .strip_prefix("pkh(")
      .and_then(|s| s.strip_suffix("/*)"))
      .ok_or_else(|| anyhow!("invalid descriptor format: {descriptor}"))?;
    ExtendedPubKey::from_str(xpub_str).context("invalid xpub in descriptor")
  }

  fn get_addresses(database: &Database, chain: Chain) -> Result<Vec<Address>> {
    let rtx = database.begin_read()?;
    let descriptors = rtx.open_table(DESCRIPTORS)?;
    let address_index = rtx.open_table(ADDRESS_INDEX)?;
    
    let mut addresses = Vec::new();
    let secp = Secp256k1::new();
    let network = chain.network();

    for key in ["receive", "change"] {
      if let Ok(Some(desc_str)) = descriptors.get(key) {
        let last_index = address_index.get(key)?.map(|v| v.value()).unwrap_or(0);
        
        if let Ok(xpub) = Self::parse_xpub_from_descriptor(desc_str.value()) {
          for i in 0..=last_index + 20 { 
            let derived_xpub = xpub.derive_pub(&secp, &[ChildNumber::Normal { index: i }])?;
            let public_key = derived_xpub.to_pub();
            addresses.push(Address::p2pkh(&public_key, network));
          }
        }
      }
    }
    
    Ok(addresses)
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

  pub(crate) fn bitcoin_client(&self) -> &Client {
    &self.bitcoin_client
  }

  pub(crate) fn chain(&self) -> Chain {
    self.options.chain()
  }

  pub(crate) fn get_unspent_output_ranges(&self) -> Result<Vec<(OutPoint, Vec<(u128, u128)>)>> {
    if !self._has_sat_index {
      bail!("ordpep server does not have a sat index");
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

  pub(crate) fn get_address(&self, change: bool) -> Result<Address> {
    let tx = self.database.begin_write()?;
    let key = if change { "change" } else { "receive" };
    
    let index = tx.open_table(ADDRESS_INDEX)?.get(key)?.map(|v| v.value()).unwrap_or(0);

    let address = {
      let descriptors = tx.open_table(DESCRIPTORS)?;
      let desc_str = descriptors.get(key)?
        .ok_or_else(|| anyhow!("wallet contains no descriptors. Was it created with `ordpep wallet create`?"))?;
      
      let xpub = Self::parse_xpub_from_descriptor(desc_str.value())?;
      
      let secp = Secp256k1::new();
      let derived_xpub = xpub.derive_pub(&secp, &[ChildNumber::Normal { index }])?;
      Address::p2pkh(&derived_xpub.to_pub(), self.chain().network())
    };
    
    tx.open_table(ADDRESS_INDEX)?.insert(key, index + 1)?;
    tx.commit()?;
    
    Ok(address)
  }

  pub(crate) fn get_address_info(&self, script_pubkey: &Script) -> Result<(bool, u32)> {
    let rtx = self.database.begin_read()?;
    let descriptors = rtx.open_table(DESCRIPTORS)?;
    let address_index = rtx.open_table(ADDRESS_INDEX)?;
    
    let secp = Secp256k1::new();
    let network = self.chain().network();

    for change in [false, true] {
      let key = if change { "change" } else { "receive" };
      if let Ok(Some(desc_str)) = descriptors.get(key) {
        let last_index = address_index.get(key)?.map(|v| v.value()).unwrap_or(0);
        
        if let Ok(xpub) = Self::parse_xpub_from_descriptor(desc_str.value()) {
          for i in 0..=last_index + 100 { // Search a bit further just in case
            let derived_xpub = xpub.derive_pub(&secp, &[ChildNumber::Normal { index: i }])?;
            let public_key = derived_xpub.to_pub();
            let address = Address::p2pkh(&public_key, network);
            if address.script_pubkey() == *script_pubkey {
              return Ok((change, i));
            }
          }
        }
      }
    }
    
    bail!("script_pubkey not found in wallet: {}", script_pubkey);
  }

  pub(crate) fn get_master_key(&self) -> Result<ExtendedPrivKey> {
    let rtx = self.database.begin_read()?;
    let metadata = rtx.open_table(METADATA)?;
    let seed_bytes = metadata.get("seed")?
      .ok_or_else(|| anyhow!("wallet contains no seed. Was it created with `ordpep wallet create`?"))?;
    
    let mut seed = [0u8; 64];
    seed.copy_from_slice(seed_bytes.value());

    Ok(ExtendedPrivKey::new_master(self.chain().network(), &seed)?)
  }

  pub(crate) fn get_private_key(&self, change: bool, index: u32) -> Result<PrivateKey> {
    let master_private_key = self.get_master_key()?;
    let secp = Secp256k1::new();
    
    let derivation_path = DerivationPath::master()
      .child(ChildNumber::Hardened { index: 44 })
      .child(ChildNumber::Hardened { index: 3434 }) // PEPECOIN_COIN_TYPE
      .child(ChildNumber::Hardened { index: 0 })
      .child(ChildNumber::Normal { index: u32::from(change) })
      .child(ChildNumber::Normal { index });
    
    let derived_xpriv = master_private_key.derive_priv(&secp, &derivation_path)?;
    
    Ok(PrivateKey::new(derived_xpriv.private_key, self.chain().network()))
  }
}
