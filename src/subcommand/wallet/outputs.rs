use {super::*, crate::wallet::Wallet};

#[derive(Debug, Parser)]
pub(crate) struct Outputs {
  #[clap(short, long, help = "Show list of sat <RANGES> in outputs.")]
  pub(crate) ranges: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub output: OutPoint,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub address: Option<Address>,
  pub amount: u64,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub inscriptions: Option<Vec<InscriptionId>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sat_ranges: Option<Vec<String>>,
}

impl Outputs {
  pub(crate) fn run(&self, wallet: Wallet) -> Result {
    let mut outputs = Vec::new();

    for (outpoint, txout) in wallet.utxos() {
      let address = Address::from_script(&txout.script_pubkey, wallet.chain().network()).ok();

      let inscriptions: Vec<InscriptionId> = wallet
        .inscription_info()
        .values()
        .filter(|info| info.satpoint.outpoint == *outpoint)
        .map(|info| info.id)
        .collect();

      let inscriptions = if inscriptions.is_empty() {
        None
      } else {
        Some(inscriptions)
      };

      let sat_ranges = if self.ranges {
        wallet
          .get_unspent_output_ranges()?
          .iter()
          .find(|(op, _)| op == outpoint)
          .map(|(_, ranges)| {
            ranges
              .iter()
              .map(|(start, end)| format!("{start}-{end}"))
              .collect()
          })
      } else {
        None
      };

      outputs.push(Output {
        output: *outpoint,
        address,
        amount: txout.value,
        inscriptions,
        sat_ranges,
      });
    }

    print_json(outputs)?;

    Ok(())
  }
}
