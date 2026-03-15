use {
  super::*,
  crate::wallet::{
    signer::LocalSigner,
    Wallet,
  },
};

#[derive(Debug, Parser)]
pub(crate) struct Send {
  address: Address,
  outgoing: Option<Outgoing>,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB. [default: 1000.0]")]
  fee_rate: Option<FeeRate>,
  #[clap(long, help = "Use postage of <POSTAGE> sats. [default: 100000]")]
  postage: Option<Amount>,
  #[clap(long, help = "Send all cardinal (non-inscription) UTXOs, minus fees.")]
  max: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub transaction: Txid,
}

/// Try LocalSigner first, fall back to Core's signrawtransaction for
/// UTXOs at legacy Core-managed addresses.
fn sign_transaction(wallet: &Wallet, unsigned_transaction: Transaction) -> Result<Transaction> {
  match LocalSigner::sign_transaction(wallet, unsigned_transaction.clone()) {
    Ok(signed) => Ok(signed),
    Err(_) => {
      let client = wallet.bitcoin_client();
      let tx_hex = bitcoin::consensus::encode::serialize_hex(&unsigned_transaction);
      let result: serde_json::Value = client
        .call("signrawtransaction", &[tx_hex.into()])
        .context("failed to sign transaction")?;
      let signed_hex = result["hex"]
        .as_str()
        .ok_or_else(|| anyhow!("missing hex in signrawtransaction response"))?;
      if result["complete"].as_bool() != Some(true) {
        bail!("Failed to sign transaction: {}", result["errors"]);
      }
      let signed_tx: Transaction = bitcoin::consensus::deserialize(&hex::decode(signed_hex)?)
        .context("failed to deserialize signed transaction")?;
      Ok(signed_tx)
    }
  }
}

