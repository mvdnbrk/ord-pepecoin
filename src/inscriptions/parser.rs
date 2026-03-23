use {super::*, bitcoin::Script, std::str};

pub(crate) const PROTOCOL_ID: &[u8] = b"ord";

pub(crate) struct InscriptionParser {}

impl InscriptionParser {
  pub(crate) fn parse(sig_scripts: Vec<Script>) -> ParsedInscription {
    let sig_script = &sig_scripts[0];

    let mut push_datas_vec = match Self::decode_push_datas(sig_script) {
      Some(push_datas) => push_datas,
      None => return ParsedInscription::None,
    };

    let mut push_datas = push_datas_vec.as_slice();

    // read protocol

    if push_datas.len() < 3 {
      return ParsedInscription::None;
    }

    let protocol = &push_datas[0];

    if protocol != PROTOCOL_ID {
      return ParsedInscription::None;
    }

    // read npieces

    let mut npieces = match Self::push_data_to_number(&push_datas[1]) {
      Some(n) => n,
      None => return ParsedInscription::None,
    };

    // read content type

    let content_type = push_datas[2].clone();

    push_datas = &push_datas[3..];

    if npieces == 0 {
      let mut tag_data: Vec<Vec<u8>> = push_datas.to_vec();
      for script in &sig_scripts[1..] {
        if let Some(more) = Self::decode_push_datas(script) {
          tag_data.extend(more);
        }
      }
      let tags = Self::parse_tags(&tag_data);
      return ParsedInscription::Complete(Inscription {
        content_type: Some(content_type),
        body: None,
        tags,
      });
    }

    // read body

    let mut body = vec![];

    let mut sig_scripts = sig_scripts.as_slice();

    // loop over transactions
    loop {
      // loop over chunks
      loop {
        if npieces == 0 {
          // Collect PRC-721 tag trailer from remaining push data
          // in this tx and all subsequent txs in the chain
          let mut tag_data: Vec<Vec<u8>> = push_datas.to_vec();
          let mut remaining = &sig_scripts[1..];
          while !remaining.is_empty() {
            if let Some(more) = Self::decode_push_datas(&remaining[0]) {
              tag_data.extend(more);
            }
            remaining = &remaining[1..];
          }

          let tags = Self::parse_tags(&tag_data);

          return ParsedInscription::Complete(Inscription {
            content_type: Some(content_type),
            body: Some(body),
            tags,
          });
        }

        if push_datas.len() < 2 {
          break;
        }

        let next = match Self::push_data_to_number(&push_datas[0]) {
          Some(n) => n,
          None => break,
        };

        if next != npieces - 1 {
          break;
        }

        body.append(&mut push_datas[1].clone());

        push_datas = &push_datas[2..];
        npieces -= 1;
      }

      if sig_scripts.len() <= 1 {
        return ParsedInscription::Partial;
      }

      sig_scripts = &sig_scripts[1..];

      push_datas_vec = match Self::decode_push_datas(&sig_scripts[0]) {
        Some(push_datas) => push_datas,
        None => return ParsedInscription::None,
      };

      if push_datas_vec.len() < 2 {
        return ParsedInscription::None;
      }

      let next = match Self::push_data_to_number(&push_datas_vec[0]) {
        Some(n) => n,
        None => return ParsedInscription::None,
      };

      if next != npieces - 1 {
        return ParsedInscription::None;
      }

      push_datas = push_datas_vec.as_slice();
    }
  }

  /// Parse tag trailer: consecutive key/value push data pairs after body countdown.
  fn parse_tags(push_datas: &[Vec<u8>]) -> BTreeMap<String, Vec<Vec<u8>>> {
    let mut tags: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();

    for pair in push_datas.chunks_exact(2) {
      if let Ok(key) = str::from_utf8(&pair[0]) {
        tags
          .entry(key.to_string())
          .or_default()
          .push(pair[1].clone());
      }
    }

    tags
  }

