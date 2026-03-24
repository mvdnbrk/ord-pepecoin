use super::*;

const MAX_SIZE: usize = 4_000;
const MAX_COMPRESSION_RATIO: usize = 30;
const COMPRESSION_THRESHOLD: usize = 64;

const KEY_TITLE: i64 = 0; // "title"
const KEY_TRAITS: i64 = 1; // "traits"

const KEY_TITLE_STR: &str = "title";
const KEY_TRAITS_STR: &str = "traits";

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

  /// Decode a trait value from CBOR.
  /// Returns `Ok(Some(val))` for valid types, `Ok(None)` for empty strings
  /// (skipped), and `Err(())` for unsupported types (floats, bytes, arrays,
  /// maps, etc.) which invalidate the entire properties tag.
  fn from_cbor(value: &ciborium::Value) -> std::result::Result<Option<Self>, ()> {
    match value {
      ciborium::Value::Null => Ok(Some(TraitValue::Null)),
      ciborium::Value::Bool(b) => Ok(Some(TraitValue::Bool(*b))),
      ciborium::Value::Integer(n) => {
        let n: i64 = (*n).try_into().map_err(|_| ())?;
        Ok(Some(TraitValue::Integer(n)))
      }
      ciborium::Value::Text(s) => {
        let trimmed = s.trim().to_string();
        if trimmed.is_empty() {
          Ok(None)
        } else {
          Ok(Some(TraitValue::String(trimmed)))
        }
      }
      // Floats, bytes, arrays, maps, and other CBOR types are rejected
      _ => Err(()),
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
  traits: Vec<(String, TraitValue)>,
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
      self.traits.push((key.to_string(), value));
    }
    self
  }

  pub(crate) fn with_traits(mut self, traits: Vec<(String, TraitValue)>) -> Self {
    for (k, v) in traits {
      let k = k.trim().to_string();
      if !k.is_empty() {
        self.traits.push((k, v));
      }
    }
    self
  }

  pub(crate) fn title(&self) -> Option<&str> {
    self.title.as_deref()
  }

  pub(crate) fn traits(&self) -> &[(String, TraitValue)] {
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
      let key_id = match &k {
        ciborium::Value::Integer(n) => Some(i128::from(*n)),
        ciborium::Value::Text(s) if s == KEY_TITLE_STR => Some(i128::from(KEY_TITLE)),
        ciborium::Value::Text(s) if s == KEY_TRAITS_STR => Some(i128::from(KEY_TRAITS)),
        _ => None,
      };

      match (key_id, &v) {
        (Some(id), ciborium::Value::Text(val)) if id == i128::from(KEY_TITLE) => {
          let trimmed = val.trim().to_string();
          if !trimmed.is_empty() {
            props.title = Some(trimmed);
          }
        }
        (Some(id), ciborium::Value::Map(trait_map)) if id == i128::from(KEY_TRAITS) => {
          let mut seen = HashSet::new();
          for (tk, tv) in trait_map {
            if let ciborium::Value::Text(trait_key) = tk {
              let trait_key = trait_key.trim().to_string();
              if trait_key.is_empty() {
                continue;
              }
              // Reject duplicate trait names
              if !seen.insert(trait_key.clone()) {
                return None;
              }
              // Reject unsupported value types (floats, bytes, arrays, maps)
              match TraitValue::from_cbor(tv) {
                Ok(Some(trait_val)) => {
                  props.traits.push((trait_key, trait_val));
                }
                Ok(None) => {} // empty string — skip
                Err(()) => return None,
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
          ciborium::Value::Integer(KEY_TITLE.into()),
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
        ciborium::Value::Integer(KEY_TRAITS.into()),
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
    // Order preserved: background first, eyes second
    assert_eq!(
      decoded.traits()[0],
      ("background".into(), TraitValue::String("gold".into()))
    );
    assert_eq!(
      decoded.traits()[1],
      ("eyes".into(), TraitValue::String("laser".into()))
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
      decoded.traits()[0],
      ("background".into(), TraitValue::String("gold".into()))
    );
    assert_eq!(
      decoded.traits()[1],
      ("level".into(), TraitValue::Integer(42))
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
      decoded.traits()[0],
      ("name".into(), TraitValue::String("pepe".into()))
    );
    assert_eq!(decoded.traits()[1], ("rare".into(), TraitValue::Bool(true)));
    assert_eq!(
      decoded.traits()[2],
      ("level".into(), TraitValue::Integer(99))
    );
    assert_eq!(decoded.traits()[3], ("extra".into(), TraitValue::Null));
  }

  #[test]
  fn with_traits_bulk() {
    let traits = vec![
      ("a".to_string(), TraitValue::String("1".into())),
      ("b".to_string(), TraitValue::Integer(2)),
    ];
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
      props.traits()[0],
      ("bg".into(), TraitValue::String("gold".into()))
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
  fn duplicate_trait_keys_rejected() {
    let trait_pairs = vec![
      (
        ciborium::Value::Text("bg".to_string()),
        ciborium::Value::Text("gold".to_string()),
      ),
      (
        ciborium::Value::Text("bg".to_string()),
        ciborium::Value::Text("silver".to_string()),
      ),
    ];
    let cbor_map = ciborium::Value::Map(vec![(
      ciborium::Value::Integer(1.into()),
      ciborium::Value::Map(trait_pairs),
    )]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn float_trait_value_rejected() {
    let trait_pairs = vec![(
      ciborium::Value::Text("score".to_string()),
      ciborium::Value::Float(1.5),
    )];
    let cbor_map = ciborium::Value::Map(vec![(
      ciborium::Value::Integer(1.into()),
      ciborium::Value::Map(trait_pairs),
    )]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn array_trait_value_rejected() {
    let trait_pairs = vec![(
      ciborium::Value::Text("tags".to_string()),
      ciborium::Value::Array(vec![ciborium::Value::Text("a".to_string())]),
    )];
    let cbor_map = ciborium::Value::Map(vec![(
      ciborium::Value::Integer(1.into()),
      ciborium::Value::Map(trait_pairs),
    )]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn nested_map_trait_value_rejected() {
    let trait_pairs = vec![(
      ciborium::Value::Text("nested".to_string()),
      ciborium::Value::Map(vec![(
        ciborium::Value::Text("key".to_string()),
        ciborium::Value::Text("val".to_string()),
      )]),
    )];
    let cbor_map = ciborium::Value::Map(vec![(
      ciborium::Value::Integer(1.into()),
      ciborium::Value::Map(trait_pairs),
    )]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn invalid_trait_invalidates_entire_properties_including_title() {
    // Valid title + invalid trait (float) → entire properties rejected
    let cbor_map = ciborium::Value::Map(vec![
      (
        ciborium::Value::Integer(0.into()),
        ciborium::Value::Text("Rare Pepe".to_string()),
      ),
      (
        ciborium::Value::Integer(1.into()),
        ciborium::Value::Map(vec![(
          ciborium::Value::Text("score".to_string()),
          ciborium::Value::Float(9.5),
        )]),
      ),
    ]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    // Title is lost — all-or-nothing
    assert_eq!(Properties::from_tags(&tags), None);
  }

  #[test]
  fn string_keys_accepted_on_decode() {
    // Encode with string keys — should still decode
    let cbor_map = ciborium::Value::Map(vec![
      (
        ciborium::Value::Text("title".to_string()),
        ciborium::Value::Text("Legacy Pepe".to_string()),
      ),
      (
        ciborium::Value::Text("traits".to_string()),
        ciborium::Value::Map(vec![(
          ciborium::Value::Text("bg".to_string()),
          ciborium::Value::Text("gold".to_string()),
        )]),
      ),
    ]);
    let mut cbor = Vec::new();
    ciborium::into_writer(&cbor_map, &mut cbor).unwrap();

    let mut tags = BTreeMap::new();
    tags.insert(tag::PROPERTIES.to_string(), vec![cbor]);

    let decoded = Properties::from_tags(&tags).unwrap();
    assert_eq!(decoded.title().unwrap(), "Legacy Pepe");
    assert_eq!(
      decoded.traits()[0],
      ("bg".into(), TraitValue::String("gold".into()))
    );
  }

  #[test]
  fn traits_only_no_title() {
    let props = Properties::default().with_trait("rarity", TraitValue::String("epic".into()));
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let decoded = Properties::from_tags(&tags).unwrap();
    assert!(decoded.title().is_none());
    assert_eq!(
      decoded.traits()[0],
      ("rarity".into(), TraitValue::String("epic".into()))
    );
  }
}
