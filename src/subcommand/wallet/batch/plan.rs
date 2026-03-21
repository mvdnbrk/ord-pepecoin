use {
  super::{super::inscribe::ParentInfo, *},
  bitcoin::{blockdata::opcodes, blockdata::script, PackedLockTime, PublicKey, TxIn, Witness},
  std::collections::BTreeMap,
  std::collections::BTreeSet,
};

// Pepecoin Core enforces a 1650-byte scriptSig limit (IsStandard policy).
// The scriptSig contains: inscription data + signature (~74 bytes) + redeem script.
// We reserve 150 bytes for signature + redeem script overhead, leaving ~1500 for data.
pub(crate) const MAX_PAYLOAD_LEN: usize = 1500;

pub(crate) struct RevealTx {
  pub(crate) tx: Transaction,
  pub(crate) redeem_script: Script,
  pub(crate) partial_script: Script,
}

pub(crate) fn split_inscription_into_batches(inscription: &Inscription) -> Vec<Script> {
  let inscription_script = inscription.get_inscription_script();

  #[derive(Clone)]
  enum Elem {
    Push(Vec<u8>),
    Op(opcodes::All),
  }

  impl Elem {
    fn apply(self, builder: script::Builder) -> script::Builder {
      match self {
        Elem::Push(data) => builder.push_slice(&data),
        Elem::Op(op) => builder.push_opcode(op),
      }
    }
    fn encoded_len(&self) -> usize {
      match self {
        Elem::Push(data) => {
          let len = data.len();
          if len <= 75 {
            1 + len
          } else if len <= 255 {
            2 + len
          } else if len <= 65535 {
            3 + len
          } else {
            5 + len
          }
        }
        Elem::Op(_) => 1,
      }
    }
  }

  let elems: Vec<Elem> = inscription_script
    .instructions()
    .filter_map(|instr| match instr.ok()? {
      script::Instruction::PushBytes(data) => Some(Elem::Push(data.to_vec())),
      script::Instruction::Op(op) => Some(Elem::Op(op)),
    })
    .collect();

  let header = &elems[..3.min(elems.len())];
  let data_elems = &elems[3.min(elems.len())..];

  let mut pairs: Vec<(&Elem, &Elem)> = Vec::new();
  let mut i = 0;
  while i + 1 < data_elems.len() {
    pairs.push((&data_elems[i], &data_elems[i + 1]));
    i += 2;
  }

  let mut batches = Vec::new();
  let mut partial = script::Builder::new();
  let mut partial_len: usize = 0;

  for elem in header {
    partial = elem.clone().apply(partial);
    partial_len += elem.encoded_len();
  }

  for (countdown, data) in pairs {
    let pair_len = countdown.encoded_len() + data.encoded_len();

    if partial_len + pair_len > MAX_PAYLOAD_LEN && partial_len > 0 {
      batches.push(partial.into_script());
      partial = script::Builder::new();
      partial_len = 0;
    }

    partial = countdown.clone().apply(partial);
    partial = data.clone().apply(partial);
    partial_len += pair_len;
  }

  if partial_len > 0 {
    batches.push(partial.into_script());
  }

  batches
}

pub(crate) fn build_lock_scripts(batches: &[Script], pubkey: &PublicKey) -> Vec<Script> {
  let mut locks = Vec::new();
  for batch in batches {
    let mut lock_builder = script::Builder::new()
      .push_slice(&pubkey.to_bytes())
      .push_opcode(opcodes::all::OP_CHECKSIGVERIFY);
    for _ in batch.instructions() {
      lock_builder = lock_builder.push_opcode(opcodes::all::OP_DROP);
    }
    let lock = lock_builder
      .push_opcode(opcodes::all::OP_PUSHNUM_1)
      .into_script();
    locks.push(lock);
  }
  locks
}

