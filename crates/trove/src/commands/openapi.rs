use super::Context;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(_ctx: &Context, _args: Args) -> anyhow::Result<()> {
    println!("{}", trove_core::http::openapi().to_pretty_json()?);

    Ok(())
}
