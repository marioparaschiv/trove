use trove_core::{http::Status, index::Index};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    let (stats, loaded) = match Service::connect(&ctx.data_dir)? {
        Some(service) => {
            let status: Status = service.get("/status")?;
            println!(
                "service: running (pid {}, port {})",
                status.pid,
                service.runtime().port
            );

            (status.index, status.model.loaded)
        }
        None => {
            println!("service: not running");

            // Only a service keeps the model loaded.
            (Index::open(&ctx.data_dir)?.stats()?, false)
        }
    };

    println!("index: {} files, {} chunks", stats.files, stats.chunks);
    println!("model: {}", if loaded { "loaded" } else { "not loaded" });

    Ok(())
}
