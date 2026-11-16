//! Statement splitter.
//!
//! Splits on top-level `;` while respecting single-quoted strings (with `''` and, for
//! PostgreSQL `E'..'`, backslash escapes), double-quoted identifiers, `--` line
//! comments, nested `/* */` block comments (PostgreSQL) and `$tag$` dollar quoting
//! (PostgreSQL). SQLite trigger bodies (`BEGIN ... END;`) are kept intact.

use crate::Dialect;

/// Byte range of one statement in the source text, trimmed of surrounding whitespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
}

pub fn split_statements(src: &str, dialect: Dialect) -> Vec<Span> {
    let bytes = src.as_bytes();
    let mut spans = Vec::new();
    let mut stmt_start = 0usize;
    let mut i = 0usize;
    // SQLite: depth of BEGIN...END inside CREATE TRIGGER.
    let mut trigger_depth = 0usize;
    let mut in_create_trigger = false;
    let mut word_start: Option<usize> = None;

    let push = |spans: &mut Vec<Span>, start: usize, end: usize| {
        if let Some(span) = trim_span(src, start, end) {
            spans.push(span);
        }
    };

    while i < bytes.len() {
        let c = bytes[i];

        // Track words for SQLite trigger detection.
        if dialect == Dialect::Sqlite && (c.is_ascii_alphanumeric() || c == b'_') {
            if word_start.is_none() {
                word_start = Some(i);
            }
            i += 1;
            if i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                continue;
            }
            let word = src[word_start.take().unwrap_or(i - 1)..i].to_ascii_lowercase();
            match word.as_str() {
                "trigger" if is_create_statement(src, stmt_start, i) => in_create_trigger = true,
                "begin" if in_create_trigger => trigger_depth += 1,
                "case" if in_create_trigger && trigger_depth > 0 => trigger_depth += 1,
                "end" if in_create_trigger && trigger_depth > 0 => trigger_depth -= 1,
                _ => {}
            }
            continue;
        }

        match c {
            b'\'' => {
                let backslash_escapes = dialect == Dialect::Postgres
                    && i > 0
                    && matches!(bytes[i - 1], b'E' | b'e')
                    && (i < 2 || !(bytes[i - 2].is_ascii_alphanumeric() || bytes[i - 2] == b'_'));
                i = skip_quoted(bytes, i, b'\'', backslash_escapes);
            }
            b'"' => i = skip_quoted(bytes, i, b'"', false),
            b'`' if dialect == Dialect::Sqlite => i = skip_quoted(bytes, i, b'`', false),
            b'[' if dialect == Dialect::Sqlite => {
                i = memchr(bytes, i + 1, b']').map_or(bytes.len(), |p| p + 1);
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i = memchr(bytes, i, b'\n').map_or(bytes.len(), |p| p + 1);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = skip_block_comment(bytes, i, dialect == Dialect::Postgres);
            }
            b'$' if dialect == Dialect::Postgres => match dollar_tag(bytes, i) {
                Some(tag_len) => {
                    let tag = &bytes[i..i + tag_len];
                    i = find_subslice(bytes, i + tag_len, tag).map_or(bytes.len(), |p| p + tag_len);
                }
                None => i += 1,
            },
            b';' if trigger_depth == 0 => {
                push(&mut spans, stmt_start, i);
                i += 1;
                stmt_start = i;
                in_create_trigger = false;
            }
            _ => i += 1,
        }
    }
    push(&mut spans, stmt_start, bytes.len());
    spans
}

/// The statement containing `byte_offset`. A cursor just after a statement's `;` (or in
/// whitespace after it) belongs to that preceding statement, matching editor intuition.
pub fn statement_at(src: &str, dialect: Dialect, byte_offset: usize) -> Option<Span> {
    let spans = split_statements(src, dialect);
    spans
        .iter()
        .find(|s| byte_offset >= s.start && byte_offset <= s.end)
        .or_else(|| spans.iter().rev().find(|s| s.end <= byte_offset))
        .or_else(|| spans.first())
        .copied()
}

/// Convert a UTF-16 code-unit offset (as reported by JS editors) to a byte offset.
pub fn utf16_to_byte_offset(src: &str, utf16: usize) -> usize {
    let mut units = 0usize;
    for (byte_idx, ch) in src.char_indices() {
        if units >= utf16 {
            return byte_idx;
        }
        units += ch.len_utf16();
    }
    src.len()
}

fn trim_span(src: &str, start: usize, end: usize) -> Option<Span> {
    let slice = &src[start..end];
    let lead = slice.len() - slice.trim_start().len();
    let trimmed = slice.trim();
    if trimmed.is_empty() || is_only_comments(trimmed) {
        return None;
    }
    Some(Span {
        start: start + lead,
        end: start + lead + trimmed.len(),
    })
}

fn is_only_comments(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
        } else if b[i] == b'-' && b.get(i + 1) == Some(&b'-') {
            i = memchr(b, i, b'\n').map_or(b.len(), |p| p + 1);
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_block_comment(b, i, true);
        } else {
            return false;
        }
    }
    true
}

