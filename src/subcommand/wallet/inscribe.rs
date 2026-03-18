use {
  super::*,
  crate::wallet::{
    signer::LocalSigner,
    Wallet,
  },
  bitcoin::{
    blockdata::script,
    secp256k1::{self, Secp256k1},
    util::key::{PrivateKey, PublicKey},
    EcdsaSighashType, PackedLockTime, Witness,
  },
  std::collections::BTreeSet,
  super::batch::{
    file::BatchFile,
    plan::{
      build_lock_scripts, calculate_fee, create_batch_inscription_transactions,
      split_inscription_into_batches,
    },
    BatchOutput, InscriptionOutput,
  },
  super::broadcast::{RevealJob, RevealTx},
};

const MEMPOOL_CHAIN_LIMIT: usize = 23;

#[derive(Serialize)]
struct Output {
  commit: Txid,
  inscription: InscriptionId,
  reveal: Txid,
  destination: Address,
  fees: u64,
}

#[derive(Serialize)]
struct DryRunOutput {
  commit: Txid,
  inscription: InscriptionId,
  reveal: Txid,
  destination: Address,
  fees: u64,
  reveal_count: usize,
  batch_count: usize,
  batch_size: usize,
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
  #[clap(
    help = "Shibescribe sat with contents of <FILE>",
    required_unless_present = "batch"
  )]
  pub(crate) file: Option<PathBuf>,
  #[clap(
    long,
    help = "Do not check that transactions are equal to or below the 100,000 bytes limit. Transactions over this limit are currently nonstandard and will not be relayed by bitcoind in its default configuration. Do not use this flag unless you understand the implications."
  )]
  pub(crate) no_limit: bool,
  #[clap(long, alias = "dryrun", help = "Don't sign or broadcast transactions.")]
  pub(crate) dry_run: bool,
  #[clap(long, help = "Do not back up recovery key.")]
  pub(crate) no_backup: bool,
  #[clap(long, help = "Send inscription to <DESTINATION>.", conflicts_with = "batch")]
  pub(crate) destination: Option<Address>,
  #[clap(long, help = "Use postage of <POSTAGE> sats. [default: 100000]")]
  pub(crate) postage: Option<Amount>,
  #[clap(
    long,
    help = "Inscribe multiple files from <BATCH> YAML file.",
    conflicts_with = "file"
  )]
  pub(crate) batch: Option<PathBuf>,
}

impl Inscribe {
  pub(crate) fn validate_files(&self) -> Result {
    if let Some(ref file) = self.file {
      if !file.exists() {
        bail!("file not found: {}", file.display());
      }
    }
    if let Some(ref batch) = self.batch {
      BatchFile::load(batch)?;
    }
    Ok(())
  }

