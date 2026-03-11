use super::*;

pub(crate) struct Rtx(pub(crate) redb::ReadTransaction);

impl Rtx {
  pub(crate) fn height(&self) -> Result<Option<Height>> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .next_back()
        .transpose()?
        .map(|(height, _hash)| Height(height.value())),
    )
  }

  pub(crate) fn block_count(&self) -> Result<u64> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .next_back()
        .transpose()?
        .map(|(height, _hash)| height.value() + 1)
        .unwrap_or(0),
    )
  }
}
