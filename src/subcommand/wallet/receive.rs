use super::*;

#[derive(Deserialize, Serialize)]
pub struct Output {
  pub address: Address,
}

pub(crate) fn run(options: Options) -> Result {
  let client = options.pepecoin_rpc_client_for_wallet_command(false)?;
  let address: Address = client.call("getnewaddress", &[])?;

  print_json(Output { address })?;

  Ok(())
}
