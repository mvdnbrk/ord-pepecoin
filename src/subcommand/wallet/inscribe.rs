use {
  super::batch::{
    file::BatchFile,
    plan::{
      build_lock_scripts, calculate_fee, create_batch_inscription_transactions,
      split_inscription_into_batches,
    },
    BatchOutput, InscriptionOutput,
  },
  super::job::{sanitize_batch_name, RevealJob, RevealTx, MEMPOOL_CHAIN_LIMIT},
  super::*,
  crate::wallet::{signer::LocalSigner, Wallet},
  bitcoin::{
    blockdata::script,
    secp256k1::{self, Secp256k1},
    util::key::{PrivateKey, PublicKey},
    EcdsaSighashType, PackedLockTime, Witness,
  },
  std::collections::BTreeSet,
};

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
  broadcast_rounds: usize,
}

#[derive(Debug, Parser)]
pub(crate) struct Inscribe {
  #[clap(long, help = "Inscribe <SATPOINT>")]
  pub(crate) satpoint: Option<SatPoint>,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB. [default: 1000.0]")]
  pub(crate) fee_rate: Option<FeeRate>,
  #[clap(
    long,
    help = "Use <COMMIT_FEE_RATE> sats/vbyte for commit transaction.\nDefaults to <FEE_RATE> if unset."
  )]
  pub(crate) commit_fee_rate: Option<FeeRate>,
  #[clap(
    long,
    help = "Inscribe sat with contents of <FILE>.",
    conflicts_with_all = &["batch", "delegate"],
    required_unless_present_any = &["batch", "delegate"]
  )]
  pub(crate) file: Option<PathBuf>,
  #[clap(
    long,
    help = "Delegate inscription content to <DELEGATE>.",
    conflicts_with_all = &["batch", "file"],
    required_unless_present_any = &["batch", "file"]
  )]
  pub(crate) delegate: Option<InscriptionId>,
  #[clap(
    long,
    help = "Do not check that transactions are equal to or below the 100,000 bytes limit. Transactions over this limit are currently nonstandard and will not be relayed by bitcoind in its default configuration. Do not use this flag unless you understand the implications."
  )]
  pub(crate) no_limit: bool,
  #[clap(long, alias = "dryrun", help = "Don't sign or broadcast transactions.")]
  pub(crate) dry_run: bool,
  #[clap(long, help = "Do not back up recovery key.")]
  pub(crate) no_backup: bool,
  #[clap(
    long,
    help = "Send inscription to <DESTINATION>.",
    conflicts_with = "batch"
  )]
  pub(crate) destination: Option<Address>,
  #[clap(long, help = "Use postage of <POSTAGE> sats. [default: 100000]")]
  pub(crate) postage: Option<Amount>,
  #[clap(
    long,
    help = "Inscribe multiple files from <BATCH> YAML file.",
    conflicts_with_all = &["file", "delegate"]
  )]
  pub(crate) batch: Option<PathBuf>,
  #[clap(long, help = "Make inscription a child of <PARENT>.")]
  pub(crate) parent: Option<InscriptionId>,
}

pub(crate) struct ParentInfo {
  pub(crate) id: InscriptionId,
  pub(crate) location: SatPoint,
  pub(crate) tx_out: TxOut,
  pub(crate) destination: Address,
}

pub(crate) const BATCH_COMMIT_CHUNK_SIZE: usize = 2000;

struct SignedReveal {
  reveals: Vec<RevealTx>,
  inscription_id: InscriptionId,
  last_txid: Txid,
}

