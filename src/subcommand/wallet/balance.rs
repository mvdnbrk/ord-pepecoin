use {super::*, crate::wallet::Wallet};

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub cardinal: u64,
}

pub(crate) fn run(wallet: Wallet) -> Result {
  let inscribed_outputs = wallet
    .inscriptions()
    .keys()
    .map(|satpoint| satpoint.outpoint)
    .collect::<HashSet<OutPoint>>();

  let mut balance = 0;
  for (outpoint, txout) in wallet.utxos() {
    if !inscribed_outputs.contains(outpoint) {
      balance += txout.value;
    }
  }

  print_json(Output { cardinal: balance })?;

  Ok(())
}
