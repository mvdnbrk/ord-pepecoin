use {
  super::*,
  bitcoin::{
    hashes::hex::ToHex,
    psbt::serialize::Deserialize,
    secp256k1::{rand, Secp256k1},
    util::key::PublicKey,
    Address, Witness,
  },
  bitcoincore_rpc::RawTx,
  serde_json::json,
  std::str::FromStr,
};

pub(crate) struct Server {
  pub(crate) state: Arc<Mutex<State>>,
  pub(crate) network: Network,
}

impl Server {
  pub(crate) fn new(state: Arc<Mutex<State>>) -> Self {
    let network = state.lock().unwrap().network;
    Self { network, state }
  }

  fn state(&self) -> MutexGuard<'_, State> {
    self.state.lock().unwrap()
  }

  fn not_found() -> jsonrpc_core::Error {
    jsonrpc_core::Error::new(jsonrpc_core::types::error::ErrorCode::ServerError(-8))
  }
}

impl Api for Server {
  fn get_balances(&self) -> Result<GetBalancesResult, jsonrpc_core::Error> {
    let unspent = self.list_unspent(None, None, None, None, None)?;
    Ok(GetBalancesResult {
      mine: GetBalancesResultEntry {
        immature: Amount::from_sat(0),
        trusted: unspent.iter().map(|entry| entry.amount).sum(),
        untrusted_pending: Amount::from_sat(0),
      },
      watchonly: None,
    })
  }

  fn get_blockchain_info(&self) -> Result<GetBlockchainInfoResult, jsonrpc_core::Error> {
    Ok(GetBlockchainInfoResult {
      chain: String::from(match self.network {
        Network::Bitcoin => "main",
        Network::Testnet => "test",
        Network::Signet => "signet",
        Network::Regtest => "regtest",
      }),
      blocks: 0,
      headers: 0,
      best_block_hash: self.state().hashes[0],
      difficulty: 0.0,
      median_time: 0,
      verification_progress: 0.0,
      initial_block_download: false,
      chain_work: Vec::new(),
      size_on_disk: 0,
      pruned: false,
      prune_height: None,
      automatic_pruning: None,
      prune_target_size: None,
      softforks: HashMap::new(),
      warnings: String::new(),
    })
  }

  fn get_network_info(&self) -> Result<GetNetworkInfoResult, jsonrpc_core::Error> {
    Ok(GetNetworkInfoResult {
      version: self.state().version,
      subversion: String::new(),
      protocol_version: 0,
      local_services: String::new(),
      local_relay: false,
      time_offset: 0,
      connections: 0,
      connections_in: None,
      connections_out: None,
      network_active: true,
      networks: Vec::new(),
      relay_fee: Amount::from_sat(0),
      incremental_fee: Amount::from_sat(0),
      local_addresses: Vec::new(),
      warnings: String::new(),
    })
  }

  fn get_block_hash(&self, height: usize) -> Result<BlockHash, jsonrpc_core::Error> {
    match self.state().hashes.get(height) {
      Some(block_hash) => Ok(*block_hash),
      None => Err(Self::not_found()),
    }
  }

  fn get_block_header(
    &self,
    block_hash: BlockHash,
    verbose: bool,
  ) -> Result<Value, jsonrpc_core::Error> {
    if verbose {
      let height = match self
        .state()
        .hashes
        .iter()
        .position(|hash| *hash == block_hash)
      {
        Some(height) => height,
        None => return Err(Self::not_found()),
      };

      Ok(
        serde_json::to_value(GetBlockHeaderResult {
          bits: String::new(),
          chainwork: Vec::new(),
          confirmations: 0,
          difficulty: 0.0,
          hash: block_hash,
          height,
          median_time: None,
          merkle_root: TxMerkleNode::all_zeros(),
          n_tx: 0,
          next_block_hash: None,
          nonce: 0,
          previous_block_hash: None,
          time: 0,
          version: 0,
          version_hex: Some(vec![0, 0, 0, 0]),
        })
        .unwrap(),
      )
    } else {
      match self.state().blocks.get(&block_hash) {
        Some(block) => Ok(serde_json::to_value(hex::encode(serialize(&block.header))).unwrap()),
        None => Err(Self::not_found()),
      }
    }
  }

