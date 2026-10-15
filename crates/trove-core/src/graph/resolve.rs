use std::collections::HashMap;

const NOTE_SUFFIX: &str = ".md";

/// The notes of each source, to find the note a link names the way Obsidian does: case-insensitive, `.md` optional,
/// relative to the linking note, from the source's root, or by file name (nearest first).
#[derive(Default)]
pub(super) struct NotePaths {
    /// Per source id: each note's comparable path and file id.
    by_source: HashMap<i64, Vec<(String, i64)>>,
}

impl NotePaths {
    pub(super) fn insert(&mut self, source_id: i64, path: &str, file_id: i64) {
        self.by_source
            .entry(source_id)
            .or_default()
            .push((comparable(path), file_id));
    }

    /// The note in `source_id` that `target` names when written in the note at `from`.
    pub(super) fn resolve(&self, source_id: i64, from: &str, target: &str) -> Option<i64> {
        let notes = self.by_source.get(&source_id)?;
        let exact = |path: &str| {
            notes
                .iter()
                .find(|(note, _)| note == path)
                .map(|&(_, file_id)| file_id)
        };

        let target = comparable(target.trim());
        if target.is_empty() {
            return None;
        }
        if let Some(from_root) = target.strip_prefix('/') {
            return exact(from_root);
        }

        let from = comparable(from);
        let from_folder = folder(&from);
        if let Some(file_id) = join(from_folder, &target).and_then(|joined| exact(&joined)) {
            return Some(file_id);
        }
        if target.starts_with("./") || target.starts_with("../") {
            return None;
        }
        if let Some(file_id) = exact(&target) {
            return Some(file_id);
        }

        let suffix = format!("/{target}");
        notes
            .iter()
            .filter(|(note, _)| note.ends_with(&suffix))
            .min_by_key(|(note, _)| (folder(note) != from_folder, note.len(), note.as_str()))
            .map(|&(_, file_id)| file_id)
    }
}

/// `path` lowercased, without the `.md` extension.
fn comparable(path: &str) -> String {
    let lower = path.to_lowercase();
    match lower.strip_suffix(NOTE_SUFFIX) {
        Some(stem) => stem.to_owned(),
        None => lower,
    }
}

fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// `relative` inside `folder`, with `.` and `..` worked out; `None` when it climbs out of the source.
fn join(folder: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = folder.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }

    Some(parts.join("/"))
}
