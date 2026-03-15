use {
  super::*,
  bitcoin::{
    blockdata::script,
    secp256k1::{self, Secp256k1},
    util::sighash::SighashCache,
    EcdsaSighashType, Transaction,
  },
};

pub(crate) struct LocalSigner;

impl LocalSigner {
  pub(crate) fn sign_transaction(
    wallet: &Wallet,
    unsigned_transaction: Transaction,
  ) -> Result<Transaction> {
    let mut signed_transaction = unsigned_transaction.clone();
    let secp = Secp256k1::new();
    
    for (i, input) in unsigned_transaction.input.iter().enumerate() {
      let Some(utxo) = wallet.utxos().get(&input.previous_output) else {
        bail!("UTXO not found for input {}", i);
      };

      let (change, index) = wallet.get_address_info(&utxo.script_pubkey)?;
      let privkey = wallet.get_private_key(change, index)?;

      let sighash = SighashCache::new(&signed_transaction).legacy_signature_hash(
        i,
        &utxo.script_pubkey,
        EcdsaSighashType::All as u32,
      )?;

      let msg = secp256k1::Message::from_slice(&sighash[..])?;
      let sig = secp.sign_ecdsa(&msg, &privkey.inner);

      let mut sig_with_hashtype = sig.serialize_der().to_vec();
      sig_with_hashtype.push(EcdsaSighashType::All as u8);

      let script_sig = script::Builder::new()
        .push_slice(&sig_with_hashtype)
        .push_slice(&privkey.public_key(&secp).to_bytes())
        .into_script();

      signed_transaction.input[i].script_sig = script_sig;
    }

    Ok(signed_transaction)
  }
}
