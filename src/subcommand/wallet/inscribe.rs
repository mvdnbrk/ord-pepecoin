use {
  super::*,
  crate::wallet::Wallet,
  bitcoin::{
    blockdata::{opcodes, script},
    secp256k1::{self, Secp256k1},
    util::key::{PrivateKey, PublicKey},
    EcdsaSighashType, PackedLockTime, Witness,
  },
  std::collections::BTreeSet,
};

// Pepecoin Core enforces a 1650-byte scriptSig limit (IsStandard policy).
// The scriptSig contains: inscription data + signature (~74 bytes) + redeem script.
// We reserve 150 bytes for signature + redeem script overhead, leaving ~1500 for data.
const MAX_PAYLOAD_LEN: usize = 1500;

#[derive(Serialize)]
struct Output {
  commit: Txid,
  inscription: InscriptionId,
  reveal: Txid,
  fees: u64,
}

#[derive(Debug, Parser)]
pub(crate) struct Inscribe {
  #[clap(long, help = "Shibescribe <SATPOINT>")]
  pub(crate) satpoint: Option<SatPoint>,
  #[clap(
    long,
    help = "Use fee rate of <FEE_RATE> sats/vB. [default: 1000.0]"
  )]
  pub(crate) fee_rate: Option<FeeRate>,
  #[clap(
    long,
    help = "Use <COMMIT_FEE_RATE> sats/vbyte for commit transaction.\nDefaults to <FEE_RATE> if unset."
  )]
  pub(crate) commit_fee_rate: Option<FeeRate>,
  #[clap(help = "Shibescribe sat with contents of <FILE>")]
  pub(crate) file: PathBuf,
  #[clap(
    long,
    help = "Do not check that transactions are equal to or below the 100,000 bytes limit. Transactions over this limit are currently nonstandard and will not be relayed by bitcoind in its default configuration. Do not use this flag unless you understand the implications."
  )]
  pub(crate) no_limit: bool,
  #[clap(long, help = "Don't sign or broadcast transactions.")]
  pub(crate) dry_run: bool,
  #[clap(long, help = "Do not back up recovery key.")]
  pub(crate) no_backup: bool,
  #[clap(long, help = "Send inscription to <DESTINATION>.")]
  pub(crate) destination: Option<Address>,
  #[clap(long, help = "Use postage of <POSTAGE> sats. [default: 100000]")]
  pub(crate) postage: Option<Amount>,
}

