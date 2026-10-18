use std::sync::{Arc, Mutex};

use trove_core::{
    embedder::{Embedder, Profile},
    http::IndexRequest,
    index::Index,
    indexer::{self, IndexMode, IndexReport},
};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {
    /// Read and chunk every note, not only changed ones. Still embeds only new chunk texts.
    #[arg(long)]
    full: bool,
    /// Embed every chunk again. Implies --full.
    #[arg(long)]
    reembed: bool,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let mode = if args.reembed {
        IndexMode::Reembed
    } else if args.full {
        IndexMode::Full
    } else {
        IndexMode::Changed
    };

    let report: IndexReport = match Service::connect(&ctx.data_dir)? {
        Some(service) => service.post("/index", &IndexRequest { mode })?,
        None => indexer::index_sources(
            &Mutex::new(Index::open(&ctx.data_dir)?),
            &Arc::new(Embedder::from_config(&ctx.data_dir, &ctx.config)),
            &ctx.config,
            mode,
            Profile::Foreground,
            &mut |_| {},
        )?,
    };

    for problem in &report.problems {
        eprintln!("warning: {}: {}", problem.path, problem.message);
    }
    println!(
        "indexed {} notes ({} removed), embedded {} chunks",
        report.files, report.removed, report.embedded
    );

    Ok(())
}