  pub(crate) fn run(self, wallet: Wallet) -> Result {
    let client = wallet.bitcoin_client();
    let (pubkey, privkey) = self.get_key_pair(&wallet)?;
    let fee_rate = self
      .fee_rate
      .unwrap_or(FeeRate::try_from(wallet.chain().default_fee_rate()).unwrap());
    let commit_fee_rate = self.commit_fee_rate.unwrap_or(fee_rate);

    let min = wallet.chain().min_fee_rate();
    let min_fee_rate = FeeRate::try_from(min).unwrap();
    if fee_rate < min_fee_rate {
      bail!("fee rate must be at least {min} sat/vB (Pepecoin minimum relay fee)");
    }
    if commit_fee_rate < min_fee_rate {
      bail!("commit fee rate must be at least {min} sat/vB (Pepecoin minimum relay fee)");
    }

    let postage = self.postage.unwrap_or(wallet.chain().default_postage());

    let utxos = wallet
      .utxos()
      .iter()
      .map(|(outpoint, txout)| (*outpoint, Amount::from_sat(txout.value)))
      .collect::<BTreeMap<OutPoint, Amount>>();

    let existing_inscriptions = wallet
      .inscriptions()
      .iter()
      .map(|(sp, ids)| (*sp, ids[0]))
      .collect::<BTreeMap<SatPoint, InscriptionId>>();

    if let Some(batch_path) = &self.batch {
      let batch_file = BatchFile::load(batch_path)?;
      let (inscriptions, destinations) = batch_file.inscriptions(wallet.chain(), batch_path, client)?;

      let (commit_tx, reveal_chains, fees) = create_batch_inscription_transactions(
        inscriptions,
        destinations.clone(),
        existing_inscriptions,
        wallet.chain().network(),
        utxos,
        [wallet.get_address(true)?, wallet.get_address(true)?],
        commit_fee_rate,
        fee_rate,
        pubkey,
        postage,
      )?;

      let total_reveal_count: usize = reveal_chains.iter().map(|c| c.len()).sum();

      if self.dry_run {
        let mut inscription_outputs = Vec::new();
        for (i, chain) in reveal_chains.iter().enumerate() {
          inscription_outputs.push(InscriptionOutput {
            inscription: chain[0].tx.txid().into(),
            reveal: chain.last().unwrap().tx.txid(),
            destination: destinations[i].clone(),
          });
        }

        let batch_count = (total_reveal_count + MEMPOOL_CHAIN_LIMIT - 1) / MEMPOOL_CHAIN_LIMIT;

        let dry_dir = wallet.settings().data_dir()
          .join("wallets")
          .join(wallet.name())
          .join("jobs")
          .join("dry");
        fs::create_dir_all(&dry_dir)?;
        let dry_file = dry_dir.join(format!("{}.json", commit_tx.txid()));

        #[derive(Serialize)]
        struct BatchDryRunOutput {
          commit: Txid,
          inscriptions: Vec<InscriptionOutput>,
          total_fees: u64,
          reveal_count: usize,
          batch_count: usize,
          batch_size: usize,
        }

        let output = BatchDryRunOutput {
          commit: commit_tx.txid(),
          inscriptions: inscription_outputs,
          total_fees: fees,
          reveal_count: total_reveal_count,
          batch_count,
          batch_size: MEMPOOL_CHAIN_LIMIT,
        };

        serde_json::to_writer_pretty(fs::File::create(&dry_file)?, &output)?;
        print_json(&output)?;
      } else {
        let signed_commit_tx = LocalSigner::sign_transaction(&wallet, commit_tx)?;
        let commit_txid = signed_commit_tx.txid();

        client
          .send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_commit_tx))
          .context("Failed to send commit transaction")?;

        let mut inscription_outputs = Vec::new();
        let secp = Secp256k1::new();
        let mut all_reveals = Vec::new();

        for (i, chain) in reveal_chains.into_iter().enumerate() {
          let mut last_txid = commit_txid;
          let mut signed_chain = Vec::new();

          for (j, reveal) in chain.into_iter().enumerate() {
            let mut tx = reveal.tx;
            if j == 0 {
              tx.input[0].previous_output = OutPoint { txid: commit_txid, vout: i as u32 };
            } else {
              tx.input[0].previous_output = OutPoint { txid: last_txid, vout: 0 };
            }

            let sighash = tx.signature_hash(0, &reveal.redeem_script, EcdsaSighashType::All as u32);
            let msg = secp256k1::Message::from_slice(&sighash[..])?;
            let sig = secp.sign_ecdsa(&msg, &privkey.inner);

            let mut sig_bytes = sig.serialize_der().to_vec();
            sig_bytes.push(EcdsaSighashType::All as u8);

            let mut script_sig = script::Builder::new();
            for instruction in reveal.partial_script.instructions() {
              match instruction {
                Ok(script::Instruction::PushBytes(data)) => {
                  script_sig = script_sig.push_slice(data);
                }
                Ok(script::Instruction::Op(op)) => {
                  script_sig = script_sig.push_opcode(op);
                }
                _ => {}
              }
            }
            script_sig = script_sig.push_slice(&sig_bytes);
            script_sig = script_sig.push_slice(reveal.redeem_script.as_bytes());

            tx.input[0].script_sig = script_sig.into_script();
            last_txid = tx.txid();

            all_reveals.push(RevealTx {
              index: all_reveals.len(),
              txid: tx.txid(),
              raw_hex: hex::encode(bitcoin::consensus::encode::serialize(&tx)),
              broadcast: false,
              confirmed: false,
            });

            signed_chain.push(tx);
          }

          inscription_outputs.push(InscriptionOutput {
            inscription: signed_chain[0].txid().into(),
            reveal: signed_chain.last().unwrap().txid(),
            destination: destinations[i].clone(),
          });
        }

        let job_file = wallet.settings().data_dir()
          .join("wallets")
          .join(wallet.name())
          .join("jobs")
          .join(format!("{}.json", commit_txid));

        fs::create_dir_all(job_file.parent().unwrap())?;

        let mut job = RevealJob {
          commit_txid,
          inscription_id: inscription_outputs[0].inscription,
          destination: inscription_outputs[0].destination.clone(),
          total_fees: fees,
          batch_size: MEMPOOL_CHAIN_LIMIT,
          created_at: Utc::now(),
          reveals: all_reveals,
        };

        // Broadcast first batch
        for reveal in job.reveals.iter_mut().take(MEMPOOL_CHAIN_LIMIT) {
          match client.call::<Txid>("sendrawtransaction", &[serde_json::to_value(&reveal.raw_hex)?]) {
            Ok(_) => {
              reveal.broadcast = true;
            }
            Err(e) => {
              let error_msg = e.to_string();
              if error_msg.contains("-26") && error_msg.contains("too-long-mempool-chain") {
                break;
              } else if error_msg.contains("-27") || error_msg.contains("Transaction already in mempool") {
                reveal.broadcast = true;
              } else {
                return Err(e.into());
              }
            }
          }
        }

        serde_json::to_writer_pretty(fs::File::create(&job_file)?, &job)?;

        print_json(BatchOutput {
          commit: commit_txid,
          inscriptions: inscription_outputs,
          total_fees: fees,
        })?;

        log::info!("Reveal broadcast job created at: {}", job_file.display());
      }
    } else {
      let inscription = Inscription::from_file(
        wallet.chain(),
        self.file.as_ref().context("missing file")?,
      )?;

      let reveal_tx_destination = self
        .destination
        .clone()
        .map(Ok)
        .unwrap_or_else(|| wallet.get_address(false))?;

      let (txs, scripts, fees) = self.create_inscription_transactions(
        self.satpoint,
        inscription,
        existing_inscriptions,
        wallet.chain().network(),
        utxos.clone(),
        [wallet.get_address(true)?, wallet.get_address(true)?],
        reveal_tx_destination.clone(),
        commit_fee_rate,
        fee_rate,
        pubkey,
        postage,
      )?;

      let reveal_count = txs.len() - 1; // exclude commit tx

      if self.dry_run {
        let inscription_id = txs[1].txid().into();
        let batch_count = (reveal_count + MEMPOOL_CHAIN_LIMIT - 1) / MEMPOOL_CHAIN_LIMIT;

        let dry_dir = wallet.settings().data_dir()
          .join("wallets")
          .join(wallet.name())
          .join("jobs")
          .join("dry");
        fs::create_dir_all(&dry_dir)?;
        let dry_file = dry_dir.join(format!("{}.json", txs[0].txid()));

        let output = DryRunOutput {
          commit: txs[0].txid(),
          inscription: inscription_id,
          reveal: txs.last().unwrap().txid(),
          destination: reveal_tx_destination,
          fees,
          reveal_count,
          batch_count,
          batch_size: MEMPOOL_CHAIN_LIMIT,
        };

        serde_json::to_writer_pretty(fs::File::create(&dry_file)?, &output)?;
        print_json(&output)?;
      } else {
        let signed_commit_tx = LocalSigner::sign_transaction(&wallet, txs[0].clone())?;
        let mut last_txid = signed_commit_tx.txid();
        let commit = last_txid;

        client
          .send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_commit_tx))
          .context("Failed to send commit transaction")?;

        let secp = Secp256k1::new();
        let mut reveal_txid = Txid::all_zeros();
        let mut inscription_id = InscriptionId { txid: Txid::all_zeros(), index: 0 };
        let mut reveals = Vec::new();

        for i in 1..txs.len() {
          let (redeem_script, partial_script) = &scripts[i - 1];

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
              Ok(script::Instruction::PushBytes(data)) => {
                script_sig = script_sig.push_slice(data);
              }
              Ok(script::Instruction::Op(op)) => {
                script_sig = script_sig.push_opcode(op);
              }
              _ => {}
            }
          }
          script_sig = script_sig.push_slice(&sig_bytes);
          script_sig = script_sig.push_slice(redeem_script.as_bytes());

          reveal_tx.input[0].script_sig = script_sig.into_script();
          last_txid = reveal_tx.txid();
          
          if i == 1 {
            inscription_id = last_txid.into();
          }
          reveal_txid = last_txid;

          reveals.push(RevealTx {
            index: i - 1,
            txid: reveal_txid,
            raw_hex: hex::encode(bitcoin::consensus::encode::serialize(&reveal_tx)),
            broadcast: false,
            confirmed: false,
          });
        }

        let job_file = wallet.settings().data_dir()
          .join("wallets")
          .join(wallet.name())
          .join("jobs")
          .join(format!("{}.json", commit));

        fs::create_dir_all(job_file.parent().unwrap())?;

        let mut job = RevealJob {
          commit_txid: commit,
          inscription_id,
          destination: reveal_tx_destination.clone(),
          total_fees: fees,
          batch_size: MEMPOOL_CHAIN_LIMIT,
          created_at: Utc::now(),
          reveals,
        };

        // Broadcast first batch
        for reveal in job.reveals.iter_mut().take(MEMPOOL_CHAIN_LIMIT) {
          match client.call::<Txid>("sendrawtransaction", &[serde_json::to_value(&reveal.raw_hex)?]) {
            Ok(_) => {
              reveal.broadcast = true;
            }
            Err(e) => {
              let error_msg = e.to_string();
              if error_msg.contains("-26") && error_msg.contains("too-long-mempool-chain") {
                break;
              } else if error_msg.contains("-27") || error_msg.contains("Transaction already in mempool") {
                reveal.broadcast = true;
              } else {
                return Err(e.into());
              }
            }
          }
        }

        serde_json::to_writer_pretty(fs::File::create(&job_file)?, &job)?;

        print_json(Output {
          commit,
          reveal: reveal_txid,
          inscription: inscription_id,
          destination: reveal_tx_destination,
          fees,
        })?;

        log::info!("Reveal broadcast job created at: {}", job_file.display());
      };
    }

    Ok(())
  }

  fn get_key_pair(&self, wallet: &Wallet) -> Result<(PublicKey, PrivateKey)> {
    let privkey = wallet.get_private_key(false, 0)?;
    let secp = Secp256k1::new();
    let pubkey = privkey.public_key(&secp);
    Ok((pubkey, privkey))
  }

  fn create_inscription_transactions(
    &self,
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

    let batches = split_inscription_into_batches(&inscription);

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

    let locks = build_lock_scripts(&batches, &pubkey);

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

    fees += calculate_fee(&unsigned_commit_tx, &utxos);
    let mut last_outpoint = OutPoint {
      txid: unsigned_commit_tx.txid(),
      vout: unsigned_commit_tx
        .output
        .iter()
        .position(|o| o.script_pubkey == first_lock_address.script_pubkey())
        .unwrap() as u32,
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
          script_pubkey: if is_last {
            destination.script_pubkey()
          } else {
            Address::p2sh(&locks[i + 1], network)
              .unwrap()
              .script_pubkey()
          },
          value: next_value,
        }],
        lock_time: PackedLockTime::ZERO,
        version: 1,
      };

      fees += fee;
      scripts.push((locks[i].clone(), batch));

      last_outpoint = OutPoint {
        txid: reveal_tx.txid(),
        vout: 0,
      };
      last_value = next_value;
      txs.push(reveal_tx);
    }

    Ok((txs, scripts, fees))
  }
}