/// Sign a chain of reveal transactions, returning signed RevealTx entries.
///
/// Each reveal tx in the chain spends the previous one's output.
/// The first reveal spends `parent_outpoint`.
fn sign_reveal_chain(
  chain: Vec<super::batch::plan::RevealTx>,
  parent_outpoint: OutPoint,
  privkey: &PrivateKey,
  secp: &Secp256k1<secp256k1::All>,
  parent_signing: Option<(&Wallet, &[(&ParentInfo, OutPoint)])>,
) -> Result<SignedReveal> {
  let mut last_txid = parent_outpoint.txid;
  let mut reveals = Vec::new();

  for (j, reveal) in chain.into_iter().enumerate() {
    let mut tx = reveal.tx;
    tx.input[0].previous_output = if j == 0 {
      parent_outpoint
    } else {
      OutPoint {
        txid: last_txid,
        vout: 0,
      }
    };

    // Sign parent inputs on the first reveal tx
    if j == 0 {
      if let Some((wallet, parent_entries)) = &parent_signing {
        for (idx, (parent_info, parent_utxo_outpoint)) in parent_entries.iter().enumerate() {
          let input_idx = idx + 1;
          tx.input[input_idx].previous_output = *parent_utxo_outpoint;

          let (change, index) = wallet.get_address_info(&parent_info.tx_out.script_pubkey)?;
          let parent_privkey = wallet.get_private_key(change, index)?;

          let parent_sighash = tx.signature_hash(
            input_idx,
            &parent_info.tx_out.script_pubkey,
            EcdsaSighashType::All as u32,
          );
          let parent_msg = secp256k1::Message::from_slice(&parent_sighash[..])?;
          let parent_sig = secp.sign_ecdsa(&parent_msg, &parent_privkey.inner);

          let mut parent_sig_bytes = parent_sig.serialize_der().to_vec();
          parent_sig_bytes.push(EcdsaSighashType::All as u8);

          tx.input[input_idx].script_sig = script::Builder::new()
            .push_slice(&parent_sig_bytes)
            .push_slice(&parent_privkey.public_key(secp).to_bytes())
            .into_script();
        }
      }
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

    reveals.push(RevealTx {
      index: j,
      txid: last_txid,
      raw_hex: hex::encode(bitcoin::consensus::encode::serialize(&tx)),
      broadcast: false,
      confirmed: false,
    });
  }

  let inscription_id = InscriptionId {
    txid: reveals[0].txid,
    index: 0,
  };

  Ok(SignedReveal {
    reveals,
    inscription_id,
    last_txid,
  })
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

    let mut utxos = wallet
      .utxos()
      .iter()
      .map(|(outpoint, txout)| (*outpoint, Amount::from_sat(txout.value)))
      .collect::<BTreeMap<OutPoint, Amount>>();

    // Add a large fake UTXO for dry-run so transaction building succeeds
    // regardless of wallet balance.
    if self.dry_run {
      utxos.insert(OutPoint::null(), Amount::from_sat(1_000_000_000_000));
    }

    let existing_inscriptions = wallet
      .inscriptions()
      .iter()
      .map(|(sp, ids)| (*sp, ids[0]))
      .collect::<BTreeMap<SatPoint, InscriptionId>>();

    if let Some(batch_path) = &self.batch {
      let batch_file = BatchFile::load(batch_path)?;

      // Validate all parents exist in wallet and exclude their UTXOs from funding
      let batch_parent_infos: Vec<ParentInfo> = batch_file
        .parents
        .iter()
        .map(|parent_id| {
          let info = Self::get_parent_info(parent_id, &wallet)?;
          utxos.remove(&info.location.outpoint);
          Ok(info)
        })
        .collect::<Result<Vec<_>>>()?;

      // Load all inscriptions once
      let mut inscriptions = Vec::new();
      let mut destinations = Vec::new();
      let default_address = get_change_address(client)?;

      for entry in &batch_file.inscriptions {
        let (mut inscription, path, delegate_id) = if let Some(ref file) = entry.file {
          let path = if file.is_absolute() {
            file.clone()
          } else {
            batch_path.parent().unwrap().join(file)
          };
          (
            Inscription::from_file(wallet.chain(), &path)?,
            Some(path),
            None,
          )
        } else if let Some(delegate_id) = entry.delegate {
          let delegate = wallet
            .get_inscription(delegate_id)?
            .ok_or_else(|| anyhow!("delegate {delegate_id} not found"))?;

          if delegate.delegate.is_some() {
            bail!("delegate {delegate_id} is itself a delegate");
          }

          let mut inscription = Inscription::new(None, None, BTreeMap::new());
          inscription.tags.insert(
            crate::inscriptions::tag::DELEGATE.to_string(),
            vec![crate::inscriptions::tag::encode_inscription_id(
              &delegate_id,
            )],
          );
          (inscription, None, Some(delegate_id))
        } else {
          unreachable!()
        };

        // Add parent tags from batch file
        for parent_id in &batch_file.parents {
          inscription
            .tags
            .entry(crate::inscriptions::tag::PARENT.to_string())
            .or_default()
            .push(crate::inscriptions::tag::encode_inscription_id(parent_id));
        }

        inscriptions.push((inscription, path, delegate_id));
        destinations.push(
          entry
            .destination
            .clone()
            .unwrap_or_else(|| default_address.clone()),
        );
      }

      // Pre-flight balance check
      let mut total_postage = 0;
      let mut total_reveal_fees = 0;
      for (inscription, _, _) in &inscriptions {
        total_postage += postage.to_sat();
        let batches = split_inscription_into_batches(inscription);
        for batch in &batches {
          let num_chunks = batch.instructions().count();
          let estimated_sig_size = batch.len() + 1 + 72 + 1 + (33 + 1 + num_chunks + 1);
          let tx_vsize = 82 + estimated_sig_size;
          total_reveal_fees += fee_rate.fee(tx_vsize).to_sat();
        }
      }

      let num_chunks = inscriptions.len().div_ceil(BATCH_COMMIT_CHUNK_SIZE);
      let estimated_commit_fees = num_chunks as u64 * commit_fee_rate.fee(200).to_sat();
      let total_required = total_postage + total_reveal_fees + estimated_commit_fees;
      let available: u64 = utxos.values().map(|a| a.to_sat()).sum();

      if !self.dry_run && available < total_required {
        bail!(
          "insufficient funds for batch inscription\n  required: {:.2} PEP ({} inscriptions in {} chunks)\n  available: {:.2} PEP",
          total_required as f64 / 100_000_000.0,
          inscriptions.len(),
          num_chunks,
          available as f64 / 100_000_000.0
        );
      }

      let mut all_chunk_results = Vec::new();
      let mut total_fees = 0;
      let mut total_reveal_count = 0;
      let mut used_utxos = BTreeSet::new();

      for (chunk_idx, chunk_data) in inscriptions
        .chunks(BATCH_COMMIT_CHUNK_SIZE)
        .zip(destinations.chunks(BATCH_COMMIT_CHUNK_SIZE))
        .enumerate()
      {
        let (chunk_inscriptions_with_metadata, chunk_destinations) = chunk_data;
        let chunk_inscriptions: Vec<Inscription> = chunk_inscriptions_with_metadata
          .iter()
          .map(|(ins, _, _)| ins.clone())
          .collect();

        let chunk_utxos: BTreeMap<OutPoint, Amount> = utxos
          .iter()
          .filter(|(op, _)| !used_utxos.contains(*op))
          .map(|(op, amount)| (*op, *amount))
          .collect();

        match create_batch_inscription_transactions(
          chunk_inscriptions,
          chunk_destinations.to_vec(),
          existing_inscriptions.clone(),
          wallet.chain().network(),
          chunk_utxos,
          [wallet.get_address(true)?, wallet.get_address(true)?],
          commit_fee_rate,
          fee_rate,
          pubkey,
          postage,
          &batch_parent_infos,
        ) {
          Ok((commit_tx, reveal_chains, fees)) => {
            for input in &commit_tx.input {
              used_utxos.insert(input.previous_output);
            }
            total_fees += fees;
            total_reveal_count += reveal_chains.iter().map(|c| c.len()).sum::<usize>();
            all_chunk_results.push((
              commit_tx,
              reveal_chains,
              chunk_destinations.to_vec(),
              chunk_inscriptions_with_metadata,
              fees,
            ));
          }
          Err(e) => {
            if chunk_idx == 0 {
              return Err(e);
            } else {
              bail!("insufficient funds for batch inscription\n  required: more than current available (failed at chunk {})\n  total inscriptions: {}\n  processed chunks: {}", chunk_idx + 1, inscriptions.len(), chunk_idx);
            }
          }
        }
      }

      if self.dry_run {
        let mut inscription_outputs = Vec::new();
        for (_commit_tx, reveal_chains, chunk_destinations, _, _) in &all_chunk_results {
          for (i, chain) in reveal_chains.iter().enumerate() {
            inscription_outputs.push(InscriptionOutput {
              inscription: chain[0].tx.txid().into(),
              reveal: chain.last().unwrap().tx.txid(),
              destination: chunk_destinations[i].clone(),
            });
          }
        }

        let broadcast_rounds = total_reveal_count.div_ceil(MEMPOOL_CHAIN_LIMIT);

        #[derive(Serialize)]
        struct BatchDryRunOutput {
          commit: Txid,
          commit_transactions: usize,
          inscriptions: Vec<InscriptionOutput>,
          total_fees: u64,
          reveal_count: usize,
          broadcast_rounds: usize,
        }

        let output = BatchDryRunOutput {
          commit: all_chunk_results[0].0.txid(),
          commit_transactions: all_chunk_results.len(),
          inscriptions: inscription_outputs,
          total_fees,
          reveal_count: total_reveal_count,
          broadcast_rounds,
        };

        print_json(&output)?;
      } else {
        let mut all_jobs = Vec::new();
        let secp = Secp256k1::new();
        let mut inscription_outputs = Vec::new();
        let mut current_parent_outpoints: Vec<OutPoint> = batch_parent_infos
          .iter()
          .map(|pi| pi.location.outpoint)
          .collect();

        for (
          commit_tx,
          reveal_chains,
          chunk_destinations,
          chunk_inscriptions_with_metadata,
          fees,
        ) in all_chunk_results
        {
          let signed_commit_tx = LocalSigner::sign_transaction(&wallet, commit_tx)?;
          let commit_txid = signed_commit_tx.txid();
          let commit_raw_hex =
            hex::encode(bitcoin::consensus::encode::serialize(&signed_commit_tx));

          for (i, chain) in reveal_chains.into_iter().enumerate() {
            let commit_out = OutPoint {
              txid: commit_txid,
              vout: u32::try_from(i).unwrap(),
            };
            let parent_entries: Vec<(&ParentInfo, OutPoint)> = batch_parent_infos
              .iter()
              .zip(current_parent_outpoints.iter())
              .map(|(pi, po)| (pi, *po))
              .collect();
            let parent_signing = if parent_entries.is_empty() {
              None
            } else {
              Some((&wallet, parent_entries.as_slice()))
            };
            let signed = sign_reveal_chain(chain, commit_out, &privkey, &secp, parent_signing)?;

            // Update parent outpoints: after this chain's first reveal tx,
            // parent inscriptions are at output[1], output[2], etc.
            for (idx, _) in batch_parent_infos.iter().enumerate() {
              current_parent_outpoints[idx] = OutPoint {
                txid: signed.reveals[0].txid,
                vout: u32::try_from(idx + 1).unwrap(),
              };
            }

            inscription_outputs.push(InscriptionOutput {
              inscription: signed.inscription_id,
              reveal: signed.last_txid,
              destination: chunk_destinations[i].clone(),
            });

            let (_ins, path, delegate_id) = &chunk_inscriptions_with_metadata[i];

            all_jobs.push(RevealJob {
              file_name: path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| delegate_id.unwrap().to_string()),
              content_type: path
                .as_ref()
                .and_then(|p| Media::content_type_for_path(p).ok())
                .unwrap_or("application/octet-stream")
                .to_string(),
              file_size: path
                .as_ref()
                .and_then(|p| fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0),
              commit_txid,
              commit_raw_hex: commit_raw_hex.clone(),
              commit_broadcast: false,
              commit_confirmed: false,
              inscription_id: signed.inscription_id,
              destination: chunk_destinations[i].clone(),
              total_fees: fees,
              created_at: Utc::now(),
              reveals: signed.reveals,
              parent_ids: batch_parent_infos.iter().map(|pi| pi.id).collect(),
              delegate_id: *delegate_id,
            });
          }
        }

        let batch_name = sanitize_batch_name(batch_path.file_stem().unwrap().to_str().unwrap());
        let timestamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
        let batch_dir = RevealJob::jobs_dir(wallet.settings(), wallet.name())
          .join(format!("{}-{}", batch_name, timestamp));

        fs::create_dir_all(&batch_dir)?;
        fs::copy(batch_path, batch_dir.join("batch.yaml"))?;

        let mut broadcasted_commits = BTreeSet::new();
        for job in &mut all_jobs {
          if num_chunks == 1 {
            if !broadcasted_commits.contains(&job.commit_txid) {
              job.broadcast_commit(client);
              broadcasted_commits.insert(job.commit_txid);
            } else {
              job.commit_broadcast = true;
            }
            job.broadcast_batch(client);
          }
          let job_file = batch_dir.join(format!("{}.json", job.inscription_id.txid));
          job.save(&job_file)?;
        }

        print_json(BatchOutput {
          commit: all_jobs[0].commit_txid,
          inscriptions: inscription_outputs,
          total_fees,
        })?;

        log::info!("Batch inscription jobs created in: {}", batch_dir.display());
        if num_chunks > 1 {
          log::info!("Run 'ordpep wallet broadcast' or start the server to begin broadcasting.");
        }
      }
    } else {
      let mut inscription = if let Some(ref file) = self.file {
        Inscription::from_file(wallet.chain(), file)?
      } else if let Some(delegate_id) = self.delegate {
        let delegate = wallet
          .get_inscription(delegate_id)?
          .ok_or_else(|| anyhow!("delegate {delegate_id} not found"))?;

        if delegate.delegate.is_some() {
          bail!("delegate {delegate_id} is itself a delegate");
        }

        let mut inscription = Inscription::new(None, None, BTreeMap::new());
        inscription.tags.insert(
          crate::inscriptions::tag::DELEGATE.to_string(),
          vec![crate::inscriptions::tag::encode_inscription_id(
            &delegate_id,
          )],
        );
        inscription
      } else {
        unreachable!()
      };

      let parent_info = if let Some(ref parent_id) = self.parent {
        let info = Self::get_parent_info(parent_id, &wallet)?;
        inscription.tags.insert(
          crate::inscriptions::tag::PARENT.to_string(),
          vec![crate::inscriptions::tag::encode_inscription_id(parent_id)],
        );
        // Exclude parent UTXO from available funding UTXOs
        utxos.remove(&info.location.outpoint);
        Some(info)
      } else {
        None
      };

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
        parent_info.as_ref(),
      )?;

      let reveal_count = txs.len() - 1; // exclude commit tx

      if self.dry_run {
        let inscription_id = txs[1].txid().into();
        let broadcast_rounds = reveal_count.div_ceil(MEMPOOL_CHAIN_LIMIT);

        let dry_dir = wallet
          .settings()
          .data_dir()
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
          broadcast_rounds,
        };

        serde_json::to_writer_pretty(fs::File::create(&dry_file)?, &output)?;
        print_json(&output)?;
      } else {
        let signed_commit_tx = LocalSigner::sign_transaction(&wallet, txs[0].clone())?;
        let commit_raw_hex = hex::encode(bitcoin::consensus::encode::serialize(&signed_commit_tx));
        let commit = signed_commit_tx.txid();

        client
          .send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_commit_tx))
          .context("Failed to send commit transaction")?;

        let secp = Secp256k1::new();
        let chain: Vec<super::batch::plan::RevealTx> = txs[1..]
          .iter()
          .zip(scripts.iter())
          .map(
            |(tx, (redeem_script, partial_script))| super::batch::plan::RevealTx {
              tx: tx.clone(),
              redeem_script: redeem_script.clone(),
              partial_script: partial_script.clone(),
            },
          )
          .collect();

        let parent = OutPoint {
          txid: commit,
          vout: 0,
        };
        let parent_entries: Vec<(&ParentInfo, OutPoint)> = parent_info
          .as_ref()
          .map(|pi| vec![(pi, pi.location.outpoint)])
          .unwrap_or_default();
        let parent_signing = if parent_entries.is_empty() {
          None
        } else {
          Some((&wallet, parent_entries.as_slice()))
        };
        let signed = sign_reveal_chain(chain, parent, &privkey, &secp, parent_signing)?;

        let jobs_dir = RevealJob::jobs_dir(wallet.settings(), wallet.name());
        fs::create_dir_all(&jobs_dir)?;
        let job_file = jobs_dir.join(format!("{}.json", commit));

        let mut job = RevealJob {
          file_name: self
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.delegate.unwrap().to_string()),
          content_type: self
            .file
            .as_ref()
            .and_then(|p| Media::content_type_for_path(p).ok())
            .unwrap_or("application/octet-stream")
            .to_string(),
          file_size: self
            .file
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0),
          commit_txid: commit,
          commit_raw_hex,
          commit_broadcast: true,
          commit_confirmed: false,
          inscription_id: signed.inscription_id,
          destination: reveal_tx_destination.clone(),
          total_fees: fees,
          created_at: Utc::now(),
          reveals: signed.reveals,
          parent_ids: self.parent.iter().cloned().collect(),
          delegate_id: self.delegate,
        };

        job.broadcast_batch(client);
        job.save(&job_file)?;

        print_json(Output {
          commit,
          reveal: signed.last_txid,
          inscription: signed.inscription_id,
          destination: reveal_tx_destination,
          fees,
        })?;
      }
    }

    Ok(())
  }

  fn get_parent_info(parent_id: &InscriptionId, wallet: &Wallet) -> Result<ParentInfo> {
    let inscriptions = wallet.inscriptions();
    let location = inscriptions
      .iter()
      .find_map(|(satpoint, ids)| {
        if ids.contains(parent_id) {
          Some(*satpoint)
        } else {
          None
        }
      })
      .ok_or_else(|| anyhow!("parent inscription {parent_id} not found in wallet"))?;

    let tx_out = wallet
      .utxos()
      .get(&location.outpoint)
      .ok_or_else(|| anyhow!("parent UTXO {} not found in wallet", location.outpoint))?
      .clone();

    let destination = Address::from_script(&tx_out.script_pubkey, wallet.chain().network())
      .context("could not derive address from parent UTXO")?;

    Ok(ParentInfo {
      id: *parent_id,
      location,
      tx_out,
      destination,
    })
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
    parent_info: Option<&ParentInfo>,
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
      vout: u32::try_from(
        unsigned_commit_tx
          .output
          .iter()
          .position(|o| o.script_pubkey == first_lock_address.script_pubkey())
          .unwrap(),
      )
      .unwrap(),
    };
    let mut last_value = unsigned_commit_tx.output[last_outpoint.vout as usize].value;
    txs.push(unsigned_commit_tx);

    for (i, batch) in batches.into_iter().enumerate() {
      let is_last = i == reveal_fees.len() - 1;
      let fee = reveal_fees[i];
      let next_value = last_value.checked_sub(fee).unwrap();

      let mut inputs = vec![TxIn {
        previous_output: last_outpoint,
        script_sig: Script::new(),
        witness: Witness::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
      }];

      let mut outputs = vec![TxOut {
        script_pubkey: if is_last {
          destination.script_pubkey()
        } else {
          Address::p2sh(&locks[i + 1], network)
            .unwrap()
            .script_pubkey()
        },
        value: next_value,
      }];

      // Add parent UTXO as input and return output on the first reveal tx
      if i == 0 {
        if let Some(parent) = parent_info {
          inputs.push(TxIn {
            previous_output: parent.location.outpoint,
            script_sig: Script::new(),
            witness: Witness::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          });
          outputs.push(TxOut {
            script_pubkey: parent.destination.script_pubkey(),
            value: parent.tx_out.value,
          });
        }
      }

      let reveal_tx = Transaction {
        input: inputs,
        output: outputs,
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
