use {super::*, crate::wallet::Wallet};

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub inscription: InscriptionId,
  pub location: SatPoint,
  pub explorer: String,
  pub postage: u64,
  pub address: String,
}

pub(crate) fn run(wallet: Wallet) -> Result {
  let explorer = match wallet.chain() {
    Chain::Mainnet => "https://peppool.space/inscription/",
    Chain::Regtest => "http://localhost/inscription/",
    Chain::Signet => "https://signet.peppool.space/inscription/",
    Chain::Testnet => "https://testnet.peppool.space/inscription/",
  };

  let mut output = Vec::new();

  for (location, inscriptions) in wallet.inscriptions() {
    if let Some(txout) = wallet.utxos().get(&location.outpoint) {
      for inscription in inscriptions {
        let address = Address::from_script(&txout.script_pubkey, wallet.chain().network())
          .map(|a| a.to_string())
          .unwrap_or_default();
        output.push(Output {
          location: *location,
          inscription: *inscription,
          explorer: format!("{explorer}{inscription}"),
          postage: txout.value,
          address,
        });
      }
    }
  }

  print_json(&output)?;

  Ok(())
}
