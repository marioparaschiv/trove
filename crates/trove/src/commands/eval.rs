use anyhow::bail;

use super::Context;

#[derive(clap::Args)]
pub struct Args {}

pub fn run(_ctx: &Context, _args: Args) -> anyhow::Result<()> {
    bail!("`trove eval` is not implemented yet")
}