  fn get_block(&self, block_hash: BlockHash, verbose: bool) -> Result<String, jsonrpc_core::Error> {
    assert!(!verbose, "Verbosity level {verbose} is unsupported");
    match self.state().blocks.get(&block_hash) {
      Some(block) => Ok(hex::encode(serialize(block))),
      None => Err(Self::not_found()),
    }
  }

  fn get_block_count(&self) -> Result<u64, jsonrpc_core::Error> {
    Ok(
      self
        .state()
        .hashes
        .len()
        .saturating_sub(1)
        .try_into()
        .unwrap(),
    )
  }

  fn get_wallet_info(&self) -> Result<GetWalletInfoResult, jsonrpc_core::Error> {
    if let Some(wallet_name) = self.state().loaded_wallets.first().cloned() {
      Ok(GetWalletInfoResult {
        avoid_reuse: None,
        balance: Amount::from_sat(0),
        hd_seed_id: None,
        immature_balance: Amount::from_sat(0),
        keypool_oldest: None,
        keypool_size: 0,
        keypool_size_hd_internal: 0,
        pay_tx_fee: Amount::from_sat(0),
        private_keys_enabled: false,
        scanning: None,
        tx_count: 0,
        unconfirmed_balance: Amount::from_sat(0),
        unlocked_until: None,
        wallet_name,
        wallet_version: 0,
      })
    } else {
      Err(Self::not_found())
    }
  }

  fn create_raw_transaction(
    &self,
    utxos: Vec<CreateRawTransactionInput>,
    outs: HashMap<String, f64>,
    locktime: Option<i64>,
    replaceable: Option<bool>,
  ) -> Result<String, jsonrpc_core::Error> {
    assert_eq!(locktime, None, "locktime param not supported");
    assert_eq!(replaceable, None, "replaceable param not supported");

    let tx = Transaction {
      version: 0,
      lock_time: PackedLockTime(0),
      input: utxos
        .iter()
        .map(|input| TxIn {
          previous_output: OutPoint::new(input.txid, input.vout),
          script_sig: Script::new(),
          sequence: Sequence::MAX,
          witness: Witness::new(),
        })
        .collect(),
      output: outs
        .values()
        .map(|amount| TxOut {
          value: (*amount * COIN_VALUE as f64) as u64,
          script_pubkey: Script::new(),
        })
        .collect(),
    };

    Ok(hex::encode(serialize(&tx)))
  }

  fn create_wallet(
    &self,
    name: String,
    _disable_private_keys: Option<bool>,
    _blank: Option<bool>,
    _passphrase: Option<String>,
    _avoid_reuse: Option<bool>,
  ) -> Result<LoadWalletResult, jsonrpc_core::Error> {
    self.state().wallets.insert(name.clone());
    Ok(LoadWalletResult {
      name,
      warning: None,
    })
  }

  fn sign_raw_transaction_with_wallet(
    &self,
    tx: String,
    _utxos: Option<Vec<serde_json::Value>>,
    _sighash_type: Option<String>,
  ) -> Result<Value, jsonrpc_core::Error> {
    self.sign_raw_transaction(tx, _utxos, None, _sighash_type)
  }

  fn sign_raw_transaction(
    &self,
    tx: String,
    _utxos: Option<Vec<serde_json::Value>>,
    _privkeys: Option<Vec<String>>,
    _sighash_type: Option<String>,
  ) -> Result<Value, jsonrpc_core::Error> {
    let mut transaction = Transaction::deserialize(&hex::decode(tx).unwrap()).unwrap();
    for input in &mut transaction.input {
      input.witness = Witness::from_vec(vec![vec![0; 64]]);
      // Add a dummy signature to script_sig if it's empty (for P2SH)
      if input.script_sig.is_empty() {
        input.script_sig = script::Builder::new()
          .push_slice(&[0; 71]) // dummy sig
          .push_slice(&[0; 33]) // dummy redeem script (will be replaced by inscriber)
          .into_script();
      }
    }

    Ok(
      serde_json::to_value(SignRawTransactionResult {
        hex: hex::decode(transaction.raw_hex()).unwrap(),
        complete: true,
        errors: None,
      })
      .unwrap(),
    )
  }

