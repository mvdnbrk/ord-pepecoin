use {super::*, crate::wallet::Wallet};

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

    let addresses = wallet.addresses()?;
    let address_set: HashSet<Address> = addresses.into_iter().collect();

    for tx in wallet.bitcoin_client().list_transactions(
      None,
      Some(self.limit.unwrap_or(u16::MAX).into()),
      None,
      None,
    )? {
      let belongs_to_wallet = match &tx.detail.address {
        Some(address) => address_set.contains(address),
        None => false,
      };

      if belongs_to_wallet && seen.insert(tx.info.txid) {
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