impl Inscribe {
  pub(crate) fn run(self, wallet: Wallet) -> Result {
    let inscription = Inscription::from_file(wallet.chain(), &self.file)?;

    let client = wallet.bitcoin_client();

    let utxos = wallet
      .utxos()
      .iter()
      .map(|(outpoint, txout)| (*outpoint, Amount::from_sat(txout.value)))
      .collect::<BTreeMap<OutPoint, Amount>>();

    let inscriptions = wallet.inscriptions().iter().map(|(sp, ids)| (*sp, ids[0])).collect::<BTreeMap<SatPoint, InscriptionId>>();

    let commit_tx_change = [get_change_address(client)?, get_change_address(client)?];

    let reveal_tx_destination = self
      .destination
      .clone()
      .map(Ok)
      .unwrap_or_else(|| get_change_address(client))?;

    let (pubkey, privkey) = self.get_key_pair(client)?;

    let fee_rate = self.fee_rate.unwrap_or(FeeRate::try_from(wallet.chain().default_fee_rate()).unwrap());

    let postage = self.postage.unwrap_or(wallet.chain().default_postage());

    let (txs, scripts, fees) = Inscribe::create_inscription_transactions(
      self.satpoint,
      inscription,
      inscriptions,
      wallet.chain().network(),
      utxos.clone(),
      commit_tx_change,
      reveal_tx_destination,
      self.commit_fee_rate.unwrap_or(fee_rate),
      fee_rate,
      pubkey,
      postage,
    )?;

    if self.dry_run {
      let inscription_id = txs[1].txid().into();
      print_json(Output {
        commit: txs[0].txid(),
        reveal: txs.last().unwrap().txid(),
        inscription: inscription_id,
        fees,
      })?;
    } else {
      let mut signed_txs = Vec::new();
      let mut last_txid;
      
      let commit_hex = bitcoin::consensus::encode::serialize_hex(&txs[0]);
      let result: serde_json::Value = client
        .call("signrawtransaction", &[commit_hex.into()])
        .context("failed to sign commit transaction")?;
      let signed_hex = result["hex"]
        .as_str()
        .ok_or_else(|| anyhow!("missing hex in signrawtransaction response"))?;
      if result["complete"].as_bool() != Some(true) {
        bail!("Failed to sign commit transaction: {}", result["errors"]);
      }
      let signed_bytes = hex::decode(signed_hex)?;
      let signed_commit_tx: Transaction = bitcoin::consensus::encode::deserialize(&signed_bytes)?;
      last_txid = signed_commit_tx.txid();
      signed_txs.push(signed_bytes);

      let secp = Secp256k1::new();

      for i in 1..txs.len() {
        let (redeem_script, partial_script) = &scripts[i-1];

        let mut reveal_tx = txs[i].clone();
        reveal_tx.input[0].previous_output.txid = last_txid;

        // Compute P2SH sighash and sign locally
        let sighash = reveal_tx.signature_hash(0, redeem_script, EcdsaSighashType::All as u32);
        let msg = secp256k1::Message::from_slice(&sighash[..])?;
        let sig = secp.sign_ecdsa(&msg, &privkey.inner);

        // Build DER signature with sighash type byte
        let mut sig_bytes = sig.serialize_der().to_vec();
        sig_bytes.push(EcdsaSighashType::All as u8);

        // Build scriptSig: <inscription_data> <signature> <redeem_script>
        let mut script_sig = script::Builder::new();
        for instruction in partial_script.instructions() {
          match instruction {
            Ok(script::Instruction::PushBytes(data)) => { script_sig = script_sig.push_slice(data); }
            Ok(script::Instruction::Op(op)) => { script_sig = script_sig.push_opcode(op); }
            _ => {}
          }
        }
        script_sig = script_sig.push_slice(&sig_bytes);
        script_sig = script_sig.push_slice(redeem_script.as_bytes());

        reveal_tx.input[0].script_sig = script_sig.into_script();
        last_txid = reveal_tx.txid();
        signed_txs.push(bitcoin::consensus::encode::serialize(&reveal_tx));
      }

      let commit_tx: Transaction = bitcoin::consensus::encode::deserialize(&signed_txs[0])?;
      let commit = commit_tx.txid();
      
      let reveal_tx: Transaction = bitcoin::consensus::encode::deserialize(signed_txs.last().unwrap())?;
      let reveal = reveal_tx.txid();
      
      let inscription_tx: Transaction = bitcoin::consensus::encode::deserialize(&signed_txs[1])?;
      let inscription_id = inscription_tx.txid().into();

      for (i, signed_tx_bytes) in signed_txs.iter().enumerate() {
        client
          .send_raw_transaction(signed_tx_bytes)
          .context(format!("Failed to send transaction {}", i))?;
      }

      print_json(Output {
        commit,
        reveal,
        inscription: inscription_id,
        fees,
      })?;
    };

    Ok(())
  }

  fn get_key_pair(&self, client: &Client) -> Result<(PublicKey, PrivateKey)> {
    let address: Address = client.call("getnewaddress", &[])?;
    let result: serde_json::Value = client
      .call("validateaddress", &[address.to_string().into()])
      .context("failed to validate address")?;
    let pubkey_hex = result["pubkey"]
      .as_str()
      .ok_or_else(|| anyhow!("no pubkey in validateaddress response for {address}"))?;
    let pubkey_bytes = hex::decode(pubkey_hex)
      .context("invalid pubkey hex")?;
    let pubkey = PublicKey::from_slice(&pubkey_bytes)
      .context("invalid pubkey")?;

    let wif: String = client
      .call("dumpprivkey", &[address.to_string().into()])
      .context("failed to dump private key")?;
    let privkey = PrivateKey::from_wif(&wif)
      .context("invalid WIF private key")?;

    Ok((pubkey, privkey))
  }