  fn send_raw_transaction(&self, tx: String) -> Result<String, jsonrpc_core::Error> {
    let tx: Transaction = deserialize(&hex::decode(tx).unwrap()).unwrap();
    self.state.lock().unwrap().mempool.push(tx.clone());

    Ok(tx.txid().to_string())
  }

  fn send_to_address(
    &self,
    address: Address,
    amount: f64,
    comment: Option<String>,
    comment_to: Option<String>,
    subtract_fee: Option<bool>,
    replaceable: Option<bool>,
    confirmation_target: Option<u32>,
    estimate_mode: Option<EstimateMode>,
  ) -> Result<Txid, jsonrpc_core::Error> {
    assert_eq!(comment, None);
    assert_eq!(comment_to, None);
    assert_eq!(subtract_fee, None);
    assert_eq!(replaceable, None);
    assert_eq!(confirmation_target, None);
    assert_eq!(estimate_mode, None);

    let mut state = self.state.lock().unwrap();
    let locked = state.locked.iter().cloned().collect();

    state.sent.push(Sent {
      address,
      amount,
      locked,
    });

    Ok(
      "0000000000000000000000000000000000000000000000000000000000000000"
        .parse()
        .unwrap(),
    )
  }

  fn get_transaction(
    &self,
    txid: Txid,
    _include_watchonly: Option<bool>,
  ) -> Result<Value, jsonrpc_core::Error> {
    match self.state.lock().unwrap().transactions.get(&txid) {
      Some(tx) => Ok(
        serde_json::to_value(GetTransactionResult {
          info: WalletTxInfo {
            txid,
            confirmations: 0,
            time: 0,
            timereceived: 0,
            blockhash: None,
            blockindex: None,
            blockheight: None,
            blocktime: None,
            wallet_conflicts: Vec::new(),
            bip125_replaceable: Bip125Replaceable::Unknown,
          },
          amount: SignedAmount::from_sat(0),
          fee: None,
          details: Vec::new(),
          hex: serialize(tx),
        })
        .unwrap(),
      ),
      None => Err(jsonrpc_core::Error::new(
        jsonrpc_core::types::error::ErrorCode::ServerError(-8),
      )),
    }
  }