  pub(crate) fn decode_push_datas(script: &Script) -> Option<Vec<Vec<u8>>> {
    let mut bytes = script.as_bytes();
    let mut push_datas = vec![];

    while !bytes.is_empty() {
      // op_0
      if bytes[0] == 0 {
        push_datas.push(vec![]);
        bytes = &bytes[1..];
        continue;
      }

      // op_1 - op_16
      if bytes[0] >= 81 && bytes[0] <= 96 {
        push_datas.push(vec![bytes[0] - 80]);
        bytes = &bytes[1..];
        continue;
      }

      // op_push 1-75
      if bytes[0] >= 1 && bytes[0] <= 75 {
        let len = bytes[0] as usize;
        if bytes.len() < 1 + len {
          return None;
        }
        push_datas.push(bytes[1..1 + len].to_vec());
        bytes = &bytes[1 + len..];
        continue;
      }

      // op_pushdata1
      if bytes[0] == 76 {
        if bytes.len() < 2 {
          return None;
        }
        let len = bytes[1] as usize;
        if bytes.len() < 2 + len {
          return None;
        }
        push_datas.push(bytes[2..2 + len].to_vec());
        bytes = &bytes[2 + len..];
        continue;
      }

      // op_pushdata2
      if bytes[0] == 77 {
        if bytes.len() < 3 {
          return None;
        }
        let len = (bytes[1] as usize) + ((bytes[2] as usize) << 8);
        if bytes.len() < 3 + len {
          return None;
        }
        push_datas.push(bytes[3..3 + len].to_vec());
        bytes = &bytes[3 + len..];
        continue;
      }

      // op_pushdata4
      if bytes[0] == 78 {
        if bytes.len() < 5 {
          return None;
        }
        let len = (bytes[1] as usize)
          + ((bytes[2] as usize) << 8)
          + ((bytes[3] as usize) << 16)
          + ((bytes[4] as usize) << 24);
        if bytes.len() < 5 + len {
          return None;
        }
        push_datas.push(bytes[5..5 + len].to_vec());
        bytes = &bytes[5 + len..];
        continue;
      }

      return None;
    }

    Some(push_datas)
  }

  pub(crate) fn push_data_to_number(data: &[u8]) -> Option<u64> {
    if data.is_empty() {
      return Some(0);
    }

    if data.len() > 8 {
      return None;
    }

    let mut n: u64 = 0;
    let mut m: u64 = 0;

    for &byte in data {
      n += u64::from(byte) << m;
      m += 8;
    }

    Some(n)
  }
}

#[cfg(test)]
mod tests {
  use bitcoin::hashes::hex::FromHex;

  use super::*;

  fn inscription(content_type: impl AsRef<[u8]>, body: impl AsRef<[u8]>) -> Inscription {
    Inscription {
      content_type: Some(content_type.as_ref().into()),
      body: Some(body.as_ref().into()),
      tags: BTreeMap::new(),
    }
  }

  #[test]
  fn empty() {
    assert_eq!(
      InscriptionParser::parse(vec![Script::new()]),
      ParsedInscription::None
    );
  }

  #[test]
  fn no_inscription() {
    assert_eq!(
      InscriptionParser::parse(vec![Script::from_hex("483045022100a942753a4e036f59648469cb6ac19b33b1e423ff5ceaf93007001b54df46ca1f022025f6554a58b6fde5ff24b5e2556acc57d1d2108c0de2a14096e7ddae9c9fb96d0121034523d20080d1abe75a9fbed07b83e695db2f30e2cd89b80b154a0ed70badfc90").unwrap()]),
      ParsedInscription::None
    );
  }

