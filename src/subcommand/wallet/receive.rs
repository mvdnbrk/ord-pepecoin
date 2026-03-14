use {super::*, crate::wallet::Wallet};

#[derive(Deserialize, Serialize)]
pub struct Output {
  pub address: Address,
}

pub(crate) fn run(wallet: Wallet) -> Result {
  let address = wallet.get_address(false)?;

  print_json(Output { address })?;

  Ok(())
}
