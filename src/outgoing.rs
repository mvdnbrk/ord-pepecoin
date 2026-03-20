use super::*;

#[derive(Debug, PartialEq)]
pub(crate) enum Outgoing {
  Amount(Amount),
  InscriptionId(InscriptionId),
  SatPoint(SatPoint),
}

impl FromStr for Outgoing {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    // Accept Pepecoin denominations as aliases for bitcoin crate denominations
    let s = &s
      .replace("ribbit", "sat")
      .replace("RIBBIT", "SAT")
      .replace("rib", "sat")
      .replace("RIB", "SAT")
      .replace("pep", "btc")
      .replace("PEP", "BTC");
    Ok(if s.contains(':') {
      Self::SatPoint(s.parse()?)
    } else if s.len() >= 66 {
      Self::InscriptionId(s.parse()?)
    } else if s.contains(' ') {
      Self::Amount(s.parse()?)
    } else if let Some(i) = s.find(|c: char| c.is_alphabetic()) {
      let mut s = s.to_owned();
      s.insert(i, ' ');
      Self::Amount(s.parse()?)
    } else {
      Self::Amount(s.parse()?)
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse() {
    assert_eq!(
      "0000000000000000000000000000000000000000000000000000000000000000i0"
        .parse::<Outgoing>()
        .unwrap(),
      Outgoing::InscriptionId(
        "0000000000000000000000000000000000000000000000000000000000000000i0"
          .parse()
          .unwrap()
      ),
    );

    assert_eq!(
      "0000000000000000000000000000000000000000000000000000000000000000:0:0"
        .parse::<Outgoing>()
        .unwrap(),
      Outgoing::SatPoint(
        "0000000000000000000000000000000000000000000000000000000000000000:0:0"
          .parse()
          .unwrap()
      ),
    );

    assert_eq!(
      "0 sat".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("0 sat".parse().unwrap()),
    );

    assert_eq!(
      "0sat".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("0 sat".parse().unwrap()),
    );

    assert_eq!(
      "1pep".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("1 btc".parse().unwrap()),
    );

    assert_eq!(
      "1 pep".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("1 btc".parse().unwrap()),
    );

    assert_eq!(
      "100rib".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("100 sat".parse().unwrap()),
    );

    assert_eq!(
      "100ribbit".parse::<Outgoing>().unwrap(),
      Outgoing::Amount("100 sat".parse().unwrap()),
    );

    assert!("0".parse::<Outgoing>().is_err());
  }
}