  #[test]
  fn valid() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof"))
    );
  }

  #[test]
  fn valid_empty_fields() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[0]);
    script.push(&[0]);
    script.push(&[0]);
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("", ""))
    );
  }

  #[test]
  fn valid_multipart() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[0]);
    script.push(&[5]);
    script.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn valid_multitx() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[0]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn valid_multitx_long() {
    let mut expected = String::new();
    let mut script_parts = vec![];

    let mut script: Vec<Vec<u8>> = Vec::new();
    script.push(vec![3]);
    script.push(b"ord".to_vec());
    const LEN: usize = 100000;
    push_number_to_vec(&mut script, LEN as u64);
    script.push(vec![24]);
    script.push(b"text/plain;charset=utf-8".to_vec());

    let mut i = 0;
    while i < LEN {
      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number_to_vec(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
      i += 1;

      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number_to_vec(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
      i += 1;

      script_parts.push(script);
      script = Vec::new();
    }

    let mut scripts = vec![];
    script_parts
      .iter()
      .for_each(|script| scripts.push(Script::from(script.concat())));

    assert_eq!(
      InscriptionParser::parse(scripts),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", expected))
    );
  }

  #[test]
  fn valid_multitx_extradata() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script1.push(&[82]);
    script1.push(&[4]);
    script1.push(b"bark");
    script2.push(&[0]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof woof"))
    );
  }

  #[test]
  fn invalid_multitx_missingdata() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[0]);
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::None
    );
  }

  #[test]
  fn invalid_multitx_wrongcountdown() {
    let mut script1: Vec<&[u8]> = Vec::new();
    let mut script2: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]);
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]);
    script1.push(&[4]);
    script1.push(b"woof");
    script2.push(&[81]);
    script2.push(&[5]);
    script2.push(b" woof");
    assert_eq!(
      InscriptionParser::parse(vec![
        Script::from(script1.concat()),
        Script::from(script2.concat())
      ]),
      ParsedInscription::None
    );
  }

  #[allow(clippy::cast_possible_truncation)]
  fn push_number_to_vec(script: &mut Vec<Vec<u8>>, num: u64) {
    if num == 0 {
      script.push(vec![0]);
      return;
    }

    if num <= 16 {
      script.push(vec![(80 + num) as u8]);
      return;
    }

    if num <= 0x7f {
      script.push(vec![1]);
      script.push(vec![num as u8]);
      return;
    }

    if num <= 0x7fff {
      script.push(vec![2]);
      script.push(vec![(num % 256) as u8, (num / 256) as u8]);
      return;
    }

    if num <= 0x7fffff {
      script.push(vec![3]);
      script.push(vec![
        (num % 256) as u8,
        ((num / 256) % 256) as u8,
        (num / 256 / 256) as u8,
      ]);
      return;
    }

    panic!();
  }

  #[test]
  fn valid_long() {
    let mut expected = String::new();
    let mut script: Vec<Vec<u8>> = Vec::new();
    script.push(vec![3]);
    script.push(b"ord".to_vec());
    const LEN: usize = 100000;
    push_number_to_vec(&mut script, LEN as u64);
    script.push(vec![24]);
    script.push(b"text/plain;charset=utf-8".to_vec());
    for i in 0..LEN {
      let text = format!("{}", i % 10);
      expected += text.as_str();
      push_number_to_vec(&mut script, (LEN - i - 1) as u64);
      script.push(vec![1]);
      script.push(text.as_bytes().to_vec());
    }
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", expected))
    );
  }

  #[test]
  fn duplicate_field() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn invalid_tag() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[82]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn no_content() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial,
    );
  }

  #[test]
  fn no_body_inscription() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Complete(Inscription {
        content_type: Some(b"woof".to_vec()),
        body: None,
        tags: BTreeMap::new(),
      }),
    );
  }

  #[test]
  fn valid_with_tag_trailer() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    // tag trailer: key="parent", value=36 zero bytes
    script.push(&[6]);
    script.push(b"parent");
    script.push(&[36]);
    script.push(&[0; 36]);
    let result = InscriptionParser::parse(vec![Script::from(script.concat())]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, Some(b"woof".to_vec()));
        assert_eq!(inscription.tags.len(), 1);
        assert_eq!(inscription.tags.get("parent").unwrap(), &vec![vec![0; 36]]);
      }
      _ => panic!("expected Complete"),
    }
  }

  #[test]
  fn extra_data_after_body_parsed_as_tags() {
    // Data after countdown 0 that isn't valid UTF-8 keys is ignored
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    // extra data with valid UTF-8 key
    script.push(&[9]);
    script.push(b"woof woof");
    script.push(&[14]);
    script.push(b"woof woof woof");
    let result = InscriptionParser::parse(vec![Script::from(script.concat())]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, Some(b"woof".to_vec()));
        // "woof woof" is a valid UTF-8 key, so it becomes a tag
        assert_eq!(
          inscription.tags.get("woof woof").unwrap(),
          &vec![b"woof woof woof".to_vec()]
        );
      }
      _ => panic!("expected Complete"),
    }
  }

  #[test]
  fn prefix_data() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::None,
    );
  }

  #[test]
  fn wrong_protocol() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"dog");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::None
    );
  }

  #[test]
  fn incomplete_multipart() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[81]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial
    );
  }

  #[test]
  fn bad_npieces() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[82]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[83]);
    script.push(&[4]);
    script.push(b"woof");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");
    assert_eq!(
      InscriptionParser::parse(vec![Script::from(script.concat())]),
      ParsedInscription::Partial
    );
  }

  #[test]
  fn extract_from_transaction() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");

    let tx = Transaction {
      version: 0,
      lock_time: bitcoin::PackedLockTime(0),
      input: vec![TxIn {
        previous_output: OutPoint::null(),
        script_sig: Script::from(script.concat()),
        sequence: Sequence(0),
        witness: Witness::new(),
      }],
      output: Vec::new(),
    };

    assert_eq!(
      Inscription::from_transactions(&[tx]),
      ParsedInscription::Complete(inscription("text/plain;charset=utf-8", "woof")),
    );
  }

  #[test]
  fn do_not_extract_from_second_input() {
    let mut script: Vec<&[u8]> = Vec::new();
    script.push(&[3]);
    script.push(b"ord");
    script.push(&[81]);
    script.push(&[24]);
    script.push(b"text/plain;charset=utf-8");
    script.push(&[0]);
    script.push(&[4]);
    script.push(b"woof");

    let tx = Transaction {
      version: 0,
      lock_time: bitcoin::PackedLockTime(0),
      input: vec![
        TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::new(),
          sequence: Sequence(0),
          witness: Witness::new(),
        },
        TxIn {
          previous_output: OutPoint::null(),
          script_sig: Script::from(script.concat()),
          sequence: Sequence(0),
          witness: Witness::new(),
        },
      ],
      output: Vec::new(),
    };

    assert_eq!(
      Inscription::from_transactions(&[tx]),
      ParsedInscription::None
    );
  }

  #[test]
  fn multitx_tags_with_properties_and_parent() {
    use super::properties::Properties;

    // Build real PRC-721 tags: properties (CBOR title) + parent (36-byte inscription ID)
    let props = Properties::default().with_title("My Pepe");
    let mut tags = BTreeMap::new();
    props.to_tags(&mut tags).unwrap();

    let parent_id = InscriptionId {
      txid: bitcoin::Txid::all_zeros(),
      index: 1,
    };
    let parent_bytes = tag::encode_inscription_id(&parent_id);

    tags.insert(tag::PARENT.to_string(), vec![parent_bytes.clone()]);

    // Flatten tags into push data pairs: [key_len, key, val_len, val, ...]
    let mut tag_pushes: Vec<Vec<u8>> = Vec::new();
    for (key, values) in &tags {
      for val in values {
        tag_pushes.push(key.as_bytes().to_vec());
        tag_pushes.push(val.clone());
      }
    }

    // tx1: header + body + first tag pair
    let mut script1: Vec<u8> = Vec::new();
    script1.push(3);
    script1.extend_from_slice(b"ord");
    script1.push(81); // npieces=1
    script1.push(24);
    script1.extend_from_slice(b"text/plain;charset=utf-8");
    script1.push(0); // countdown 0
    script1.push(4);
    script1.extend_from_slice(b"woof");
    // first tag pair in tx1
    let k1 = &tag_pushes[0];
    let v1 = &tag_pushes[1];
    script1.push(u8::try_from(k1.len()).unwrap());
    script1.extend_from_slice(k1);
    script1.push(u8::try_from(v1.len()).unwrap());
    script1.extend_from_slice(v1);

    // tx2: second tag pair
    let mut script2: Vec<u8> = Vec::new();
    let k2 = &tag_pushes[2];
    let v2 = &tag_pushes[3];
    script2.push(u8::try_from(k2.len()).unwrap());
    script2.extend_from_slice(k2);
    script2.push(u8::try_from(v2.len()).unwrap());
    script2.extend_from_slice(v2);

    let result = InscriptionParser::parse(vec![Script::from(script1), Script::from(script2)]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, Some(b"woof".to_vec()));
        // Verify properties decode correctly
        let decoded_props = inscription.properties().unwrap();
        assert_eq!(decoded_props.title().unwrap(), "My Pepe");
        // Verify parent decodes correctly
        let parent_values = inscription.tags.get(tag::PARENT).unwrap();
        let decoded_parent = tag::parse_inscription_id(&parent_values[0]).unwrap();
        assert_eq!(decoded_parent, parent_id);
      }
      _ => panic!("expected Complete"),
    }
  }

  #[test]
  fn tag_trailer_spans_two_txs() {
    // Body in tx1, tags split across tx1 and tx2
    let mut script1: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[81]); // npieces=1
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[0]); // countdown 0
    script1.push(&[4]);
    script1.push(b"woof");
    // tag trailer starts here in tx1: parent key
    script1.push(&[6]);
    script1.push(b"parent");
    script1.push(&[36]);
    script1.push(&[0; 36]);

    // tx2 continues the tag trailer: delegate key
    let mut script2: Vec<&[u8]> = Vec::new();
    script2.push(&[8]);
    script2.push(b"delegate");
    script2.push(&[36]);
    script2.push(&[1; 36]);

    let result = InscriptionParser::parse(vec![
      Script::from(script1.concat()),
      Script::from(script2.concat()),
    ]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, Some(b"woof".to_vec()));
        assert_eq!(inscription.tags.len(), 2);
        assert_eq!(inscription.tags.get("parent").unwrap(), &vec![vec![0; 36]]);
        assert_eq!(
          inscription.tags.get("delegate").unwrap(),
          &vec![vec![1; 36]]
        );
      }
      _ => panic!("expected Complete"),
    }
  }

  #[test]
  fn tag_trailer_in_separate_tx_after_multitx_body() {
    // Body spans tx1+tx2, tags entirely in tx3
    let mut script1: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[82]); // npieces=2
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    script1.push(&[81]); // countdown 1
    script1.push(&[4]);
    script1.push(b"woof");

    let mut script2: Vec<&[u8]> = Vec::new();
    script2.push(&[0]); // countdown 0
    script2.push(&[5]);
    script2.push(b" woof");

    // tx3: only tags
    let mut script3: Vec<&[u8]> = Vec::new();
    script3.push(&[6]);
    script3.push(b"parent");
    script3.push(&[36]);
    script3.push(&[0; 36]);

    let result = InscriptionParser::parse(vec![
      Script::from(script1.concat()),
      Script::from(script2.concat()),
      Script::from(script3.concat()),
    ]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, Some(b"woof woof".to_vec()));
        assert_eq!(inscription.tags.len(), 1);
        assert_eq!(inscription.tags.get("parent").unwrap(), &vec![vec![0; 36]]);
      }
      _ => panic!("expected Complete"),
    }
  }

  #[test]
  fn delegate_only_with_tags_in_second_tx() {
    // npieces=0 (delegate), tags split across tx1 and tx2
    let mut script1: Vec<&[u8]> = Vec::new();
    script1.push(&[3]);
    script1.push(b"ord");
    script1.push(&[0]); // npieces=0
    script1.push(&[24]);
    script1.push(b"text/plain;charset=utf-8");
    // tag in tx1
    script1.push(&[8]);
    script1.push(b"delegate");
    script1.push(&[36]);
    script1.push(&[2; 36]);

    // more tags in tx2
    let mut script2: Vec<&[u8]> = Vec::new();
    script2.push(&[6]);
    script2.push(b"parent");
    script2.push(&[36]);
    script2.push(&[3; 36]);

    let result = InscriptionParser::parse(vec![
      Script::from(script1.concat()),
      Script::from(script2.concat()),
    ]);
    match result {
      ParsedInscription::Complete(inscription) => {
        assert_eq!(inscription.body, None);
        assert_eq!(inscription.tags.len(), 2);
        assert_eq!(
          inscription.tags.get("delegate").unwrap(),
          &vec![vec![2; 36]]
        );
        assert_eq!(inscription.tags.get("parent").unwrap(), &vec![vec![3; 36]]);
      }
      _ => panic!("expected Complete"),
    }
  }
}
