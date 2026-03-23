use {
  super::*,
  bitcoin::{
    blockdata::{opcodes, script},
    Script,
  },
  parser::{InscriptionParser, PROTOCOL_ID},
  std::str,
};

#[derive(Debug, PartialEq, Clone)]
pub struct Inscription {
  pub body: Option<Vec<u8>>,
  pub content_type: Option<Vec<u8>>,
  pub tags: BTreeMap<String, Vec<Vec<u8>>>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ParsedInscription {
  None,
  Partial,
  Complete(Inscription),
}

impl Inscription {
  pub fn new(
    content_type: Option<Vec<u8>>,
    body: Option<Vec<u8>>,
    tags: BTreeMap<String, Vec<Vec<u8>>>,
  ) -> Self {
    Self {
      content_type,
      body,
      tags,
    }
  }

  pub(crate) fn properties_title(&self) -> Option<String> {
    let cbor_bytes = if let Some(compressed) = self.tags.get("properties;br").and_then(|v| v.first()) {
      let mut decompressed = Vec::new();
      brotli::BrotliDecompress(&mut compressed.as_slice(), &mut decompressed).ok()?;
      decompressed
    } else {
      self.tags.get("properties")?.first()?.clone()
    };

    let value: ciborium::Value = ciborium::from_reader(cbor_bytes.as_slice()).ok()?;
    if let ciborium::Value::Map(map) = value {
      for (k, v) in map {
        if let (ciborium::Value::Text(key), ciborium::Value::Text(val)) = (k, v) {
          if key == "title" && !val.is_empty() {
            return Some(val);
          }
        }
      }
    }
    None
  }

  pub(crate) fn set_title(&mut self, title: &str) -> Result {
    if !title.is_empty() {
      let mut properties = std::collections::BTreeMap::new();
      properties.insert("title", title);
      let mut cbor = Vec::new();
      ciborium::into_writer(&properties, &mut cbor)?;

      let mut compressed = Vec::new();
      brotli::BrotliCompress(&mut cbor.as_slice(), &mut compressed, &Default::default())?;

      if compressed.len() < cbor.len() {
        self.tags.insert("properties;br".to_string(), vec![compressed]);
      } else {
        self.tags.insert("properties".to_string(), vec![cbor]);
      }
    }
    Ok(())
  }

  pub(crate) fn from_transactions(txs: &[Transaction]) -> ParsedInscription {
    let mut sig_scripts = Vec::with_capacity(txs.len());
    for tx in txs {
      if tx.input.is_empty() {
        return ParsedInscription::None;
      }
      sig_scripts.push(tx.input[0].script_sig.clone());
    }
    InscriptionParser::parse(sig_scripts)
  }

  pub(crate) fn from_file(chain: Chain, path: impl AsRef<Path>) -> Result<Self, Error> {
    let path = path.as_ref();

    let body = fs::read(path).with_context(|| format!("io error reading {}", path.display()))?;

    if let Some(limit) = chain.inscription_content_size_limit() {
      let len = body.len();
      if len > limit {
        bail!("content size of {len} bytes exceeds {limit} byte limit for {chain} inscriptions");
      }
    }

    let content_type = Media::content_type_for_path(path)?;

    Ok(Self {
      body: Some(body),
      content_type: Some(content_type.into()),
      tags: BTreeMap::new(),
    })
  }

  fn push_number(mut builder: script::Builder, num: u64) -> script::Builder {
    if num == 0 {
      builder = builder.push_opcode(opcodes::all::OP_PUSHBYTES_0);
    } else if num <= 16 {
      let opcode_val = opcodes::all::OP_PUSHNUM_1.to_u8() + u8::try_from(num - 1).unwrap();
      builder = builder.push_opcode(opcodes::All::from(opcode_val));
    } else {
      builder = builder.push_int(i64::try_from(num).unwrap());
    }
    builder
  }

  pub(crate) fn get_inscription_script(&self) -> Script {
    let mut builder = script::Builder::new().push_slice(PROTOCOL_ID);

    let empty = Vec::new();
    let body = self.body.as_ref().unwrap_or(&empty);
    let chunks: Vec<&[u8]> = body.chunks(240).collect();

    builder = Self::push_number(builder, u64::try_from(chunks.len()).unwrap());

    builder = builder.push_slice(self.content_type.as_deref().unwrap_or_default());

    for (i, chunk) in chunks.iter().enumerate() {
      builder = Self::push_number(builder, u64::try_from(chunks.len() - i - 1).unwrap());
      builder = builder.push_slice(chunk);
    }

    // PRC-721 tag trailer
    for (key, values) in &self.tags {
      for value in values {
        builder = builder.push_slice(key.as_bytes());
        builder = builder.push_slice(value);
      }
    }

    builder.into_script()
  }

  pub(crate) fn delegate_id(&self) -> Option<InscriptionId> {
    self
      .tags
      .get(tag::DELEGATE)
      .and_then(|values| values.first())
      .and_then(|v| tag::parse_inscription_id(v))
  }

  pub(crate) fn media(&self) -> Media {
    if self.delegate_id().is_some() || self.body.is_none() {
      return Media::Unknown;
    }

    let Some(content_type) = self.content_type() else {
      return Media::Unknown;
    };

    content_type.parse().unwrap_or(Media::Unknown)
  }

  pub(crate) fn body(&self) -> Option<&[u8]> {
    if self.delegate_id().is_some() {
      return None;
    }
    Some(self.body.as_ref()?)
  }

  pub(crate) fn into_body(self) -> Option<Vec<u8>> {
    if self.delegate_id().is_some() {
      return None;
    }
    self.body
  }

  pub(crate) fn content_length(&self) -> Option<usize> {
    Some(self.body()?.len())
  }

  pub(crate) fn content_type(&self) -> Option<&str> {
    if self.delegate_id().is_some() {
      return None;
    }
    str::from_utf8(self.content_type.as_ref()?).ok()
  }

  pub fn to_p2sh_unlock(&self) -> Script {
    self.get_inscription_script()
  }

  #[cfg(test)]
  pub(crate) fn to_witness(&self) -> Witness {
    let mut builder = script::Builder::new()
      .push_opcode(opcodes::OP_FALSE)
      .push_opcode(opcodes::all::OP_IF)
      .push_slice(PROTOCOL_ID);

    if let Some(content_type) = &self.content_type {
      builder = builder.push_slice(&[1]).push_slice(content_type);
    }

    if let Some(body) = &self.body {
      builder = builder.push_slice(&[]);
      for chunk in body.chunks(520) {
        builder = builder.push_slice(chunk);
      }
    }

    let script = builder.push_opcode(opcodes::all::OP_ENDIF).into_script();

    let mut witness = Witness::new();

    witness.push(script);
    witness.push([]);

    witness
  }
}
