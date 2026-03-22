use {super::*, std::path::Path};

#[derive(Deserialize)]
pub(crate) struct BatchFile {
  #[serde(default)]
  pub(crate) parents: Vec<InscriptionId>,
  pub(crate) inscriptions: Vec<BatchEntry>,
}

#[derive(Deserialize)]
pub(crate) struct BatchEntry {
  pub(crate) file: Option<PathBuf>,
  pub(crate) delegate: Option<InscriptionId>,
  pub(crate) destination: Option<Address>,
}

impl BatchFile {
  pub(crate) fn load(path: &Path) -> Result<Self> {
    let batch_file: BatchFile =
      serde_yaml::from_reader(File::open(path)?).context("failed to parse batch file")?;

    if batch_file.inscriptions.is_empty() {
      bail!("batch file contains no inscriptions");
    }

    let parent = path.parent().unwrap();
    for entry in &batch_file.inscriptions {
      if entry.file.is_some() && entry.delegate.is_some() {
        bail!("batch entry cannot have both `file` and `delegate` set");
      }

      if let Some(ref file) = entry.file {
        let file_path = if file.is_absolute() {
          file.clone()
        } else {
          parent.join(file)
        };
        if !file_path.exists() {
          bail!("file not found: {}", file_path.display());
        }
      } else if entry.delegate.is_none() {
        bail!("batch entry must have either `file` or `delegate` set");
      }
    }

    Ok(batch_file)
  }
}
