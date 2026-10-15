use std::sync::{Arc, Mutex};

use trove_core::{
    embedder::Embedder,
    index::Index,
    indexer::{self, IndexReport},
};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    let report: IndexReport = match Service::connect(&ctx.data_dir)? {
        Some(service) => service.post_empty("/index")?,
        None => indexer::index_sources(
            &Mutex::new(Index::open(&ctx.data_dir)?),
            &Arc::new(Embedder::from_config(&ctx.data_dir, &ctx.config)),
            &ctx.config,
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
