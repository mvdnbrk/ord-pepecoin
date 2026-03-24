use super::*;

const MAX_SIZE: usize = 4_000;
const MAX_COMPRESSION_RATIO: usize = 30;
const COMPRESSION_THRESHOLD: usize = 64;

const KEY_TITLE: &str = "title";
const KEY_TRAITS: &str = "traits";

/// Trait values: booleans, integers, null, or strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraitValue {
  Null,
  Bool(bool),
  Integer(i64),
  String(String),
}

impl std::fmt::Display for TraitValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      TraitValue::Null => write!(f, "null"),
      TraitValue::Bool(b) => write!(f, "{b}"),
      TraitValue::Integer(n) => write!(f, "{n}"),
      TraitValue::String(s) => write!(f, "{s}"),
    }
  }
}

impl TraitValue {
  fn to_cbor(&self) -> ciborium::Value {
    match self {
      TraitValue::Null => ciborium::Value::Null,
      TraitValue::Bool(b) => ciborium::Value::Bool(*b),
      TraitValue::Integer(n) => ciborium::Value::Integer((*n).into()),
      TraitValue::String(s) => ciborium::Value::Text(s.clone()),
    }
  }

  fn from_cbor(value: &ciborium::Value) -> Option<Self> {
    match value {
      ciborium::Value::Null => Some(TraitValue::Null),
      ciborium::Value::Bool(b) => Some(TraitValue::Bool(*b)),
      ciborium::Value::Integer(n) => {
        let n: i64 = (*n).try_into().ok()?;
        Some(TraitValue::Integer(n))
      }
      ciborium::Value::Text(s) => {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
          None
        } else {
          Some(TraitValue::String(trimmed))
        }
      }
      _ => None,
    }
  }
}

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
  title: Option<String>,
  traits: BTreeMap<String, TraitValue>,
}

impl Properties {
  pub(crate) fn with_title(mut self, title: &str) -> Self {
    let trimmed = title.trim();
    if !trimmed.is_empty() {
      self.title = Some(trimmed.to_string());
    }
    self
  }

  #[cfg(test)]
  pub(crate) fn with_trait(mut self, key: &str, value: TraitValue) -> Self {
    let key = key.trim();
    if !key.is_empty() {
      self.traits.insert(key.to_string(), value);
    }
    self
  }

  pub(crate) fn with_traits(mut self, traits: BTreeMap<String, TraitValue>) -> Self {
    for (k, v) in traits {
      let k = k.trim().to_string();
      if !k.is_empty() {
        self.traits.insert(k, v);
      }
    }
    self
  }

  pub(crate) fn title(&self) -> Option<&str> {
    self.title.as_deref()
  }

  pub(crate) fn traits(&self) -> &BTreeMap<String, TraitValue> {
    &self.traits
  }

