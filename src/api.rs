use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Inscription {
  pub address: Option<String>,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub genesis_fee: u64,
  pub genesis_height: u64,
  pub genesis_transaction: Txid,
  pub inscription_id: InscriptionId,
  pub location: SatPoint,
  pub number: u64,
  pub output_value: Option<u64>,
  pub timestamp: i64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Inscriptions {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page_index: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
  pub address: Option<String>,
  pub confirmations: u32,
  pub indexed: bool,
  pub inscriptions: Vec<InscriptionId>,
  pub outpoint: OutPoint,
  pub sat_ranges: Option<Vec<(u64, u64)>>,
  pub script_pubkey: String,
  pub spent: bool,
  pub transaction: Txid,
  pub value: u64,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Address {
  pub inscriptions: Vec<InscriptionId>,
  pub outputs: Vec<OutPoint>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
  pub address_index: bool,
  pub chain: String,
  pub height: Option<u64>,
  pub inscriptions: u64,
  pub sat_index: bool,
  pub unrecoverably_reorged: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
  pub hash: BlockHash,
  pub target: String,
  pub best_block: bool,
  pub height: u64,
  pub chainweight: Option<usize>,
  pub mediantime: i64,
  pub nonce: u32,
  pub bits: String,
  pub difficulty: f64,
  pub chainwork: String,
  pub confirmations: i32,
  pub previousblockhash: Option<BlockHash>,
  pub nextblockhash: Option<BlockHash>,
  pub inscriptions: Vec<InscriptionId>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OutputInfo {
  pub txout: TxOut,
  pub indexed: bool,
  pub spent: bool,
  pub confirmations: u32,
  pub sat_ranges: Option<Vec<(u64, u64)>>,
  pub inscriptions: Vec<InscriptionId>,
}
