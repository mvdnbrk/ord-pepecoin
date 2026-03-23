use super::*;

const MAX_SIZE: usize = 4_000;
const MAX_COMPRESSION_RATIO: usize = 30;
const COMPRESSION_THRESHOLD: usize = 64;

const KEY_TITLE: &str = "title";

fn decompress(tags: &BTreeMap<String, Vec<Vec<u8>>>) -> Option<Vec<u8>> {
  if let Some(compressed) = tags.get(tag::PROPERTIES_BR).and_then(|v| v.first()) {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut compressed.as_slice(), &mut decompressed).ok()?;

    if decompressed.len() > MAX_SIZE {
      return None;
    }
    if !compressed.is_empty() && decompressed.len() / compressed.len() > MAX_COMPRESSION_RATIO {
      return None;
    }

    Some(decompressed)
  } else {
    let raw = tags.get(tag::PROPERTIES)?.first()?.clone();
    if raw.len() > MAX_SIZE {
      return None;
    }
    Some(raw)
  }
}

fn compress(cbor: Vec<u8>, tags: &mut BTreeMap<String, Vec<Vec<u8>>>) -> Result {
  if cbor.len() > MAX_SIZE {
    bail!(
      "properties size of {} bytes exceeds {} byte limit",
      cbor.len(),
      MAX_SIZE
    );
  }

  if cbor.len() >= COMPRESSION_THRESHOLD {
    let mut compressed = Vec::new();
    brotli::BrotliCompress(&mut cbor.as_slice(), &mut compressed, &Default::default())?;

    if compressed.len() < cbor.len() {
      tags.insert(tag::PROPERTIES_BR.to_string(), vec![compressed]);
      return Ok(());
    }
  }

  tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);
  Ok(())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Properties {
  pub(crate) title: Option<String>,
}

impl Properties {
  pub(crate) fn from_tags(tags: &BTreeMap<String, Vec<Vec<u8>>>) -> Option<Self> {
    let cbor_bytes = decompress(tags)?;

    let value: ciborium::Value = ciborium::from_reader(cbor_bytes.as_slice()).ok()?;
    let map = match value {
      ciborium::Value::Map(map) => map,
      _ => return None,
    };

    let mut props = Properties::default();

    for (k, v) in map {
      if let (ciborium::Value::Text(key), ciborium::Value::Text(val)) = (k, v) {
        match key.as_str() {
          KEY_TITLE if !val.is_empty() => props.title = Some(val),
          _ => {}
        }
      }
    }

    if props == Properties::default() {
      None
    } else {
      Some(props)
    }
  }

  pub(crate) fn to_tags(&self, tags: &mut BTreeMap<String, Vec<Vec<u8>>>) -> Result {
    let mut map = Vec::new();

    if let Some(title) = &self.title {
      if !title.is_empty() {
        map.push((
          ciborium::Value::Text(KEY_TITLE.to_string()),
          ciborium::Value::Text(title.clone()),
        ));
      }
    }

    if map.is_empty() {
      return Ok(());
    }

    let mut cbor = Vec::new();
    ciborium::into_writer(&ciborium::Value::Map(map), &mut cbor)?;

    compress(cbor, tags)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roundtrip_title() {
    let props = Properties {
      title: Some("Hello".to_string()),
    };
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    assert!(tags.contains_key(tag::PROPERTIES));
    assert!(!tags.contains_key(tag::PROPERTIES_BR));

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title.unwrap(), "Hello");
  }

  #[test]
  fn roundtrip_title_compressed() {
    let long_title: String = (0..500).map(|i| char::from(b'A' + (i % 26) as u8)).collect();
    let props = Properties {
      title: Some(long_title.clone()),
    };
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    assert!(tags.contains_key(tag::PROPERTIES_BR));
    assert!(!tags.contains_key(tag::PROPERTIES));

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title.unwrap(), long_title);
  }

  #[test]
  fn empty_properties_not_encoded() {
    let props = Properties::default();
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();
    assert!(tags.is_empty());
  }

  #[test]
  fn empty_title_not_encoded() {
    let props = Properties {
      title: Some(String::new()),
    };
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();
    assert!(tags.is_empty());
  }

  #[test]
  fn rejects_oversized() {
    let props = Properties {
      title: Some("X".repeat(4000)),
    };
    let mut tags = BTreeMap::new();
    assert!(props.to_tags(&mut tags).is_err());
  }

  #[test]
  fn rejects_oversized_raw_on_read() {
    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![vec![0u8; 4001]]);
    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn no_tags_returns_none() {
    let tags = BTreeMap::new();
    assert_eq!(Properties::from_tags(&tags), None);
  }
}
