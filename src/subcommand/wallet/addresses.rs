use {super::*, crate::wallet::Wallet};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Output {
  pub output: OutPoint,
  pub amount: u64,
  pub inscriptions: Vec<InscriptionId>,
}

pub(crate) fn run(wallet: Wallet) -> Result {
  let mut addresses: BTreeMap<Address, Vec<Output>> = BTreeMap::new();

  for (outpoint, txout) in wallet.utxos() {
    let address = Address::from_script(&txout.script_pubkey, wallet.chain().network())
      .context("failed to derive address from script pubkey")?;

    let inscriptions = wallet
      .inscription_info()
      .values()
      .filter(|info| info.satpoint.outpoint == *outpoint)
      .map(|info| info.id)
      .collect();

    let output = Output {
      output: *outpoint,
      amount: txout.value,
      inscriptions,
    };

    addresses.entry(address).or_default().push(output);
  }

  print_json(&addresses)?;

  Ok(())
}
