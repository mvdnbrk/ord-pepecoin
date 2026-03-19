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

    let parent = path.parent().unwrap();
    for entry in &batch_file.inscriptions {
      let file_path = if entry.file.is_absolute() {
        entry.file.clone()
      } else {
        parent.join(&entry.file)
      };
      if !file_path.exists() {
        bail!("file not found: {}", file_path.display());
      }
    }

    Ok(batch_file)
  }
}