#[cfg(test)]
mod tests {
  use {super::*, super::batch::plan::RevealTx};

  #[test]
  fn reveal_transaction_pays_fee() {
    let utxos = vec![(outpoint(1), Amount::from_sat(200000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(100_000)),
      batch: None,
    };

    let (txs, _scripts, fees) = inscribe.create_inscription_transactions(
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

    assert_eq!(final_output_value + change_value, total_input - fees);
  }

  #[test]
  fn inscript_tansactions_opt_in_to_rbf() {
    let utxos = vec![(outpoint(1), Amount::from_sat(200000))];
    let inscription = inscription("text/plain", "ord");
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(100_000)),
      batch: None,
    };

    let (txs, _, _) = inscribe.create_inscription_transactions(
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
    let commit_address = change(0);
    let reveal_address = recipient();
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(100_000)),
      batch: None,
    };

    let error = inscribe.create_inscription_transactions(
      None,
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
      "{}" ,
      error
    );
  }

  #[test]
  fn batched_multitx_inscription_roundtrip() {
    use crate::inscription::{Inscription, ParsedInscription};

    // Create a large inscription requiring multiple batches (20 chunks × 520 bytes)
    // This exercises countdown values ≤ 16 which use OP_PUSHNUM opcodes
    let body = vec![0x42u8; 520 * 20];
    let inscription = Inscription::new(Some(b"image/svg+xml".to_vec()), Some(body.clone()), None);

    let utxos = vec![(outpoint(1), Amount::from_sat(50_000_000_000))];
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(100_000)),
      batch: None,
    };

    let (txs, scripts, _fees) = inscribe.create_inscription_transactions(
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
    )
    .unwrap();

    // Must be multi-tx (commit + multiple reveals)
    assert!(
      txs.len() > 2,
      "Expected multi-tx inscription, got {} txs",
      txs.len()
    );

    // Reconstruct scriptSigs as the signing code would (preserving opcodes)
    let sig_scripts: Vec<Script> = scripts
      .iter()
      .map(|(_lock, batch)| {
        let mut builder = script::Builder::new();
        for instruction in batch.instructions() {
          match instruction {
            Ok(script::Instruction::PushBytes(data)) => {
              builder = builder.push_slice(data);
            }
            Ok(script::Instruction::Op(op)) => {
              builder = builder.push_opcode(op);
            }
            _ => {}
          }
        }
        builder.into_script()
      })
      .collect();

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

  #[test]
  fn batch_creates_multiple_inscriptions() {
    let utxos = vec![(outpoint(1), Amount::from_sat(1_000_000_000))];
    let inscriptions = vec![
      inscription("text/plain", "A"),
      inscription("text/plain", "B"),
      inscription("text/plain", "C"),
    ];
    let destinations = vec![recipient(), recipient(), recipient()];
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let _inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(10_000)),
      batch: None,
    };

    let (commit_tx, reveal_chains, fees): (Transaction, Vec<Vec<RevealTx>>, u64) = 
      create_batch_inscription_transactions(
        inscriptions,
        destinations,
        BTreeMap::new(),
        Network::Bitcoin,
        utxos.into_iter().collect(),
        [change(0), change(1)],
        FeeRate::try_from(1.0).unwrap(),
        FeeRate::try_from(1.0).unwrap(),
        pubkey,
        Amount::from_sat(10_000),
      )
      .unwrap();

    assert_eq!(reveal_chains.len(), 3);
    assert_eq!(commit_tx.output.len(), 4); // 3 inscriptions + 1 change
    assert!(fees > 0);

    for (i, chain) in reveal_chains.iter().enumerate() {
      assert_eq!(chain.len(), 1);
      assert_eq!(chain[0].tx.input[0].previous_output.txid, commit_tx.txid());
      assert_eq!(chain[0].tx.input[0].previous_output.vout, i as u32);
    }
  }

  #[test]
  fn batch_with_large_files_creates_multi_tx_chains() {
    let utxos = vec![(outpoint(1), Amount::from_sat(1_000_000_000))];
    let inscriptions = vec![
      inscription("text/plain", "small"),
      Inscription::new(Some(b"text/plain".to_vec()), Some(vec![0; 3000]), None),
    ];
    let destinations = vec![recipient(), recipient()];
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let _inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(10_000)),
      batch: None,
    };

    let (commit_tx, reveal_chains, _fees): (Transaction, Vec<Vec<RevealTx>>, u64) = 
      create_batch_inscription_transactions(
        inscriptions,
        destinations,
        BTreeMap::new(),
        Network::Bitcoin,
        utxos.into_iter().collect(),
        [change(0), change(1)],
        FeeRate::try_from(1.0).unwrap(),
        FeeRate::try_from(1.0).unwrap(),
        pubkey,
        Amount::from_sat(10_000),
      )
      .unwrap();

    assert_eq!(reveal_chains.len(), 2);
    assert_eq!(reveal_chains[0].len(), 1);
    assert!(reveal_chains[1].len() > 1);
    assert_eq!(commit_tx.output.len(), 3); // 2 inscriptions + 1 change
  }

  #[test]
  fn batch_with_single_inscription() {
    let utxos = vec![(outpoint(1), Amount::from_sat(1_000_000_000))];
    let inscriptions = vec![inscription("text/plain", "A")];
    let destinations = vec![recipient()];
    let pubkey = PublicKey::from_slice(
      &hex::decode("03adb2ca38e09e396cf600906cc6ec66ae6be09fbcc0bc600fb060000000000000").unwrap(),
    )
    .unwrap();

    let _inscribe = Inscribe {
      satpoint: None,
      fee_rate: Some(FeeRate::try_from(1.0).unwrap()),
      commit_fee_rate: None,
      file: None,
      no_limit: false,
      dry_run: true,
      no_backup: true,
      destination: None,
      postage: Some(Amount::from_sat(10_000)),
      batch: None,
    };

    let (commit_tx, reveal_chains, _fees): (Transaction, Vec<Vec<RevealTx>>, u64) = 
      create_batch_inscription_transactions(
        inscriptions,
        destinations,
        BTreeMap::new(),
        Network::Bitcoin,
        utxos.into_iter().collect(),
        [change(0), change(1)],
        FeeRate::try_from(1.0).unwrap(),
        FeeRate::try_from(1.0).unwrap(),
        pubkey,
        Amount::from_sat(10_000),
      )
      .unwrap();

    assert_eq!(reveal_chains.len(), 1);
    assert_eq!(commit_tx.output.len(), 2); // 1 inscription + 1 change
  }
}