  pub(crate) fn from_tags(tags: &BTreeMap<String, Vec<Vec<u8>>>) -> Option<Self> {
    let cbor_bytes = decompress(tags)?;

    let value: ciborium::Value = ciborium::from_reader(cbor_bytes.as_slice()).ok()?;
    let map = match value {
      ciborium::Value::Map(map) => map,
      _ => return None,
    };

    let mut props = Properties::default();

    for (k, v) in map {
      match (&k, &v) {
        (ciborium::Value::Text(key), ciborium::Value::Text(val)) if key == KEY_TITLE => {
          let trimmed = val.trim().to_string();
          if !trimmed.is_empty() {
            props.title = Some(trimmed);
          }
        }
        (ciborium::Value::Text(key), ciborium::Value::Map(trait_map)) if key == KEY_TRAITS => {
          for (tk, tv) in trait_map {
            if let ciborium::Value::Text(trait_key) = tk {
              let trait_key = trait_key.trim().to_string();
              if let Some(trait_val) = TraitValue::from_cbor(tv) {
                if !trait_key.is_empty() {
                  props.traits.insert(trait_key, trait_val);
                }
              }
            }
          }
        }
        _ => {}
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

    if !self.traits.is_empty() {
      let trait_pairs: Vec<(ciborium::Value, ciborium::Value)> = self
        .traits
        .iter()
        .map(|(k, v)| (ciborium::Value::Text(k.clone()), v.to_cbor()))
        .collect();
      map.push((
        ciborium::Value::Text(KEY_TRAITS.to_string()),
        ciborium::Value::Map(trait_pairs),
      ));
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
    let props = Properties::default().with_title("Hello");
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    assert!(tags.contains_key(tag::PROPERTIES));
    assert!(!tags.contains_key(tag::PROPERTIES_BR));

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title().unwrap(), "Hello");
  }

  #[test]
  fn roundtrip_title_compressed() {
    let long_title: String = (0u8..250)
      .cycle()
      .take(500)
      .map(|i| char::from(b'A' + i % 26))
      .collect();
    let props = Properties::default().with_title(&long_title);
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    assert!(tags.contains_key(tag::PROPERTIES_BR));
    assert!(!tags.contains_key(tag::PROPERTIES));

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title().unwrap(), long_title);
  }

  #[test]
  fn roundtrip_traits() {
    let props = Properties::default()
      .with_trait("background", TraitValue::String("gold".into()))
      .with_trait("eyes", TraitValue::String("laser".into()));
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(
      decoded.traits().get("background").unwrap(),
      &TraitValue::String("gold".into())
    );
    assert_eq!(
      decoded.traits().get("eyes").unwrap(),
      &TraitValue::String("laser".into())
    );
    assert_eq!(decoded.traits().len(), 2);
  }

  #[test]
  fn roundtrip_title_and_traits() {
    let props = Properties::default()
      .with_title("Rare Pepe #1")
      .with_trait("background", TraitValue::String("gold".into()))
      .with_trait("level", TraitValue::Integer(42));
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title().unwrap(), "Rare Pepe #1");
    assert_eq!(
      decoded.traits().get("background").unwrap(),
      &TraitValue::String("gold".into())
    );
    assert_eq!(
      decoded.traits().get("level").unwrap(),
      &TraitValue::Integer(42)
    );
  }

  #[test]
  fn roundtrip_trait_types() {
    let props = Properties::default()
      .with_trait("name", TraitValue::String("pepe".into()))
      .with_trait("rare", TraitValue::Bool(true))
      .with_trait("level", TraitValue::Integer(99))
      .with_trait("extra", TraitValue::Null);
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(
      decoded.traits().get("name").unwrap(),
      &TraitValue::String("pepe".into())
    );
    assert_eq!(
      decoded.traits().get("rare").unwrap(),
      &TraitValue::Bool(true)
    );
    assert_eq!(
      decoded.traits().get("level").unwrap(),
      &TraitValue::Integer(99)
    );
    assert_eq!(decoded.traits().get("extra").unwrap(), &TraitValue::Null);
  }

  #[test]
  fn with_traits_bulk() {
    let mut traits = BTreeMap::new();
    traits.insert("a".to_string(), TraitValue::String("1".into()));
    traits.insert("b".to_string(), TraitValue::Integer(2));
    let props = Properties::default().with_traits(traits);
    assert_eq!(props.traits().len(), 2);
  }

  #[test]
  fn empty_trait_key_skipped() {
    let props = Properties::default().with_trait("", TraitValue::String("value".into()));
    assert!(props.traits().is_empty());
  }

  #[test]
  fn whitespace_trimmed_in_trait_key() {
    let props = Properties::default().with_trait("  bg  ", TraitValue::String("gold".into()));
    assert_eq!(
      props.traits().get("bg").unwrap(),
      &TraitValue::String("gold".into())
    );
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
    let props = Properties::default().with_title("");
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();
    assert!(tags.is_empty());
  }

  #[test]
  fn rejects_oversized() {
    let props = Properties::default().with_title(&"X".repeat(4000));
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

  #[test]
  fn traits_only_no_title() {
    let props = Properties::default().with_trait("rarity", TraitValue::String("epic".into()));
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let decoded = Properties::from_tags(&tags).unwrap();
    assert!(decoded.title().is_none());
    assert_eq!(
      decoded.traits().get("rarity").unwrap(),
      &TraitValue::String("epic".into())
    );
  }
}
