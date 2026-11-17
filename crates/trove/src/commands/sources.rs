use std::path::{self, Path, PathBuf};

use anyhow::Context as _;
use trove_core::{
    http::SourcePath,
    index::{Index, Source},
};

use super::Context;
use crate::service::Service;

#[derive(clap::Subcommand)]
pub enum Command {
    /// Register a folder, or a single text file, as a source
    Add { path: PathBuf },
    /// Stop indexing a source
    Remove { path: PathBuf },
    /// List registered sources
    List,
}

pub fn run(ctx: &Context, command: Command) -> anyhow::Result<()> {
    let service = Service::connect(&ctx.data_dir)?;

    match command {
        Command::Add { path } => {
            let path = absolute(&path)?;
            let source: Source = match service {
                Some(service) => service.post("/sources", &SourcePath { path })?,
                None => Index::open(&ctx.data_dir)?.add_source(Path::new(&path))?,
            };
            println!("added {}", source.path);
        }
        Command::Remove { path } => {
            let path = absolute(&path)?;
            let source: Source = match service {
                Some(service) => service.delete("/sources", &[("path", &path)])?,
                None => Index::open(&ctx.data_dir)?.remove_source(Path::new(&path))?,
            };
            println!("removed {}", source.path);
        }
        Command::List => {
            let sources: Vec<Source> = match service {
                Some(service) => service.get("/sources")?,
                None => Index::open(&ctx.data_dir)?.sources()?,
            };
            for source in sources {
                println!("{}", source.path);
            }
        }
    }

    Ok(())
}

/// The service may run in another folder, so it gets absolute paths only.
fn absolute(source: &Path) -> anyhow::Result<String> {
    let path = path::absolute(source)
        .with_context(|| format!("failed to resolve {}", source.display()))?;

    path.into_os_string()
        .into_string()
        .map_err(|path| anyhow::anyhow!("{} is not valid Unicode", PathBuf::from(path).display()))
}
