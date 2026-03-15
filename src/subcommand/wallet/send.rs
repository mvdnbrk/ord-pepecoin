use {super::*, crate::wallet::Wallet};

#[derive(Debug, Parser)]
pub(crate) struct Send {
  address: Address,
  outgoing: Outgoing,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB. [default: 1000.0]")]
  fee_rate: Option<FeeRate>,
  #[clap(long, help = "Use postage of <POSTAGE> sats. [default: 100000]")]
  postage: Option<Amount>,
}

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub transaction: Txid,
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

    let satpoint = match self.outgoing {
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

        let wallet_inscription_outputs = wallet
          .utxos()
          .keys()
          .filter(|utxo| inscribed_outputs.contains(utxo))
          .cloned()
          .collect::<Vec<OutPoint>>();

        if !client.lock_unspent(&wallet_inscription_outputs)? {
          bail!("failed to lock ordinal UTXOs");
        }

        let txid =
          client.send_to_address(&self.address, amount, None, None, None, None, None, None)?;

        print_json(Output { transaction: txid })?;

        return Ok(());
      }
    };

    let change = [get_change_address(client)?, get_change_address(client)?];

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
    let signed_tx = hex::decode(signed_hex)?;

    let txid = client.send_raw_transaction(&signed_tx)?;

    println!("{txid}");

    Ok(())
  }
}
