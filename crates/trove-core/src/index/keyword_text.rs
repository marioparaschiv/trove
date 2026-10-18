//! Text the way keyword search keeps it and matches it, so that a word is found however it was typed: indexed text and
//! queries go through the same [`normalized`]. Ported from DigUp's `SearchText` (see THIRD_PARTY_NOTICES.md).

use rusqlite::Statement;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Whether `c` is part of a word, as the tokenizer splits text: letters, digits and marks, so that a vowel sign or a
/// virama stays inside its word.
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c)
}

/// NFKC (Arabic presentation forms and ligatures become plain letters, full-width becomes ASCII), invisible formatting
/// characters dropped (zero-width joiners, direction marks, soft hyphens), a zero-width space read as a word break
/// (Thai, Khmer), and Arabic the way people type it: no tatweel, no short-vowel marks, one alef.
pub(crate) fn normalized(text: &str) -> String {
    text.nfkc()
        .filter_map(|c| match c {
            '\u{200B}' => Some(' '),
            '\u{00AD}'
            | '\u{061C}'
            | '\u{200C}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}' => None,
            // Tatweel, harakat and the dagger alef.
            '\u{0640}' | '\u{064B}'..='\u{065F}' | '\u{0670}' => None,
            // Alef with hamza or madda, and alef wasla.
            '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' => Some('\u{0627}'),
            c => Some(c),
        })
        .collect()
}

/// Adds a chunk to keyword search, for [`insert`].
pub(crate) const INSERT: &str = "INSERT INTO chunks_fts (rowid, title, heading_path, text, original) VALUES (?1, ?2, ?3, ?4, ?5)";

/// Adds a chunk to keyword search through `statement`, prepared from [`INSERT`]: its title, heading path and text
/// normalised, and the text as written when normalising changed it.
pub(crate) fn insert(
    statement: &mut Statement<'_>,
    chunk_id: i64,
    title: &str,
    heading_path: &str,
    text: &str,
) -> rusqlite::Result<()> {
    let normalized_text = normalized(text);
    let original = (normalized_text != text).then_some(text);
    statement.execute((
        chunk_id,
        normalized(title),
        normalized(heading_path),
        &normalized_text,
        original,
    ))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arabic_loses_its_harakat_tatweel_and_alef_variants() {
        assert_eq!(normalized("مَدْرَسَة"), "مدرسة");
        assert_eq!(normalized("أحمد إلى آخر"), "احمد الى اخر");
        assert_eq!(normalized("كـتـاب"), "كتاب");
    }

    #[test]
    fn compatibility_forms_become_plain_and_invisible_characters_go() {
        assert_eq!(normalized("ＡＢＣ１２３"), "ABC123");
        assert_eq!(normalized("ﬁle"), "file");
        assert_eq!(normalized("co\u{00AD}operate\u{200D}"), "cooperate");
        assert_eq!(normalized("a\u{200B}b"), "a b");
    }
}
