use super::*;

#[derive(Debug, Parser)]
pub(crate) enum IndexSubcommand {
  #[clap(about = "Compact the index database")]
  Compact,
  #[clap(about = "Export index to TSV")]
  Export(Export),
  #[clap(about = "Update the index")]
  Run,
  #[clap(about = "Update the index")]
  Update,
}

impl IndexSubcommand {
  pub(crate) fn run(self, options: Options) -> Result {
    match self {
      Self::Compact => {
        let mut index = Index::open(&options)?;
        index.compact()
      }
      Self::Export(export) => export.run(options),
      Self::Run | Self::Update => run(options),
    }
  }
}

#[derive(Debug, Parser)]
pub(crate) struct Export {
  #[clap(long, help = "Write export to <TSV> file.")]
  pub(crate) tsv: Option<PathBuf>,
  #[clap(long, help = "Include addresses in export.")]
  pub(crate) include_addresses: bool,
}

impl Export {
  pub(crate) fn run(self, options: Options) -> Result {
    let index = Index::open(&options)?;

    index.update()?;

    index.export(self.tsv, self.include_addresses)?;

    Ok(())
  }
}

pub(crate) fn run(options: Options) -> Result {
  let index = Index::open(&options)?;

  index.update()?;

  Ok(())
}
