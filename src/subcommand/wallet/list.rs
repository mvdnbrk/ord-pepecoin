use super::*;

pub(crate) fn run(settings: Settings) -> Result {
  let wallets_dir = settings.data_dir().join("wallets");
  if !wallets_dir.exists() {
    print_json(Vec::<String>::new())?;
    return Ok(());
  }

  let mut wallets = Vec::new();
  for entry in fs::read_dir(&wallets_dir)? {
    let entry = entry?;
    if entry.file_type()?.is_dir() && entry.path().join("wallet.redb").exists() {
      wallets.push(entry.file_name().to_string_lossy().to_string());
    }
  }

  wallets.sort();
  print_json(&wallets)?;

  Ok(())
}
