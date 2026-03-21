use super::*;

pub(crate) const PARENT: &str = "parent";
pub(crate) const DELEGATE: &str = "delegate";
pub(crate) const METADATA: &str = "metadata";
pub(crate) const PROPERTIES: &str = "properties";
pub(crate) const CONTENT_ENCODING: &str = "content-encoding";

/// Extract the first value for a tag key, removing it from the map.
pub(crate) fn take(tags: &mut BTreeMap<String, Vec<Vec<u8>>>, key: &str) -> Option<Vec<u8>> {
  let values = tags.get_mut(key)?;
  if values.is_empty() {
    return None;
  }
  Some(values.remove(0))
}

/// Extract all values for a tag key, removing it from the map.
pub(crate) fn take_all(tags: &mut BTreeMap<String, Vec<Vec<u8>>>, key: &str) -> Vec<Vec<u8>> {
  tags.remove(key).unwrap_or_default()
}

/// Parse a 36-byte inscription ID (32-byte txid LE + 4-byte index LE) from tag value.
pub(crate) fn parse_inscription_id(value: &[u8]) -> Option<InscriptionId> {
  if value.len() != 36 {
    return None;
  }

  let txid = bitcoin::Txid::from_slice(&value[0..32]).ok()?;
  let index = u32::from_le_bytes(value[32..36].try_into().ok()?);

  Some(InscriptionId { txid, index })
}

/// Encode an inscription ID as 36 bytes (32-byte txid LE + 4-byte index LE).
pub(crate) fn encode_inscription_id(id: &InscriptionId) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(36);
  bytes.extend_from_slice(&id.txid[..]);
  bytes.extend_from_slice(&id.index.to_le_bytes());
  bytes
}

#[cfg(test)]
mod tests {
  use super::*;
  use bitcoin::hashes::Hash;

  #[test]
  fn roundtrip_inscription_id() {
    let id = InscriptionId {
      txid: bitcoin::Txid::all_zeros(),
      index: 0,
    };
    let encoded = encode_inscription_id(&id);
    assert_eq!(encoded.len(), 36);
    let decoded = parse_inscription_id(&encoded).unwrap();
    assert_eq!(decoded, id);
  }

  #[test]
  fn roundtrip_inscription_id_with_index() {
    let id = InscriptionId {
      txid: bitcoin::Txid::all_zeros(),
      index: 42,
    };
    let encoded = encode_inscription_id(&id);
    let decoded = parse_inscription_id(&encoded).unwrap();
    assert_eq!(decoded, id);
  }

  #[test]
  fn parse_invalid_length() {
    assert_eq!(parse_inscription_id(&[0; 35]), None);
    assert_eq!(parse_inscription_id(&[0; 37]), None);
    assert_eq!(parse_inscription_id(&[]), None);
  }

  #[test]
  fn take_first_value() {
    let mut tags = BTreeMap::new();
    tags.insert("parent".to_string(), vec![vec![1], vec![2]]);
    assert_eq!(take(&mut tags, "parent"), Some(vec![1]));
    assert_eq!(tags.get("parent").unwrap(), &vec![vec![2]]);
  }

  #[test]
  fn take_missing_key() {
    let mut tags: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
    assert_eq!(take(&mut tags, "parent"), None);
  }

  #[test]
  fn take_all_values() {
    let mut tags = BTreeMap::new();
    tags.insert("parent".to_string(), vec![vec![1], vec![2]]);
    assert_eq!(take_all(&mut tags, "parent"), vec![vec![1], vec![2]]);
    assert!(!tags.contains_key("parent"));
  }
}
