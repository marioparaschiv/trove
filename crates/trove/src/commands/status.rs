use trove_core::{http::Status, index::Index};

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    let stats = match Service::connect(&ctx.data_dir)? {
        Some(service) => {
            let status: Status = service.get("/status")?;
            println!(
                "service: running (pid {}, port {})",
                status.pid,
                service.runtime().port
            );

            status.index
        }
        None => {
            println!("service: not running");

            Index::open(&ctx.data_dir)?.stats()?
        }
    };

    println!("index: {} files, {} chunks", stats.files, stats.chunks);

    Ok(())
}
