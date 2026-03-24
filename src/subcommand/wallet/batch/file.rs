use {
  super::*, crate::inscriptions::properties::TraitValue, serde::de::Deserializer, std::path::Path,
};

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
  pub(crate) title: Option<String>,
  #[serde(default, deserialize_with = "deserialize_traits")]
  pub(crate) traits: Option<Vec<(String, crate::inscriptions::properties::TraitValue)>>,
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

fn deserialize_traits<'de, D>(
  deserializer: D,
) -> std::result::Result<Option<Vec<(String, TraitValue)>>, D::Error>
where
  D: Deserializer<'de>,
{
  let value: Option<serde_yaml::Value> = Option::deserialize(deserializer)?;
  let mapping = match value {
    Some(serde_yaml::Value::Mapping(m)) => m,
    Some(_) => return Err(serde::de::Error::custom("traits must be a mapping")),
    None => return Ok(None),
  };

  let mut traits = Vec::new();
  for (k, v) in mapping {
    let key = k
      .as_str()
      .ok_or_else(|| serde::de::Error::custom("trait key must be a string"))?
      .to_string();
    let val = match v {
      serde_yaml::Value::String(s) => TraitValue::String(s),
      serde_yaml::Value::Bool(b) => TraitValue::Bool(b),
      serde_yaml::Value::Number(n) => {
        let n = n
          .as_i64()
          .ok_or_else(|| serde::de::Error::custom("trait value must be an integer, not float"))?;
        TraitValue::Integer(n)
      }
      serde_yaml::Value::Null => TraitValue::Null,
      _ => {
        return Err(serde::de::Error::custom(
          "trait value must be a string, boolean, integer, or null",
        ))
      }
    };
    traits.push((key, val));
  }

  Ok(Some(traits))
}