pub(crate) fn create_batch_inscription_transactions(
  inscriptions: Vec<Inscription>,
  destinations: Vec<Address>,
  existing_inscriptions: BTreeMap<SatPoint, InscriptionId>,
  network: Network,
  utxos: BTreeMap<OutPoint, Amount>,
  change: [Address; 2],
  commit_fee_rate: FeeRate,
  reveal_fee_rate: FeeRate,
  pubkey: PublicKey,
  postage: Amount,
  parent_infos: &[ParentInfo],
) -> Result<(Transaction, Vec<Vec<RevealTx>>, u64)> {
  let mut reveal_chains: Vec<Vec<RevealTx>> = Vec::new();
  let mut chain_initial_values: Vec<u64> = Vec::new();
  let mut total_reveal_value = 0;
  let mut fees = 0;

  for (inscription, destination) in inscriptions.iter().zip(destinations.iter()) {
    let batches = split_inscription_into_batches(inscription);
    let locks = build_lock_scripts(&batches, &pubkey);

    let mut chain_reveal_fees = Vec::new();
    for batch in &batches {
      let num_chunks = batch.instructions().count();
      let estimated_sig_size = batch.len() + 1 + 72 + 1 + (33 + 1 + num_chunks + 1);
      let tx_vsize = 82 + estimated_sig_size;
      let fee = reveal_fee_rate.fee(tx_vsize).to_sat();
      chain_reveal_fees.push(fee);
    }

    let mut reveal_chain = Vec::new();
    let mut current_reveal_value = postage.to_sat() + chain_reveal_fees.iter().sum::<u64>();
    chain_initial_values.push(current_reveal_value);
    total_reveal_value += current_reveal_value;

    for (i, (batch, lock)) in batches.into_iter().zip(locks.iter()).enumerate() {
      let is_last = i == chain_reveal_fees.len() - 1;
      let fee = chain_reveal_fees[i];
      let next_value = current_reveal_value.checked_sub(fee).unwrap();

      let mut inputs = vec![TxIn {
        previous_output: OutPoint::null(), // To be filled after commit tx
        script_sig: Script::new(),
        witness: Witness::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
      }];

      let mut outputs = vec![TxOut {
        script_pubkey: if is_last {
          destination.script_pubkey()
        } else {
          Address::p2sh(&locks[i + 1], network)
            .unwrap()
            .script_pubkey()
        },
        value: next_value,
      }];

      // Add parent inputs/outputs on the first reveal tx
      if i == 0 {
        for parent in parent_infos {
          inputs.push(TxIn {
            previous_output: OutPoint::null(), // resolved during signing
            script_sig: Script::new(),
            witness: Witness::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
          });
          outputs.push(TxOut {
            script_pubkey: parent.destination.script_pubkey(),
            value: parent.tx_out.value,
          });
        }
      }

      let reveal_tx = Transaction {
        input: inputs,
        output: outputs,
        lock_time: PackedLockTime::ZERO,
        version: 1,
      };

      fees += fee;
      reveal_chain.push(RevealTx {
        tx: reveal_tx,
        redeem_script: lock.clone(),
        partial_script: batch,
      });

      current_reveal_value = next_value;
    }

    reveal_chains.push(reveal_chain);
  }

  let mut inputs = Vec::new();
  let mut input_value = 0;

  let inscribed_utxos = existing_inscriptions
    .keys()
    .map(|satpoint| satpoint.outpoint)
    .collect::<BTreeSet<OutPoint>>();

  for (outpoint, amount) in &utxos {
    if inscribed_utxos.contains(outpoint) {
      continue;
    }

    inputs.push(TxIn {
      previous_output: *outpoint,
      script_sig: Script::new(),
      witness: Witness::new(),
      sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
    });
    input_value += amount.to_sat();

    if input_value >= total_reveal_value {
      let mut outputs = Vec::new();
      for chain in &reveal_chains {
        outputs.push(TxOut {
          script_pubkey: Address::p2sh(&chain[0].redeem_script, network)
            .unwrap()
            .script_pubkey(),
          value: 0, // Placeholder
        });
      }
      outputs.push(TxOut {
        script_pubkey: change[0].script_pubkey(),
        value: 0, // Placeholder
      });

      let commit_tx = Transaction {
        version: 1,
        lock_time: PackedLockTime::ZERO,
        input: inputs.clone(),
        output: outputs,
      };

      let fee = commit_fee_rate.fee(commit_tx.vsize()).to_sat();
      if input_value >= total_reveal_value + fee {
        break;
      }
    }
  }

  if input_value < total_reveal_value {
    bail!(
      "not enough cardinal UTXOs: need {} sat ({:.2} PEP) but only {} sat ({:.2} PEP) available",
      total_reveal_value,
      total_reveal_value as f64 / 100_000_000.0,
      input_value,
      input_value as f64 / 100_000_000.0,
    );
  }

  let mut commit_tx = Transaction {
    version: 1,
    lock_time: PackedLockTime::ZERO,
    input: inputs,
    output: Vec::new(),
  };

  for (i, chain) in reveal_chains.iter().enumerate() {
    commit_tx.output.push(TxOut {
      script_pubkey: Address::p2sh(&chain[0].redeem_script, network)
        .unwrap()
        .script_pubkey(),
      value: chain_initial_values[i],
    });
  }

  let fee = commit_fee_rate.fee(commit_tx.vsize()).to_sat();
  let change_value = input_value.checked_sub(total_reveal_value + fee).unwrap();

  if change_value > 0 {
    commit_tx.output.push(TxOut {
      script_pubkey: change[0].script_pubkey(),
      value: change_value,
    });
  }

  fees += calculate_fee(&commit_tx, &utxos);

  let commit_txid = commit_tx.txid();

  for (i, chain) in reveal_chains.iter_mut().enumerate() {
    chain[0].tx.input[0].previous_output = OutPoint {
      txid: commit_txid,
      vout: u32::try_from(i).unwrap(),
    };
  }

  Ok((commit_tx, reveal_chains, fees))
}

pub(crate) fn calculate_fee(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> u64 {
  tx.input
    .iter()
    .map(|txin| {
      utxos
        .get(&txin.previous_output)
        .map(|a| a.to_sat())
        .unwrap_or(0)
    })
    .sum::<u64>()
    .saturating_sub(tx.output.iter().map(|txout| txout.value).sum::<u64>())
}