  fn get_raw_transaction(
    &self,
    txid: Txid,
    verbose: Option<bool>,
    blockhash: Option<String>,
  ) -> Result<Value, jsonrpc_core::Error> {
    assert_eq!(blockhash, None, "Blockhash param is unsupported");
    if verbose.unwrap_or(false) {
      let state = self.state();
      let in_active_chain = state.transactions.contains_key(&txid);
      let tx = state.transactions.get(&txid).or_else(|| {
        state.mempool.iter().find(|tx| tx.txid() == txid)
      });
      match tx {
        Some(tx) => {
            let blockhash = state.hashes.iter().find(|h| {
                state.blocks.get(*h).map(|b| b.txdata.iter().any(|t| t.txid() == txid)).unwrap_or(false)
            }).cloned().or_else(|| state.hashes.last().cloned());

            let mut json = json!({
                "in_active_chain": in_active_chain,
                "hex": hex::encode(serialize(tx)),
                "txid": tx.txid().to_string(),
                "hash": tx.wtxid().to_string(),
                "size": tx.size(),
                "vsize": tx.vsize(),
                "version": tx.version,
                "locktime": tx.lock_time.0,
                "confirmations": if in_active_chain { 1 } else { 0 },
                "time": 0,
                "blocktime": 0,
                "vin": tx.input.iter().map(|input| {
                    if input.previous_output.is_null() {
                        json!({
                            "coinbase": hex::encode(input.script_sig.as_bytes()),
                            "sequence": input.sequence
                        })
                    } else {
                        json!({
                            "txid": input.previous_output.txid.to_string(),
                            "vout": input.previous_output.vout,
                            "scriptSig": {
                                "asm": "",
                                "hex": hex::encode(input.script_sig.as_bytes())
                            },
                            "sequence": input.sequence
                        })
                    }
                }).collect::<Vec<_>>(),
                "vout": tx.output.iter().enumerate().map(|(i, output)| {
                    json!({
                        "value": output.value as f64 / 100000000.0,
                        "n": i,
                        "scriptPubKey": {
                            "asm": "",
                            "hex": hex::encode(output.script_pubkey.as_bytes()),
                            "type": "pubkeyhash",
                            "address": Address::from_script(&output.script_pubkey, self.network).map(|a| a.to_string()).ok()
                        }
                    })
                }).collect::<Vec<_>>()
            });

            if let Some(blockhash) = blockhash {
                json.as_object_mut().unwrap().insert("blockhash".to_string(), json!(blockhash.to_string()));
            }

            Ok(json)
        }
        None => Err(Self::not_found()),
      }
    } else {
      let state = self.state();
      let tx = state.transactions.get(&txid).or_else(|| {
        state.mempool.iter().find(|tx| tx.txid() == txid)
      });
      match tx {
        Some(tx) => Ok(Value::String(hex::encode(serialize(tx)))),
        None => Err(Self::not_found()),
      }
    }
  }

  fn list_unspent(
    &self,
    minconf: Option<usize>,
    maxconf: Option<usize>,
    address: Option<bitcoin::Address>,
    include_unsafe: Option<bool>,
    query_options: Option<String>,
  ) -> Result<Vec<ListUnspentResultEntry>, jsonrpc_core::Error> {
    assert_eq!(minconf, None, "minconf param not supported");
    assert_eq!(maxconf, None, "maxconf param not supported");
    assert_eq!(address, None, "address param not supported");
    assert_eq!(include_unsafe, None, "include_unsafe param not supported");
    assert_eq!(query_options, None, "query_options param not supported");

    let state = self.state();

    let mut result = Vec::new();
    for (outpoint, &amount) in &state.utxos {
        if state.locked.contains(outpoint) {
            continue;
        }

        let tx = &state.transactions[&outpoint.txid];
        let output = &tx.output[outpoint.vout as usize];
        
        let address = Address::from_script(&output.script_pubkey, self.network).ok();
        
        result.push(ListUnspentResultEntry {
          txid: outpoint.txid,
          vout: outpoint.vout,
          address,
          label: None,
          redeem_script: None,
          witness_script: None,
          script_pub_key: output.script_pubkey.clone(),
          amount,
          confirmations: 0,
          spendable: true,
          solvable: true,
          descriptor: None,
          safe: true,
        });
    }

    Ok(result)
  }

  fn list_lock_unspent(&self) -> Result<Vec<JsonOutPoint>, jsonrpc_core::Error> {
    Ok(
      self
        .state()
        .locked
        .iter()
        .map(|outpoint| (*outpoint).into())
        .collect(),
    )
  }

  fn get_raw_change_address(
    &self,
    _address_type: Option<bitcoincore_rpc::json::AddressType>,
  ) -> Result<bitcoin::Address, jsonrpc_core::Error> {
    let secp256k1 = Secp256k1::new();
    let (secret_key, public_key) = secp256k1.generate_keypair(&mut rand::thread_rng());
    let pubkey = PublicKey::new(public_key);
    let address = Address::p2pkh(&pubkey, self.network);

    let privkey = bitcoin::PrivateKey::new(secret_key, self.network);

    let mut state = self.state();
    state.address_pubkeys.insert(address.clone(), pubkey);
    state.address_privkeys.insert(address.clone(), privkey);

    Ok(address)
  }

  fn get_descriptor_info(
    &self,
    desc: String,
  ) -> Result<GetDescriptorInfoResult, jsonrpc_core::Error> {
    Ok(GetDescriptorInfoResult {
      descriptor: desc,
      checksum: "".into(),
      is_range: false,
      is_solvable: false,
      has_private_keys: true,
    })
  }

