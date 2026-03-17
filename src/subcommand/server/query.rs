use super::*;

#[derive(Copy, Clone, Debug)]
pub(crate) enum Inscription {
  Id(InscriptionId),
  Number(u32),
}

impl FromStr for Inscription {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if s.contains('i') {
      Ok(Self::Id(s.parse()?))
    } else {
      Ok(Self::Number(s.parse()?))
    }
  }
}

impl Display for Inscription {
  fn fmt(&self, f: &mut Formatter) -> fmt::Result {
    match self {
      Self::Id(id) => write!(f, "{id}"),
      Self::Number(number) => write!(f, "{number}"),
    }
  }
}