  fn create_inscription_transactions(
    satpoint: Option<SatPoint>,
    inscription: Inscription,
    inscriptions: BTreeMap<SatPoint, InscriptionId>,
    network: Network,
    utxos: BTreeMap<OutPoint, Amount>,
    change: [Address; 2],
    destination: Address,
    commit_fee_rate: FeeRate,
    reveal_fee_rate: FeeRate,
    pubkey: PublicKey,
    postage: Amount,
  ) -> Result<(Vec<Transaction>, Vec<(Script, Script)>, u64)> {
    let satpoint = if let Some(satpoint) = satpoint {
      satpoint
    } else {
      let inscribed_utxos = inscriptions
        .keys()
        .map(|satpoint| satpoint.outpoint)
        .collect::<BTreeSet<OutPoint>>();

      utxos
        .keys()
        .find(|outpoint| !inscribed_utxos.contains(outpoint))
        .map(|outpoint| SatPoint {
          outpoint: *outpoint,
          offset: 0,
        })
        .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
    };

    for (inscribed_satpoint, inscription_id) in &inscriptions {
      if inscribed_satpoint == &satpoint {
        return Err(anyhow!("sat at {} already inscribed", satpoint));
      }

      if inscribed_satpoint.outpoint == satpoint.outpoint {
        return Err(anyhow!(
          "utxo {} already inscribed with inscription {inscription_id} on sat {inscribed_satpoint}",
          satpoint.outpoint,
        ));
      }
    }

    let inscription_script = inscription.get_inscription_script();

    // Split inscription script into batches for multi-tx inscriptions.
    // The script structure is: "ord" <npieces> <content_type> [<countdown> <data>]*
    // Batches must split at (countdown, data) pair boundaries so the parser
    // sees complete pairs in each tx. We also preserve opcodes (OP_PUSHNUM_N
    // for countdown values 1-16) rather than re-encoding them as push data.
    #[derive(Clone)]
    enum Elem {
      Push(Vec<u8>),
      Op(opcodes::All),
    }

    impl Elem {
      fn apply(self, builder: script::Builder) -> script::Builder {
        match self {
          Elem::Push(data) => builder.push_slice(&data),
          Elem::Op(op) => builder.push_opcode(op),
        }
      }
      fn encoded_len(&self) -> usize {
        match self {
          Elem::Push(data) => {
            let len = data.len();
            if len <= 75 { 1 + len }
            else if len <= 255 { 2 + len }
            else if len <= 65535 { 3 + len }
            else { 5 + len }
          }
          Elem::Op(_) => 1,
        }
      }
    }

    // Collect all instructions as Elem
    let elems: Vec<Elem> = inscription_script.instructions().filter_map(|instr| {
      match instr.ok()? {
        script::Instruction::PushBytes(data) => Some(Elem::Push(data.to_vec())),
        script::Instruction::Op(op) => Some(Elem::Op(op)),
      }
    }).collect();

    // Header = first 3 elements: "ord", npieces, content_type
    // Data = pairs of (countdown, data) starting from index 3
    let header = &elems[..3.min(elems.len())];
    let data_elems = &elems[3.min(elems.len())..];

    // Collect (countdown, data) pairs
    let mut pairs: Vec<(&Elem, &Elem)> = Vec::new();
    let mut i = 0;
    while i + 1 < data_elems.len() {
      pairs.push((&data_elems[i], &data_elems[i + 1]));
      i += 2;
    }

    // Build batches: first batch gets header + initial pairs, subsequent batches get pairs only
    let mut batches = Vec::new();
    let mut partial = script::Builder::new();
    let mut partial_len: usize = 0;

    // Add header to first batch
    for elem in header {
      partial = elem.clone().apply(partial);
      partial_len += elem.encoded_len();
    }

    for (countdown, data) in pairs {
      let pair_len = countdown.encoded_len() + data.encoded_len();

      // If adding this pair would exceed limit and we have content, start new batch
      if partial_len + pair_len > MAX_PAYLOAD_LEN && partial_len > 0 {
        batches.push(partial.into_script());
        partial = script::Builder::new();
        partial_len = 0;
      }

      partial = countdown.clone().apply(partial);
      partial = data.clone().apply(partial);
      partial_len += pair_len;
    }

    if partial_len > 0 {
      batches.push(partial.into_script());
    }

    let mut total_reveal_fees = 0;
    let mut reveal_fees = Vec::new();
    for batch in &batches {
        let num_chunks = batch.instructions().count();
        let estimated_sig_size = batch.len() + 1 + 72 + 1 + (33 + 1 + num_chunks + 1);
        let tx_vsize = 82 + estimated_sig_size; 
        let fee = reveal_fee_rate.fee(tx_vsize).to_sat();
        total_reveal_fees += fee;
        reveal_fees.push(fee);
    }

    let mut txs = Vec::new();
    let mut fees = 0;
    let mut scripts = Vec::new();

    let mut locks = Vec::new();
    for batch in &batches {
        let mut lock_builder = script::Builder::new()
          .push_slice(&pubkey.to_bytes())
          .push_opcode(opcodes::all::OP_CHECKSIGVERIFY);
        for _ in batch.instructions() {
          lock_builder = lock_builder.push_opcode(opcodes::all::OP_DROP);
        }
        let lock = lock_builder.push_opcode(opcodes::all::OP_PUSHNUM_1).into_script();
        locks.push(lock);
    }

    let first_lock_address = Address::p2sh(&locks[0], network).unwrap();
    let total_postage = postage + Amount::from_sat(total_reveal_fees);
    let unsigned_commit_tx = TransactionBuilder::build_transaction_with_value(
      satpoint,
      inscriptions.clone(),
      utxos.clone(),
      first_lock_address.clone(),
      change.clone(),
      commit_fee_rate,
      total_postage,
    )?;

    fees += Self::calculate_fee(&unsigned_commit_tx, &utxos);
    let mut last_outpoint = OutPoint {
      txid: unsigned_commit_tx.txid(),
      vout: unsigned_commit_tx.output.iter().position(|o| o.script_pubkey == first_lock_address.script_pubkey()).unwrap() as u32,
    };
    let mut last_value = unsigned_commit_tx.output[last_outpoint.vout as usize].value;
    txs.push(unsigned_commit_tx);

    for (i, batch) in batches.into_iter().enumerate() {
        let is_last = i == reveal_fees.len() - 1;
        let fee = reveal_fees[i];
        let next_value = last_value.checked_sub(fee).unwrap();
        
        let reveal_tx = Transaction {
          input: vec![TxIn {
            previous_output: last_outpoint,
            script_sig: Script::new(),
            witness: Witness::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          }],
          output: vec![TxOut {
            script_pubkey: if is_last { destination.script_pubkey() } else { Address::p2sh(&locks[i+1], network).unwrap().script_pubkey() },
            value: next_value,
          }],
          lock_time: PackedLockTime::ZERO,
          version: 1,
        };

        fees += fee;
        scripts.push((locks[i].clone(), batch));
        
        last_outpoint = OutPoint { txid: reveal_tx.txid(), vout: 0 };
        last_value = next_value;
        txs.push(reveal_tx);
    }

    Ok((txs, scripts, fees))
  }

