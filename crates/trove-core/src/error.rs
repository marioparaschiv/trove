use std::{error::Error, fmt::Write};

/// The error's message followed by each of its causes.
pub(crate) fn full_message(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        write!(message, ": {cause}").expect("writing to a String can't fail");
        source = cause.source();
    }

    message
}
