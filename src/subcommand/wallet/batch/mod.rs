use super::*;

pub(crate) mod file;
pub(crate) mod plan;

#[derive(Serialize, Deserialize)]
pub(crate) struct BatchOutput {
  pub(crate) commit: Txid,
  pub(crate) inscriptions: Vec<InscriptionOutput>,
  pub(crate) total_fees: u64,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct InscriptionOutput {
  pub(crate) inscription: InscriptionId,
  pub(crate) reveal: Txid,
  pub(crate) destination: Address,
}
