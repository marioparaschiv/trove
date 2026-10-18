use trove_core::{
    http::{ModelStatus, Status},
    index::Index,
};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    let (stats, model) = match Service::connect(&ctx.data_dir)? {
        Some(service) => {
            let status: Status = service.get("/status")?;
            println!(
                "service: running (pid {}, port {})",
                status.pid,
                service.runtime().port
            );

            (status.index, status.model)
        }
        None => {
            println!("service: not running");

            // Only a service keeps the model loaded.
            let model = ModelStatus {
                loaded: false,
                device: None,
            };
            (Index::open(&ctx.data_dir)?.stats()?, model)
        }
    };

    println!("index: {} files, {} chunks", stats.files, stats.chunks);
    match (model.loaded, model.device) {
        (true, Some(device)) => println!("model: loaded on {device}"),
        (true, None) => println!("model: loaded"),
        (false, _) => println!("model: not loaded"),
    }

    Ok(())
}