  fn calculate_fee(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> u64 {
    tx.input
      .iter()
      .map(|txin| utxos.get(&txin.previous_output).map(|a| a.to_sat()).unwrap_or(0))
      .sum::<u64>()
      .checked_sub(tx.output.iter().map(|txout| txout.value).sum::<u64>())
      .unwrap_or(0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reveal_transaction_pays_fee() {
    let utxos = vec![(outpoint(1), Amount::from_sat(200000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(&hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap()).unwrap();

    let (txs, _scripts, fees) = Inscribe::create_inscription_transactions(
      Some(satpoint(1, 0)),
      inscription,
      BTreeMap::new(),
      Network::Bitcoin,
      utxos.into_iter().collect(),
      [commit_address, change(1)],
      reveal_address,
      FeeRate::try_from(1.0).unwrap(),
      FeeRate::try_from(1.0).unwrap(),
      pubkey,
      Amount::from_sat(100_000),
    )
    .unwrap();

    assert!(fees > 0);
    
    let total_input = 200000;
    
    let final_output_value = txs.last().unwrap().output[0].value;
    let commit_tx = &txs[0];
    let change_value: u64 = commit_tx.output.iter().skip(1).map(|o| o.value).sum();
    
    assert_eq!(
      final_output_value + change_value,
      total_input - fees
    );
  }

  #[test]
  fn inscript_tansactions_opt_in_to_rbf() {
    let utxos = vec![(outpoint(1), Amount::from_sat(200000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(&hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap()).unwrap();

    let (txs, _, _) = Inscribe::create_inscription_transactions(
      Some(satpoint(1, 0)),
      inscription,
      BTreeMap::new(),
      Network::Bitcoin,
      utxos.into_iter().collect(),
      [commit_address, change(1)],
      reveal_address,
      FeeRate::try_from(1.0).unwrap(),
      FeeRate::try_from(1.0).unwrap(),
      pubkey,
      Amount::from_sat(100_000),
    )
    .unwrap();

    for tx in txs {
      assert!(tx.is_explicitly_rbf());
    }
  }

  #[test]
  fn inscribe_with_no_satpoint_and_no_cardinal_utxos() {
    let utxos = vec![(outpoint(1), Amount::from_sat(1000))];
    let mut inscriptions = BTreeMap::new();
    inscriptions.insert(
      SatPoint {
        outpoint: outpoint(1),
        offset: 0,
      },
      inscription_id(1),
    );

    let inscription = inscription("text/plain", "ord");
    let satpoint = None;
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(&hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap()).unwrap();

    let error = Inscribe::create_inscription_transactions(
      satpoint,
      inscription,
      inscriptions,
      Network::Bitcoin,
      utxos.into_iter().collect(),
      [commit_address, change(1)],
      reveal_address,
      FeeRate::try_from(1.0).unwrap(),
      FeeRate::try_from(1.0).unwrap(),
      pubkey,
      Amount::from_sat(100_000),
    )
    .unwrap_err()
    .to_string();

    assert!(
      error.contains("wallet contains no cardinal utxos"),
      "{}",
      error
    );
  }

  #[test]
  fn batched_multitx_inscription_roundtrip() {
    use crate::inscription::{Inscription, ParsedInscription};

    // Create a large inscription requiring multiple batches (20 chunks × 520 bytes)
    // This exercises countdown values ≤ 16 which use OP_PUSHNUM opcodes
    let body = vec![0x42u8; 520 * 20];
    let inscription = Inscription::new(
      Some(b"image/svg+xml".to_vec()),
      Some(body.clone()),
      None,
    );

    let utxos = vec![(outpoint(1), Amount::from_sat(50_000_000_000))];
    let pubkey = PublicKey::from_slice(&hex::decode(
      "03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000"
    ).unwrap()).unwrap();

    let (txs, scripts, _fees) = Inscribe::create_inscription_transactions(
      Some(satpoint(1, 0)),
      inscription,
      BTreeMap::new(),
      Network::Bitcoin,
      utxos.into_iter().collect(),
      [change(0), change(1)],
      recipient(),
      FeeRate::try_from(1.0).unwrap(),
      FeeRate::try_from(1.0).unwrap(),
      pubkey,
      Amount::from_sat(100_000),
    ).unwrap();

    // Must be multi-tx (commit + multiple reveals)
    assert!(txs.len() > 2, "Expected multi-tx inscription, got {} txs", txs.len());

    // Reconstruct scriptSigs as the signing code would (preserving opcodes)
    let sig_scripts: Vec<Script> = scripts.iter().map(|(_lock, batch)| {
      let mut builder = script::Builder::new();
      for instruction in batch.instructions() {
        match instruction {
          Ok(script::Instruction::PushBytes(data)) => { builder = builder.push_slice(data); }
          Ok(script::Instruction::Op(op)) => { builder = builder.push_opcode(op); }
          _ => {}
        }
      }
      builder.into_script()
    }).collect();

    // Parser should reconstruct the complete inscription from all tx scriptSigs
    let result = crate::inscription::InscriptionParser::parse(sig_scripts);
    match result {
      ParsedInscription::Complete(parsed) => {
        assert_eq!(parsed.content_type, Some(b"image/svg+xml".to_vec()));
        assert_eq!(parsed.body.as_ref().map(|b| b.len()), Some(body.len()));
        assert_eq!(parsed.body, Some(body));
      }
      other => panic!("Expected Complete inscription, got {:?}", other),
    }
  }
}
