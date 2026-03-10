use super::*;

pub(crate) struct State {
  pub(crate) blocks: BTreeMap<BlockHash, Block>,
  pub(crate) descriptors: Vec<String>,
  pub(crate) fail_lock_unspent: bool,
  pub(crate) hashes: Vec<BlockHash>,
  pub(crate) locked: BTreeSet<OutPoint>,
  pub(crate) mempool: Vec<Transaction>,
  pub(crate) network: Network,
  pub(crate) nonce: u32,
  pub(crate) sent: Vec<Sent>,
  pub(crate) transactions: BTreeMap<Txid, Transaction>,
  pub(crate) utxos: BTreeMap<OutPoint, Amount>,
  pub(crate) version: usize,
  pub(crate) wallets: BTreeSet<String>,
  pub(crate) loaded_wallets: BTreeSet<String>,
  pub(crate) address_pubkeys: BTreeMap<Address, bitcoin::util::key::PublicKey>,
  pub(crate) address_privkeys: BTreeMap<Address, bitcoin::PrivateKey>,
  pub(crate) imported_privkeys: Vec<(String, Option<String>)>,
  pub(crate) coinbase_address: Option<Address>,
}

impl State {
  pub(crate) fn new(network: Network, version: usize, fail_lock_unspent: bool) -> Self {
    let mut hashes = Vec::new();
    let mut blocks = BTreeMap::new();

    let genesis_hex: &str = "0100000000000000000000000000000000000000000000000000000000000000000000001265bca4002feac94c0c06971f12aa8b2c82fb3e93244690d5cb399aa51b2ad2a01daf65f0ff0f1eb48506000101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff5104ffff001d01044957534a20312f32322f3234202d204665642052657669657720436c656172732043656e7472616c2042616e6b204f6666696369616c73206f662056696f6c6174696e672052756c6573ffffffff010058850c0200000043410436d04f40a76a1094ea10b14a513b62bfd0b47472dda1c25aa9cf8266e53f3c4353680146177f8a3b328ed2c6e02f2b8e051d9d5ffc61a4e6ccabd03409109a5aac00000000";
    let genesis_buf: Vec<u8> = hex::decode(genesis_hex).unwrap();
    let genesis_block: bitcoin::Block = bitcoin::consensus::deserialize(&genesis_buf).unwrap();
    let genesis_block_hash = genesis_block.block_hash();

    hashes.push(genesis_block_hash);
    blocks.insert(genesis_block_hash, genesis_block);

    Self {
      blocks,
      descriptors: Vec::new(),
      fail_lock_unspent,
      hashes,
      locked: BTreeSet::new(),
      mempool: Vec::new(),
      network,
      nonce: 0,
      sent: Vec::new(),
      transactions: BTreeMap::new(),
      utxos: BTreeMap::new(),
      version,
      wallets: BTreeSet::new(),
      loaded_wallets: BTreeSet::new(),
      address_pubkeys: BTreeMap::new(),
      address_privkeys: BTreeMap::new(),
      imported_privkeys: Vec::new(),
      coinbase_address: None,
    }
  }

  pub(crate) fn push_block(&mut self, subsidy: u64) -> Block {
    let mut fees = 0;
    for tx in &self.mempool {
        let input_value: u64 = tx.input.iter().map(|txin| {
            self.transactions.get(&txin.previous_output.txid)
                .map(|prev_tx| prev_tx.output[txin.previous_output.vout as usize].value)
                .unwrap_or(0)
        }).sum();
        let output_value: u64 = tx.output.iter().map(|txout| txout.value).sum();
        fees += input_value.saturating_sub(output_value);
        self.transactions.insert(tx.txid(), tx.clone());
    }

    let coinbase = Transaction {
      version: 0,
      lock_time: PackedLockTime(0),
      input: vec![TxIn {
        previous_output: OutPoint::null(),
        script_sig: script::Builder::new()
          .push_int(self.blocks.len().try_into().unwrap())
          .into_script(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
      }],
      output: vec![TxOut {
        value: subsidy + fees,
        script_pubkey: self
          .coinbase_address
          .as_ref()
          .map(|address| address.script_pubkey())
          .unwrap_or_else(|| Script::new()),
      }],
    };

    let coinbase_txid = coinbase.txid();
    for (i, output) in coinbase.output.iter().enumerate() {
      self.utxos.insert(
        OutPoint {
          txid: coinbase_txid,
          vout: i as u32,
        },
        Amount::from_sat(output.value),
      );
    }

    self.transactions.insert(coinbase.txid(), coinbase.clone());

    let block = Block {
      header: BlockHeader {
        version: 0,
        prev_blockhash: *self.hashes.last().unwrap(),
        merkle_root: TxMerkleNode::all_zeros(),
        time: self.blocks.len().try_into().unwrap(),
        bits: 0,
        nonce: self.nonce,
      },
      txdata: std::iter::once(coinbase)
        .chain(self.mempool.drain(0..))
        .collect(),
    };

    for tx in block.txdata.iter() {
      for input in tx.input.iter() {
        self.utxos.remove(&input.previous_output);
      }

      for (vout, txout) in tx.output.iter().enumerate() {
        self.utxos.insert(
          OutPoint {
            txid: tx.txid(),
            vout: vout.try_into().unwrap(),
          },
          Amount::from_sat(txout.value),
        );
      }
    }

    self.blocks.insert(block.block_hash(), block.clone());
    self.hashes.push(block.block_hash());
    self.nonce += 1;

    block
  }

  pub(crate) fn pop_block(&mut self) -> BlockHash {
    let blockhash = self.hashes.pop().unwrap();
    self.blocks.remove(&blockhash);

    blockhash
  }

  pub(crate) fn broadcast_tx(&mut self, template: TransactionTemplate) -> Txid {
    let mut total_value = 0;
    let mut input = Vec::new();
    for (i, (height, tx, vout)) in template.inputs.iter().enumerate() {
      let tx = &self.blocks.get(&self.hashes[*height]).unwrap().txdata[*tx];
      total_value += tx.output[*vout].value;
      input.push(TxIn {
        previous_output: OutPoint::new(tx.txid(), *vout as u32),
        script_sig: if i == 0 {
          template.script_sig.clone()
        } else {
          Script::new()
        },
        sequence: Sequence::MAX,
        witness: if i == 0 {
          template.witness.clone()
        } else {
          Witness::new()
        },
      });
    }

    let mut remaining = total_value - template.fee;
    let mut outputs = Vec::new();
    for i in 0..template.outputs {
        let value = if let Some(v) = template.output_values.get(i) {
            *v
        } else {
            remaining / (template.outputs - i) as u64
        };
        remaining -= value;
        outputs.push(TxOut {
          value,
          script_pubkey: script::Builder::new().into_script(),
        });
    }

    let tx = Transaction {
      version: 0,
      lock_time: PackedLockTime(0),
      input,
      output: outputs,
    };
    self.mempool.push(tx.clone());

    tx.txid()
  }

  pub(crate) fn mempool(&self) -> &[Transaction] {
    &self.mempool
  }

  pub(crate) fn get_confirmations(&self, tx: &Transaction) -> i32 {
    for (confirmations, hash) in self.hashes.iter().rev().enumerate() {
      if self.blocks.get(hash).unwrap().txdata.contains(tx) {
        return (confirmations + 1).try_into().unwrap();
      }
    }

    0
  }
}
