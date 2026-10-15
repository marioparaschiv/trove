use trove_core::http::ModelStatus;

use super::Context;
use crate::service::Service;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    // Without a service no process holds the model.
    if let Some(service) = Service::connect(&ctx.data_dir)? {
        let _: ModelStatus = service.post_empty("/unload")?;
    }
    println!("model unloaded");

    Ok(())
}