  fn import_descriptors(
    &self,
    req: Vec<ImportDescriptors>,
  ) -> Result<Vec<ImportMultiResult>, jsonrpc_core::Error> {
    self
      .state()
      .descriptors
      .extend(req.into_iter().map(|params| params.descriptor));

    Ok(vec![ImportMultiResult {
      success: true,
      warnings: Vec::new(),
      error: None,
    }])
  }

  fn get_new_address(
    &self,
    _label: Option<String>,
    _address_type: Option<bitcoincore_rpc::json::AddressType>,
  ) -> Result<bitcoin::Address, jsonrpc_core::Error> {
    let secp256k1 = Secp256k1::new();
    let (secret_key, public_key) = secp256k1.generate_keypair(&mut rand::thread_rng());
    let pubkey = PublicKey::new(public_key);
    let address = Address::p2pkh(&pubkey, self.network);

    let privkey = bitcoin::PrivateKey::new(secret_key, self.network);

    let mut state = self.state();
    state.address_pubkeys.insert(address.clone(), pubkey);
    state.address_privkeys.insert(address.clone(), privkey);

    Ok(address)
  }

  fn dump_private_key(
    &self,
    address: Address,
  ) -> Result<String, jsonrpc_core::Error> {
    let state = self.state();
    match state.address_privkeys.get(&address) {
      Some(privkey) => Ok(privkey.to_wif()),
      None => {
          for (k, v) in &state.address_privkeys {
              if k.script_pubkey() == address.script_pubkey() {
                  return Ok(v.to_wif());
              }
          }
          Err(Self::not_found())
      }
    }
  }

  fn get_address_info(
    &self,
    address: String,
  ) -> Result<serde_json::Value, jsonrpc_core::Error> {
    self.validate_address(address)
  }

  fn validate_address(
    &self,
    address: String,
  ) -> Result<serde_json::Value, jsonrpc_core::Error> {
    let mut addr: Address = Address::from_str(&address).map_err(|_| jsonrpc_core::Error::invalid_params("invalid address"))?;
    
    let mut pubkey = self.state().address_pubkeys.get(&addr).cloned();
    
    if pubkey.is_none() {
        // Try other networks if it might be parsed wrong (e.g. Testnet vs Regtest)
        for network in [Network::Bitcoin, Network::Testnet, Network::Signet, Network::Regtest] {
            if network == addr.network { continue; }
            if let Ok(_other_addr) = Address::from_str(&address) {
                // Address::from_str doesn't take network, so we have to manually check variants if possible.
                // Actually bitcoin 0.29 Address doesn't have an easy way to change network without re-parsing from script or similar.
                // But we can just iterate over our keys and see if any matches the script_pubkey.
                for (k, v) in &self.state().address_pubkeys {
                    if k.script_pubkey() == addr.script_pubkey() {
                        pubkey = Some(v.clone());
                        addr = k.clone();
                        break;
                    }
                }
            }
            if pubkey.is_some() { break; }
        }
    }

    Ok(serde_json::json!({
      "isvalid": true,
      "address": address,
      "scriptPubKey": addr.script_pubkey().to_hex(),
      "ismine": true,
      "solvable": true,
      "isscript": false,
      "iswitness": false,
      "pubkey": pubkey.map(|pk| pk.inner.to_string()),
    }))
  }

  fn import_private_key(
    &self,
    privkey: String,
    label: Option<String>,
    _rescan: Option<bool>,
  ) -> Result<serde_json::Value, jsonrpc_core::Error> {
    let privkey = bitcoin::PrivateKey::from_wif(&privkey).map_err(|_| jsonrpc_core::Error::invalid_params("invalid privkey"))?;
    let pubkey = privkey.public_key(&Secp256k1::new());
    let address = Address::p2pkh(&pubkey, self.network);

    let mut state = self.state();
    state.imported_privkeys.push((privkey.to_wif(), label));
    state.address_pubkeys.insert(address.clone(), pubkey);
    state.address_privkeys.insert(address, privkey);
    Ok(serde_json::Value::Null)
  }

