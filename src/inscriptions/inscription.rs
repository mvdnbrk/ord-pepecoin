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

  pub(crate) fn properties(&self) -> Option<super::properties::Properties> {
    super::properties::Properties::from_tags(&self.tags)
  }

  pub(crate) fn set_properties(&mut self, props: super::properties::Properties) -> Result {
    props.to_tags(&mut self.tags)
  }

  /// Compress the inscription content with Brotli. Only keeps the compressed
  /// version if it is smaller than the original. Returns the number of bytes
  /// saved (0 if compression was not beneficial).
  pub(crate) fn compress(&mut self) -> Result<usize> {
    let body = match &self.body {
      Some(b) if !b.is_empty() => b,
      _ => return Ok(0),
    };

    let mode = match self.content_type() {
      Some(ct) if ct.starts_with("text/") || ct.contains("javascript") || ct.contains("json") => {
        brotli::enc::backward_references::BrotliEncoderMode::BROTLI_MODE_TEXT
      }
      Some(ct) if ct.contains("font") || ct.contains("woff") => {
        brotli::enc::backward_references::BrotliEncoderMode::BROTLI_MODE_FONT
      }
      _ => brotli::enc::backward_references::BrotliEncoderMode::BROTLI_MODE_GENERIC,
    };

    let mut params = brotli::enc::BrotliEncoderParams::default();
    params.mode = mode;
    params.quality = 11;

    let mut compressed = Vec::new();
    brotli::BrotliCompress(&mut body.as_slice(), &mut compressed, &params)?;

    if compressed.len() < body.len() {
      let saved = body.len() - compressed.len();
      self.body = Some(compressed);
      self
        .tags
        .insert(tag::CONTENT_ENCODING.to_string(), vec![b"br".to_vec()]);
      Ok(saved)
    } else {
      Ok(0)
    }
  }

  pub(crate) fn content_encoding(&self) -> Option<&str> {
    self
      .tags
      .get(tag::CONTENT_ENCODING)
      .and_then(|values| values.first())
      .and_then(|v| str::from_utf8(v).ok())
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn set_properties_roundtrip() {
    let mut inscription = Inscription::new(None, None, BTreeMap::new());
    let props = super::properties::Properties::default().with_title("Hello");
    inscription.set_properties(props).unwrap();
    assert_eq!(inscription.properties().unwrap().title().unwrap(), "Hello");
  }

  #[test]
  fn set_properties_empty_title() {
    let mut inscription = Inscription::new(None, None, BTreeMap::new());
    let props = super::properties::Properties::default().with_title("");
    inscription.set_properties(props).unwrap();
    assert!(inscription.tags.is_empty());
    assert_eq!(inscription.properties(), None);
  }

  #[test]
  fn set_properties_trims_whitespace() {
    let mut inscription = Inscription::new(None, None, BTreeMap::new());
    let props = super::properties::Properties::default().with_title("  hello  ");
    inscription.set_properties(props).unwrap();
    assert_eq!(inscription.properties().unwrap().title().unwrap(), "hello");
  }

  #[test]
  fn set_properties_whitespace_only() {
    let mut inscription = Inscription::new(None, None, BTreeMap::new());
    let props = super::properties::Properties::default().with_title("   ");
    inscription.set_properties(props).unwrap();
    assert_eq!(inscription.properties(), None);
  }

  #[test]
  fn compress_reduces_repetitive_text() {
    let body = "hello world! ".repeat(100);
    let original_len = body.len();
    let mut inscription = Inscription::new(
      Some(b"text/plain".to_vec()),
      Some(body.into_bytes()),
      BTreeMap::new(),
    );

    let saved = inscription.compress().unwrap();
    assert!(
      saved > 0,
      "compression should save bytes on repetitive text"
    );
    assert_eq!(
      inscription.content_encoding(),
      Some("br"),
      "should set content-encoding to br"
    );
    assert!(inscription.body.as_ref().unwrap().len() < original_len);
  }

  #[test]
  fn compress_skips_when_not_beneficial() {
    // Tiny body — Brotli overhead exceeds savings
    let body = vec![42u8; 3];
    let mut inscription = Inscription::new(
      Some(b"application/octet-stream".to_vec()),
      Some(body.clone()),
      BTreeMap::new(),
    );

    let saved = inscription.compress().unwrap();
    assert_eq!(saved, 0);
    assert_eq!(inscription.content_encoding(), None);
    assert_eq!(inscription.body.as_ref().unwrap(), &body);
  }

  #[test]
  fn compress_empty_body() {
    let mut inscription = Inscription::new(
      Some(b"text/plain".to_vec()),
      Some(Vec::new()),
      BTreeMap::new(),
    );
    assert_eq!(inscription.compress().unwrap(), 0);
  }

  #[test]
  fn compress_no_body() {
    let mut inscription = Inscription::new(Some(b"text/plain".to_vec()), None, BTreeMap::new());
    assert_eq!(inscription.compress().unwrap(), 0);
  }
}
