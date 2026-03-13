use {super::*, std::path::Path};

#[derive(Deserialize)]
pub(crate) struct BatchFile {
  pub(crate) inscriptions: Vec<BatchEntry>,
}

#[derive(Deserialize)]
pub(crate) struct BatchEntry {
  pub(crate) file: PathBuf,
  pub(crate) destination: Option<Address>,
}

impl BatchFile {
  pub(crate) fn load(path: &Path) -> Result<Self> {
    let batch_file: BatchFile = serde_yaml::from_reader(File::open(path)?)
      .context("failed to parse batch file")?;

    if batch_file.inscriptions.is_empty() {
      bail!("batch file contains no inscriptions");
    }

    Ok(batch_file)
  }

  pub(crate) fn inscriptions(
    &self,
    chain: Chain,
    batch_path: &Path,
    client: &Client,
  ) -> Result<(Vec<Inscription>, Vec<Address>)> {
    let mut inscriptions = Vec::new();
    let mut destinations = Vec::new();

    for entry in &self.inscriptions {
      let path = if entry.file.is_absolute() {
        entry.file.clone()
      } else {
        batch_path.parent().unwrap().join(&entry.file)
      };

      inscriptions.push(Inscription::from_file(chain, &path)?);
      destinations.push(
        entry
          .destination
          .clone()
          .map(Ok)
          .unwrap_or_else(|| get_change_address(client))?,
      );
    }

    Ok((inscriptions, destinations))
  }
}
