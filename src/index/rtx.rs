use super::*;

pub(crate) struct Rtx<'a>(pub(crate) redb::ReadTransaction<'a>);

impl Rtx<'_> {
  pub(crate) fn height(&self) -> Result<Option<Height>> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .rev()
        .next()
        .map(|result| {
          let (height, _hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
          Height(height.value())
        }),
    )
  }

  pub(crate) fn block_count(&self) -> Result<u64> {
    Ok(
      self
        .0
        .open_table(HEIGHT_TO_BLOCK_HASH)?
        .range(0..)?
        .rev()
        .next()
        .map(|result| {
          let (height, _hash) = result.expect("Error reading from HEIGHT_TO_BLOCK_HASH table");
          height.value() + 1
        })
        .unwrap_or(0),
    )
  }
}