fn is_create_statement(src: &str, stmt_start: usize, upto: usize) -> bool {
    src[stmt_start..upto]
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("create")
}

fn skip_quoted(b: &[u8], start: usize, quote: u8, backslash_escapes: bool) -> usize {
    let mut i = start + 1;
    while i < b.len() {
        if backslash_escapes && b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == quote {
            if b.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

fn skip_block_comment(b: &[u8], start: usize, nested: bool) -> usize {
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < b.len() {
        if b[i] == b'/' && b[i + 1] == b'*' {
            if depth == 0 || nested {
                depth += 1;
            }
            i += 2;
        } else if b[i] == b'*' && b[i + 1] == b'/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    b.len()
}

/// Length of a dollar-quote opening tag (`$$` or `$tag$`) at `i`, if any.
fn dollar_tag(b: &[u8], i: usize) -> Option<usize> {
    // `$1` positional parameters are not tags; a tag cannot start with a digit.
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return None;
    }
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'$' => return Some(j - i + 1),
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => j += 1,
            c if c.is_ascii_digit() && j > i + 1 => j += 1,
            _ => return None,
        }
    }
    None
}

fn memchr(b: &[u8], from: usize, needle: u8) -> Option<usize> {
    b[from..]
        .iter()
        .position(|&c| c == needle)
        .map(|p| p + from)
}

fn find_subslice(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str, d: Dialect) -> Vec<&str> {
        split_statements(src, d)
            .iter()
            .map(|s| s.text(src))
            .collect()
    }

    #[test]
    fn simple_split() {
        assert_eq!(
            texts("select 1; select 2;", Dialect::Postgres),
            ["select 1", "select 2"]
        );
        assert_eq!(texts("select 1", Dialect::Postgres), ["select 1"]);
        assert_eq!(texts("  ;; \n ", Dialect::Postgres), Vec::<&str>::new());
    }

    #[test]
    fn respects_strings_and_identifiers() {
        let src = "select 'a;b', \"c;d\" from t; select 'it''s;'";
        assert_eq!(
            texts(src, Dialect::Postgres),
            ["select 'a;b', \"c;d\" from t", "select 'it''s;'"]
        );
    }

    #[test]
    fn e_strings_allow_backslash_escaped_quotes() {
        let src = r"select E'a\';b'; select 2";
        assert_eq!(
            texts(src, Dialect::Postgres),
            [r"select E'a\';b'", "select 2"]
        );
        // A plain string does not treat backslash as an escape.
        let src = r"select 'a\'; select 2";
        assert_eq!(texts(src, Dialect::Postgres), [r"select 'a\'", "select 2"]);
    }

    #[test]
    fn respects_comments() {
        let src = "select 1 -- a;b\n; /* x; /* nested; */ y; */ select 2";
        assert_eq!(
            texts(src, Dialect::Postgres),
            ["select 1 -- a;b", "/* x; /* nested; */ y; */ select 2"]
        );
        assert_eq!(
            texts("-- only a comment;\n", Dialect::Postgres),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn dollar_quoting() {
        let src = "create function f() returns int as $$ select 1; $$ language sql; select $tag$ ; $$ ; $tag$; select $1";
        assert_eq!(
            texts(src, Dialect::Postgres),
            [
                "create function f() returns int as $$ select 1; $$ language sql",
                "select $tag$ ; $$ ; $tag$",
                "select $1"
            ]
        );
    }

    #[test]
    fn sqlite_trigger_body_kept_together() {
        let src = "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET x = CASE WHEN 1 THEN 2 END; INSERT INTO c VALUES (1); END; select 1";
        assert_eq!(
            texts(src, Dialect::Sqlite),
            [
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET x = CASE WHEN 1 THEN 2 END; INSERT INTO c VALUES (1); END",
                "select 1"
            ]
        );
        // BEGIN as a transaction statement is unaffected.
        assert_eq!(
            texts("BEGIN; select 1; COMMIT;", Dialect::Sqlite),
            ["BEGIN", "select 1", "COMMIT"]
        );
    }

    #[test]
    fn statement_under_cursor() {
        let src = "select 1;\nselect 2;\n\nselect 3";
        let at = |off| statement_at(src, Dialect::Postgres, off).map(|s| s.text(src));
        assert_eq!(at(0), Some("select 1"));
        assert_eq!(at(9), Some("select 1")); // right after `;`
        assert_eq!(at(12), Some("select 2"));
        assert_eq!(at(20), Some("select 2")); // blank line after statement 2
        assert_eq!(at(src.len()), Some("select 3"));
    }

    #[test]
    fn utf16_offsets() {
        let src = "select '😀', 'é'; select 2";
        // '😀' is 2 UTF-16 units and 4 bytes; 'é' is 1 unit and 2 bytes.
        let semi_utf16 = src.encode_utf16().position(|u| u == ';' as u16).unwrap();
        assert_eq!(
            utf16_to_byte_offset(src, semi_utf16),
            src.find(';').unwrap()
        );
        assert_eq!(utf16_to_byte_offset(src, 10_000), src.len());
    }
}
