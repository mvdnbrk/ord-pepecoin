use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Inscription {
  pub address: Option<String>,
  pub children: Vec<InscriptionId>,
  pub child_count: u64,
  pub content_length: Option<usize>,
  pub content_type: Option<String>,
  pub fee: u64,
  pub height: u32,
  pub id: InscriptionId,
  pub next: Option<InscriptionId>,
  pub number: u32,
  pub parents: Vec<InscriptionId>,
  pub previous: Option<InscriptionId>,
  pub sat: Option<Sat>,
  pub satpoint: SatPoint,
  pub timestamp: i64,
  pub value: Option<u64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct InscriptionIds {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page: usize,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Inscriptions {
  pub ids: Vec<InscriptionId>,
  pub more: bool,
  pub page_index: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
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

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Address {
  pub inscriptions: Vec<InscriptionId>,
  pub outputs: Vec<OutPoint>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Status {
  pub address_index: bool,
  pub chain: String,
  pub height: Option<u32>,
  pub index_size: u64,
  pub inscriptions: u64,
  pub sat_index: bool,
  pub unrecoverably_reorged: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Block {
  pub hash: BlockHash,
  pub target: String,
  pub best_block: bool,
  pub height: u32,
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

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct OutputInfo {
  pub txout: TxOut,
  pub indexed: bool,
  pub spent: bool,
  pub confirmations: u32,
  pub sat_ranges: Option<Vec<(u64, u64)>>,
  pub inscriptions: Vec<InscriptionId>,
}