impl Send {
  pub(crate) fn run(self, wallet: Wallet) -> Result {
    if !self.address.is_valid_for_network(wallet.chain().network()) {
      bail!(
        "Address `{}` is not valid for {}",
        self.address,
        wallet.chain()
      );
    }

    let client = wallet.bitcoin_client();

    if self.max {
      if self.outgoing.is_some() {
        bail!("cannot specify both an amount and --max");
      }
      return self.send_max(&wallet, client);
    }

    let outgoing = self.outgoing.ok_or_else(|| anyhow!("must specify an amount, inscription ID, or satpoint (or use --max)"))?;

    let satpoint = match outgoing {
      Outgoing::SatPoint(satpoint) => {
        for inscription_satpoint in wallet.inscriptions().keys() {
          if satpoint == *inscription_satpoint {
            bail!("inscriptions must be sent by inscription ID");
          }
        }
        satpoint
      }
      Outgoing::InscriptionId(id) => wallet
        .inscription_info()
        .get(&id)
        .map(|info| info.satpoint)
        .ok_or_else(|| anyhow!("Inscription {id} not found"))?,
      Outgoing::Amount(amount) => {
        let inscribed_outputs = wallet
          .inscriptions()
          .keys()
          .map(|satpoint| satpoint.outpoint)
          .collect::<HashSet<OutPoint>>();

        let fee_rate = self.fee_rate.unwrap_or(FeeRate::try_from(wallet.chain().default_fee_rate()).unwrap());
        let change_address = wallet.get_address(true)?;

        // Select cardinal (non-inscribed) UTXOs
        let mut cardinal_utxos: Vec<(OutPoint, Amount)> = wallet
          .utxos()
          .iter()
          .filter(|(op, _)| !inscribed_outputs.contains(op))
          .map(|(op, txo)| (*op, Amount::from_sat(txo.value)))
          .collect();

        // Sort largest first for simple coin selection
        cardinal_utxos.sort_by(|a, b| b.1.cmp(&a.1));

        let mut selected = Vec::new();
        let mut selected_amount = Amount::ZERO;

        for (outpoint, utxo_amount) in &cardinal_utxos {
          selected.push(*outpoint);
          selected_amount += *utxo_amount;
          if selected_amount >= amount {
            break;
          }
        }

        if selected_amount < amount {
          bail!("wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.");
        }

        // Estimate fee: ~148 bytes per P2PKH input, ~34 per output, ~10 overhead
        let estimated_vsize = selected.len() * 148 + 2 * 34 + 10;
        let fee = fee_rate.fee(estimated_vsize);

        let total_needed = amount.checked_add(fee)
          .ok_or_else(|| anyhow!("overflow calculating total amount + fee"))?;

        // Re-select if we need more for fees
        if selected_amount < total_needed {
          for (outpoint, utxo_amount) in &cardinal_utxos {
            if selected.contains(outpoint) {
              continue;
            }
            selected.push(*outpoint);
            selected_amount += *utxo_amount;
            if selected_amount >= total_needed {
              break;
            }
          }

          if selected_amount < total_needed {
            bail!("wallet does not contain enough cardinal UTXOs, please add additional funds to wallet.");
          }
        }

        let change_amount = selected_amount.checked_sub(total_needed)
          .ok_or_else(|| anyhow!("insufficient funds for fee"))?;

        let mut tx_outputs = vec![TxOut {
          value: amount.to_sat(),
          script_pubkey: self.address.script_pubkey(),
        }];

        let dust_limit = change_address.script_pubkey().dust_value();
        if change_amount >= dust_limit {
          tx_outputs.push(TxOut {
            value: change_amount.to_sat(),
            script_pubkey: change_address.script_pubkey(),
          });
        }

        let unsigned_transaction = Transaction {
          version: 1,
          lock_time: bitcoin::PackedLockTime::ZERO,
          input: selected
            .iter()
            .map(|outpoint| TxIn {
              previous_output: *outpoint,
              script_sig: Script::new(),
              sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
              witness: bitcoin::Witness::default(),
            })
            .collect(),
          output: tx_outputs,
        };

        let signed_tx = sign_transaction(&wallet, unsigned_transaction)?;
        let txid = client.send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_tx))?;

        print_json(Output { transaction: txid })?;
        return Ok(());
      }
    };

    let change = [wallet.get_address(true)?, wallet.get_address(true)?];

    let fee_rate = self.fee_rate.unwrap_or(FeeRate::try_from(wallet.chain().default_fee_rate()).unwrap());

    let min = wallet.chain().min_fee_rate();
    if fee_rate < FeeRate::try_from(min).unwrap() {
      bail!("fee rate must be at least {min} sat/vB (Pepecoin minimum relay fee)");
    }

    let postage = self.postage.unwrap_or(wallet.chain().default_postage());

    let unsigned_transaction = TransactionBuilder::build_transaction_with_postage(
      satpoint,
      wallet.inscriptions().iter().map(|(sp, ids)| (*sp, ids[0])).collect(),
      wallet.utxos().iter().map(|(op, txo)| (*op, Amount::from_sat(txo.value))).collect(),
      self.address,
      change,
      fee_rate,
      postage,
    )?;

    let signed_tx = sign_transaction(&wallet, unsigned_transaction)?;

    let txid = client.send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_tx))?;

    println!("{txid}");

    Ok(())
  }

  fn send_max(self, wallet: &Wallet, client: &Client) -> Result {
    let inscribed_outputs = wallet
      .inscriptions()
      .keys()
      .map(|satpoint| satpoint.outpoint)
      .collect::<HashSet<OutPoint>>();

    let fee_rate = self.fee_rate.unwrap_or(FeeRate::try_from(wallet.chain().default_fee_rate()).unwrap());

    // Select ALL cardinal (non-inscribed) UTXOs
    let cardinal_utxos: Vec<(OutPoint, Amount)> = wallet
      .utxos()
      .iter()
      .filter(|(op, _)| !inscribed_outputs.contains(op))
      .map(|(op, txo)| (*op, Amount::from_sat(txo.value)))
      .collect();

    if cardinal_utxos.is_empty() {
      bail!("wallet contains no cardinal UTXOs to send");
    }

    let total_amount: Amount = cardinal_utxos.iter().map(|(_, a)| *a).sum();

    // Estimate fee: ~148 bytes per P2PKH input, ~34 for single output (no change), ~10 overhead
    let estimated_vsize = cardinal_utxos.len() * 148 + 34 + 10;
    let fee = fee_rate.fee(estimated_vsize);

    let send_amount = total_amount.checked_sub(fee)
      .ok_or_else(|| anyhow!("cardinal balance ({}) is less than the estimated fee ({})", total_amount, fee))?;

    if send_amount.to_sat() == 0 {
      bail!("cardinal balance ({}) is too small to cover fees", total_amount);
    }

    let dust_limit = self.address.script_pubkey().dust_value();
    if send_amount < dust_limit {
      bail!("send amount ({}) after fees is below dust limit ({})", send_amount, dust_limit);
    }

    let unsigned_transaction = Transaction {
      version: 1,
      lock_time: bitcoin::PackedLockTime::ZERO,
      input: cardinal_utxos
        .iter()
        .map(|(outpoint, _)| TxIn {
          previous_output: *outpoint,
          script_sig: Script::new(),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          witness: bitcoin::Witness::default(),
        })
        .collect(),
      output: vec![TxOut {
        value: send_amount.to_sat(),
        script_pubkey: self.address.script_pubkey(),
      }],
    };

    let signed_tx = sign_transaction(wallet, unsigned_transaction)?;
    let txid = client.send_raw_transaction(&bitcoin::consensus::encode::serialize(&signed_tx))?;

    print_json(Output { transaction: txid })?;

    Ok(())
  }
}
