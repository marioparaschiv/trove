use trove_core::http::{ServeError, Service};

use super::Context;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(ctx: &Context, _args: Args) -> anyhow::Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let service = match Service::start(&ctx.data_dir, &ctx.config).await {
            Ok(service) => service,
            // Clients start `trove serve` whenever they find no service, so losing that race is
            // normal: the other service writes the runtime file they wait for.
            Err(ServeError::AlreadyRunning) => {
                eprintln!("{}", ServeError::AlreadyRunning);
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };

        println!("listening on http://127.0.0.1:{}", service.runtime().port);
        service.run().await?;

        Ok(())
    })
}
