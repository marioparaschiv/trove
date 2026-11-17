use std::{
    fs, io,
    path::{Path, PathBuf},
};

use ignore::WalkBuilder;

use super::scan::{IndexProblem, IndexReport};

/// Extensions of images, audio and video, which are recognised and skipped until media can be indexed.
const MEDIA_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif", "svg",
    "ico", "mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aif", "aiff", "mp4",
    "m4v", "mov", "mkv", "webm", "avi", "wmv", "mpg", "mpeg", "3gp",
];

/// A file found in a source.
pub(super) struct Found {
    pub(super) path: PathBuf,
    /// Relative to the source's folder, `/`-separated.
    pub(super) relative: String,
}

/// What listing a source found.
#[derive(Default)]
pub(super) struct Listing {
    pub(super) found: Vec<Found>,
    /// Folders and entries that could not be listed in full, relative to the source.
    pub(super) unlisted: Vec<String>,
}

/// The files a source holds that may be text: the file itself for a file source, and for a folder every file under it
/// except media, hidden entries (`.obsidian`, `.trash`, as Obsidian does), the store folder, symbolic links and what
/// `.gitignore` and `.ignore` files leave out. Fails only when the source itself can't be read; a subfolder or an entry
/// that can't be is reported and recorded as unlisted.
pub(super) fn list(
    source: &Path,
    store_folder: &str,
    report: &mut IndexReport,
) -> io::Result<Listing> {
    let mut listing = Listing::default();
    if fs::metadata(source)?.is_file() {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::other("the name is not valid Unicode"))?;
        if !is_media(name) {
            listing.found.push(Found {
                path: source.to_path_buf(),
                relative: name.to_owned(),
            });
        }

        return Ok(listing);
    }

    fs::read_dir(source)?;
    let store_folder = store_folder.to_owned();
    let walk = WalkBuilder::new(source)
        .hidden(true)
        .git_global(false)
        .require_git(false)
        .sort_by_file_name(Ord::cmp)
        .filter_entry(move |entry| {
            entry.depth() == 0
                || entry
                    .file_name()
                    .to_str()
                    .is_none_or(|name| !name.starts_with('.') && name != store_folder)
        })
        .build();

    for entry in walk {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let path = error_path(&error);
                report
                    .problems
                    .push(IndexProblem::new(path.unwrap_or(source), &error));
                listing.unlisted.push(
                    path.and_then(|path| relative(source, path))
                        .unwrap_or_default(),
                );
                continue;
            }
        };
        // A broken ignore file still leaves the rest of the folder to list.
        if let Some(error) = entry.error() {
            report.problems.push(IndexProblem::new(entry.path(), error));
        }
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }

        let Some(child) = relative(source, entry.path()) else {
            report.problems.push(IndexProblem {
                path: entry.path().display().to_string(),
                message: "the name is not valid Unicode".to_owned(),
            });
            continue;
        };
        if !is_media(&child) {
            listing.found.push(Found {
                path: entry.into_path(),
                relative: child,
            });
        }
    }

    Ok(listing)
}

/// `path` relative to `source`, `/`-separated, or `None` when it is outside it or not valid Unicode.
fn relative(source: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path
        .strip_prefix(source)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect();

    Some(parts?.join("/"))
}

/// The path a walk error is about, if it names one.
fn error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            error_path(err)
        }
        ignore::Error::Loop { child, .. } => Some(child),
        ignore::Error::Partial(errors) => errors.iter().find_map(error_path),
        _ => None,
    }
}

/// Whether the file at `path` is an image, audio or video by its extension.
pub(crate) fn is_media(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MEDIA_EXTENSIONS
                .iter()
                .any(|media| extension.eq_ignore_ascii_case(media))
        })
}
