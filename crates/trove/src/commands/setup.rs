use std::io::{self, Write};

use trove_core::model_setup::{self, Event, Source};

use super::Context;

#[derive(clap::Args)]
pub struct Args {
    /// Print progress as JSON lines
    #[arg(long)]
    json: bool,
}

pub fn run(ctx: &Context, args: Args) -> anyhow::Result<()> {
    let mut printer = Printer {
        out: io::stdout().lock(),
        json: args.json,
        mid_line: false,
    };
    let mut print_error = None;

    let result = model_setup::setup(&ctx.data_dir, &ctx.config, &mut |event| {
        if print_error.is_none() {
            print_error = printer.print(&event).err();
        }
    });

    result?;
    print_error.map_or(Ok(()), |error| Err(error.into()))
}

struct Printer<W> {
    out: W,
    json: bool,
    /// A progress line ends in `\r` so the next one overwrites it; anything else starts a new line first.
    mid_line: bool,
}

impl<W: Write> Printer<W> {
    fn print(&mut self, event: &Event) -> io::Result<()> {
        if self.json {
            serde_json::to_writer(&mut self.out, event)?;
            return writeln!(self.out);
        }

        if let Event::Progress { bytes, total, .. } = event {
            self.mid_line = true;
            write!(self.out, "\r{}%", bytes * 100 / total)?;
            return self.out.flush();
        }

        if self.mid_line {
            self.mid_line = false;
            writeln!(self.out)?;
        }

        match event {
            Event::File {
                name,
                from: Source::Cache,
                ..
            } => writeln!(self.out, "Copying {name} from the Hugging Face cache"),
            Event::File {
                name,
                bytes,
                from: Source::Download,
            } => writeln!(self.out, "Downloading {name} ({} MB)", bytes / 1_000_000),
            Event::Waiting => writeln!(self.out, "Waiting for a network connection"),
            Event::Verifying { name } => writeln!(self.out, "Checking {name}"),
            Event::Retry { message, seconds } => {
                writeln!(self.out, "{message}; trying again in {seconds}s")
            }
            Event::Done { path } => writeln!(self.out, "Model ready at {}", path.display()),
            // main prints the error itself.
            Event::Progress { .. } | Event::Error { .. } => Ok(()),
        }
    }
}
