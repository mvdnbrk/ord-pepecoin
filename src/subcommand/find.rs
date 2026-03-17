use super::*;

#[derive(Debug, Parser)]
pub(crate) struct Find {
  #[clap(help = "Find output and offset of <SAT>.")]
  sat: Sat,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Output {
  pub satpoint: SatPoint,
}

impl Find {
  pub(crate) fn run(self, settings: Settings) -> Result {
    let index = Index::open(&settings)?;

    index.update()?;

    match index.find(self.sat.0)? {
      Some(satpoint) => {
        print_json(Output { satpoint })?;
        Ok(())
      }
      None => Err(anyhow!("sat has not been mined as of index height")),
    }
  }
}
