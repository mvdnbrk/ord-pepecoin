use {
  super::*,
  crate::wallet::Wallet,
};

#[derive(Debug, Parser)]
pub(crate) struct Transactions {
  #[clap(long, help = "Fetch at most <LIMIT> transactions.")]
  limit: Option<u16>,
}

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub transaction: Txid,
  pub confirmations: i32,
}

impl Transactions {
  pub(crate) fn run(self, wallet: Wallet) -> Result {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    for tx in wallet
      .bitcoin_client()
      .list_transactions(
        None,
        Some(self.limit.unwrap_or(u16::MAX).into()),
        None,
        None,
      )?
    {
      if seen.insert(tx.info.txid) {
        output.push(Output {
          transaction: tx.info.txid,
          confirmations: tx.info.confirmations,
        });
      }
    }

    print_json(output)?;

    Ok(())
  }
}