  fn list_transactions(
    &self,
    _label: Option<String>,
    count: Option<u16>,
    _skip: Option<usize>,
    _include_watchonly: Option<bool>,
  ) -> Result<Vec<ListTransactionResult>, jsonrpc_core::Error> {
    let state = self.state();
    let mut txs = Vec::new();

    let is_wallet_tx = |tx: &Transaction| {
        if tx.is_coin_base() {
            return false;
        }
        for output in &tx.output {
            if let Ok(address) = Address::from_script(&output.script_pubkey, self.network) {
                if state.address_pubkeys.contains_key(&address) {
                    return true;
                }
            }
        }
        for input in &tx.input {
            if let Some(prev_tx) = state.transactions.get(&input.previous_output.txid) {
                if let Some(output) = prev_tx.output.get(input.previous_output.vout as usize) {
                    if let Ok(address) = Address::from_script(&output.script_pubkey, self.network) {
                        if state.address_pubkeys.contains_key(&address) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    };

    for block_hash in state.hashes.iter().rev() {
        let block = &state.blocks[block_hash];
        for tx in block.txdata.iter().rev() {
            if is_wallet_tx(tx) {
                txs.push(tx);
            }
        }
    }
    for tx in state.mempool.iter().rev() {
        if is_wallet_tx(tx) {
            txs.push(tx);
        }
    }

    Ok(
      txs
        .into_iter()
        .take(count.unwrap_or(u16::MAX).into())
        .map(|tx| ListTransactionResult {
          info: WalletTxInfo {
            confirmations: state.get_confirmations(tx),
            blockhash: None,
            blockindex: None,
            blocktime: None,
            blockheight: None,
            txid: tx.txid(),
            time: 0,
            timereceived: 0,
            bip125_replaceable: Bip125Replaceable::Unknown,
            wallet_conflicts: Vec::new(),
          },
          detail: GetTransactionResultDetail {
            address: None,
            category: GetTransactionResultDetailCategory::Immature,
            amount: SignedAmount::from_sat(0),
            label: None,
            vout: 0,
            fee: Some(SignedAmount::from_sat(0)),
            abandoned: None,
          },
          trusted: None,
          comment: None,
        })
        .collect(),
    )
  }

  fn lock_unspent(
    &self,
    unlock: bool,
    outputs: Vec<JsonOutPoint>,
  ) -> Result<bool, jsonrpc_core::Error> {
    assert!(!unlock);

    let mut state = self.state();

    if state.fail_lock_unspent {
      return Ok(false);
    }

    for output in outputs {
      let output = OutPoint {
        vout: output.vout,
        txid: output.txid,
      };
      assert!(state.utxos.contains_key(&output));
      state.locked.insert(output);
    }

    Ok(true)
  }

  fn list_descriptors(&self) -> Result<ListDescriptorsResult, jsonrpc_core::Error> {
    Ok(ListDescriptorsResult {
      wallet_name: "ord".into(),
      descriptors: self
        .state()
        .descriptors
        .iter()
        .map(|desc| Descriptor {
          desc: desc.to_string(),
          timestamp: Timestamp::Now,
          active: true,
          internal: None,
          range: None,
          next: None,
        })
        .collect(),
    })
  }

  fn load_wallet(&self, wallet: String) -> Result<LoadWalletResult, jsonrpc_core::Error> {
    if self.state().wallets.contains(&wallet) {
      self.state().loaded_wallets.insert(wallet.clone());
      Ok(LoadWalletResult {
        name: wallet,
        warning: None,
      })
    } else {
      Err(Self::not_found())
    }
  }

  fn list_wallets(&self) -> Result<Vec<String>, jsonrpc_core::Error> {
    Ok(
      self
        .state()
        .loaded_wallets
        .clone()
        .into_iter()
        .collect::<Vec<String>>(),
    )
  }
}
