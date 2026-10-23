//! Counts the production and test lines of one crate's Rust files. It reads the text, not a syntax tree.
//!
//! Every line counts, including blank lines, comments and documentation, as `wc -l` counts them. These lines are test
//! lines, and every other line is production:
//!
//! - Every line of the crate's `tests/` and `benches/` files.
//! - Every line of a file named `tests.rs` or `*_tests.rs`, or in a `tests` folder below the crate's first folder.
//! - Every line of a module declared under `#[cfg(test)]` or `#[cfg(kani)]`, or `any` of the two, and of everything
//!   below it.
//! - Each item under either attribute inside another file.
//!
//! An item ends where its first brace closes, or at a `;` before any brace. Braces in comments and literals do not
//! count.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};

/// One crate's lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Lines {
    pub(crate) production: u64,
    pub(crate) tests: u64,
}

/// A file's lines as `wc -l` counts them: its line breaks.
fn counted(text: &str) -> u64 {
    text.bytes().filter(|&byte| byte == b'\n').count() as u64
}

/// Where the comment or literal starting at byte `start`, a character boundary, ends, or `start` when none starts there.
/// The end is a character boundary too.
fn literal_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let rest = &text[start..];
    if rest.starts_with("//") {
        return rest.find('\n').map_or(text.len(), |end| start + end);
    }
    if rest.starts_with("/*") {
        let (mut depth, mut index) = (1, start + 2);
        while index < text.len() && depth > 0 {
            if text[index..].starts_with("/*") {
                depth += 1;
                index += 2;
            } else if text[index..].starts_with("*/") {
                depth -= 1;
                index += 2;
            } else {
                index += char_length(text, index);
            }
        }
        return index;
    }
    if let Some(end) = raw_string_end(text, start).or_else(|| character_end(text, start)) {
        return end;
    }
    if rest.starts_with('"') || rest.starts_with("b\"") {
        let mut index = start + rest.find('"').map_or(0, |quote| quote + 1);
        while index < text.len() && bytes[index] != b'"' {
            if bytes[index] == b'\\' {
                index += 1;
                if index == text.len() {
                    break;
                }
            }
            index += char_length(text, index);
        }
        return (index + 1).min(text.len());
    }
    start
}

/// The length in bytes of the character starting at byte `index`, so every position the reader moves to is a character
/// boundary.
fn char_length(text: &str, index: usize) -> usize {
    text[index..].chars().next().map_or(1, char::len_utf8)
}

/// Where a raw string `r#"…"#` (or `br`) starting at `start` ends.
fn raw_string_end(text: &str, start: usize) -> Option<usize> {
    let rest = text[start..].strip_prefix('b').unwrap_or(&text[start..]);
    let after_r = rest.strip_prefix('r')?;
    let hashes = after_r.len() - after_r.trim_start_matches('#').len();
    let body = after_r[hashes..].strip_prefix('"')?;
    let opened = text.len() - body.len();
    let closing = format!("\"{}", "#".repeat(hashes));
    Some(
        text[opened..]
            .find(&closing)
            .map_or(text.len(), |end| opened + end + closing.len()),
    )
}

/// Where a character literal `'x'`, `'\n'`, `'\x41'` or `'\u{1F600}'` (or `b'…'`) starting at `start` ends. None
/// for a lifetime's quote. This recognises literal boundaries, not validity. It does not check the Unicode digit limit
/// or scalar value.
fn character_end(text: &str, start: usize) -> Option<usize> {
    let rest = text[start..].strip_prefix('b').unwrap_or(&text[start..]);
    let inside = rest.strip_prefix('\'')?;
    let after = if let Some(escaped) = inside.strip_prefix('\\') {
        if let Some(hex) = escaped.strip_prefix('x') {
            let digits = hex
                .get(..2)
                .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))?;
            &hex[digits.len()..]
        } else if let Some(unicode) = escaped.strip_prefix("u{") {
            if !unicode.starts_with(|character: char| character.is_ascii_hexdigit()) {
                return None;
            }
            unicode
                .trim_start_matches(|character: char| {
                    character.is_ascii_hexdigit() || character == '_'
                })
                .strip_prefix('}')?
        } else {
            let character = escaped.chars().next()?;
            &escaped[character.len_utf8()..]
        }
    } else {
        let character = inside
            .chars()
            .next()
            .filter(|&character| !matches!(character, '\\' | '\'' | '\n'))?;
        &inside[character.len_utf8()..]
    };
    let closed = after.strip_prefix('\'')?;
    Some(text.len() - closed.len())
}

/// The source with every comment and string, byte string and character literal blanked, its line breaks kept, so a
/// delimiter is read only where it is code.
fn code_only(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let end = literal_end(text, index);
        if end == index {
            let character = text[index..].chars().next().unwrap_or(' ');
            kept.push(character);
            index += character.len_utf8();
        } else {
            for character in text[index..end.min(text.len())].chars() {
                kept.push(if character == '\n' { '\n' } else { ' ' });
            }
            index = end;
        }
    }
    kept
}

/// Whether a line opens with a `cfg` attribute for tests or proofs: `test`, `kani`, or `any` of the two.
fn test_attribute(line: &str) -> bool {
    let squeezed: String = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    [
        "test",
        "kani",
        "any(test,kani)",
        "any(kani,test)",
        "any(test,test)",
        "any(kani,kani)",
    ]
    .iter()
    .any(|inside| squeezed.starts_with(&format!("#[cfg({inside})]")))
}

/// The module a line declares with `mod name;`, optionally public.
fn declared_module(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    if let Some(public) = rest.strip_prefix("pub") {
        let after = match public.strip_prefix('(') {
            Some(scoped) => &scoped[scoped.find(')')? + 1..],
            None => public,
        };
        if !after.starts_with(char::is_whitespace) {
            return None;
        }
        rest = after.trim_start();
    }
    let after = rest.strip_prefix("mod")?;
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    let name_start = after.trim_start();
    let length = name_start.len()
        - name_start
            .trim_start_matches(|character: char| character.is_alphanumeric() || character == '_')
            .len();
    let name = &name_start[..length];
    (!name.is_empty() && name_start[length..].trim_start().starts_with(';')).then_some(name)
}

/// The index of the last line of the item starting at line `start`: where its first `{` closes, or its line when a
/// `;` ends it first.
fn item_end(lines: &[&str], start: usize) -> usize {
    let (mut depth, mut opened) = (0_i64, false);
    for (index, line) in lines.iter().enumerate().skip(start) {
        for character in line.chars() {
            match character {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth -= 1,
                ';' if !opened => return index,
                _ => {}
            }
        }
        if opened && depth <= 0 {
            return index;
        }
    }
    lines.len().saturating_sub(1)
}

/// The modules a file declares under a test attribute, as paths without `.rs`, and how many of its lines the test
/// items inside it hold.
fn test_parts(path: &Utf8Path, text: &str) -> (Vec<Utf8PathBuf>, u64) {
    let blanked = code_only(text);
    let lines: Vec<&str> = blanked.split('\n').collect();
    let mut children = Vec::new();
    let mut tested = vec![false; lines.len()];
    let mut index = 0;
    while index < lines.len() {
        if !test_attribute(lines[index]) {
            index += 1;
            continue;
        }
        // The attribute line itself is skipped as an attribute.
        let mut item = index;
        while item < lines.len()
            && (lines[item].trim_start().starts_with("#[") || lines[item].trim().is_empty())
        {
            item += 1;
        }
        if item == lines.len() {
            break;
        }
        let end = item_end(&lines, item);
        if let Some(name) = declared_module(lines[item]) {
            let folder = if matches!(path.file_name(), Some("lib.rs" | "main.rs")) {
                path.parent().map_or_default(Utf8Path::to_owned)
            } else {
                path.with_extension("")
            };
            children.push(folder.join(name));
        }
        for marked in &mut tested[index..=end] {
            *marked = true;
        }
        index = end + 1;
    }
    let total = usize::try_from(counted(text)).unwrap_or(usize::MAX);
    let inside = tested.iter().take(total).filter(|&&marked| marked).count() as u64;
    (children, inside)
}

/// One crate's lines, its files by their paths inside the crate's folder.
pub(crate) fn crate_lines(files: &BTreeMap<Utf8PathBuf, String>) -> Lines {
    let parts: BTreeMap<&Utf8PathBuf, (Vec<Utf8PathBuf>, u64)> = files
        .iter()
        .map(|(path, text)| (path, test_parts(path, text)))
        .collect();
    let modules: Vec<&Utf8PathBuf> = parts.values().flat_map(|(children, _)| children).collect();
    let mut lines = Lines::default();
    for (path, text) in files {
        let first = path.components().next().map(|part| part.as_str());
        let name = path.file_name().unwrap_or_default();
        let whole = matches!(first, Some("tests" | "benches"))
            || name == "tests.rs"
            || name.ends_with("_tests.rs")
            || path.parent().is_some_and(|folder| {
                folder
                    .components()
                    .skip(1)
                    .any(|part| part.as_str() == "tests")
            })
            || modules.iter().any(|module| {
                path.with_extension("") == **module || path.starts_with(module.as_path())
            });
        let total = counted(text);
        let inside = if whole { total } else { parts[path].1 };
        lines.tests += inside;
        lines.production += total - inside;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_of(files: &[(&str, &str)]) -> Lines {
        crate_lines(
            &files
                .iter()
                .map(|(path, text)| (Utf8PathBuf::from(path), (*text).to_owned()))
                .collect(),
        )
    }

    #[test]
    fn test_and_bench_files_are_test_lines_whole() {
        let lines = crate_of(&[
            ("src/lib.rs", "pub fn f() {}\n\n"),
            ("tests/scenarios.rs", "#[test]\nfn t() {}\n"),
            ("benches/speed.rs", "fn main() {}\n"),
            ("src/parse/tests.rs", "fn a() {}\n"),
            ("src/parse_tests.rs", "fn b() {}\n"),
            ("src/deep/tests/helpers.rs", "fn c() {}\n"),
            ("src/deep/reader.rs", "fn d() {}\n"),
        ]);
        // lib.rs has two production lines, the blank one included, and reader.rs one. The five other files are tests:
        // 2 + 1 + 1 + 1 + 1.
        assert_eq!(
            lines,
            Lines {
                production: 3,
                tests: 6
            }
        );
    }

    #[test]
    fn items_and_modules_under_a_test_or_proof_attribute_are_test_lines() {
        let lib = "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n\n#[cfg(kani)]\n#[kani::proof]\nfn proof() {\n}\n#[cfg(any(test, kani))]\nmod checks;\n";
        let lines = crate_of(&[
            ("src/lib.rs", lib),
            ("src/checks.rs", "fn x() {}\n\n"),
            ("src/checks/more.rs", "fn y() {}\n"),
        ]);
        // lib.rs's 13 lines: `pub fn f`, and the blank line between the two items, are production. The tests module
        // (4 lines and its attribute), the proof (3 lines and two attributes) and `mod checks;` with its attribute
        // are tests. checks.rs and everything under checks/ are tests whole.
        assert_eq!(
            lines,
            Lines {
                production: 2,
                tests: 11 + 2 + 1
            }
        );
    }

    #[test]
    fn braces_in_strings_comments_and_characters_do_not_end_an_item() {
        let lib = "#[cfg(test)]\nfn t() {\n    let s = \"}\";\n    let r = r#\"}\"#;\n    let c = '}';\n    // }\n}\nfn after() {}\n";
        assert_eq!(
            crate_of(&[("src/lib.rs", lib)]),
            Lines {
                production: 1,
                tests: 7
            }
        );
    }

    #[test]
    fn a_lifetime_is_not_a_character_literal() {
        let lib = "#[cfg(test)]\nfn t<'a>(x: &'a str) {\n}\nfn after() {}\n";
        assert_eq!(
            crate_of(&[("src/lib.rs", lib)]),
            Lines {
                production: 1,
                tests: 3
            }
        );
    }

    /// A text made of code and literals, as written, and what the code reader leaves of it: every literal and
    /// comment blanked character for character.
    fn spans(parts: &[(&str, bool)]) -> (String, String) {
        let text = parts.iter().map(|(part, _)| *part).collect();
        let left = parts
            .iter()
            .map(|(part, literal)| {
                if *literal {
                    " ".repeat(part.chars().count())
                } else {
                    (*part).to_owned()
                }
            })
            .collect();
        (text, left)
    }

    #[test]
    fn comments_and_every_kind_of_literal_are_blanked_and_code_is_kept() {
        let (text, left) = spans(&[
            ("let a = ", false),
            ("/* x /* nested */ still a comment */", true),
            (" b ", false),
            ("// to the end of the line {", true),
            ("\n", false),
            ("\"s\\\"}\"", true),
            (" ", false),
            ("r#\"a\"}\"#", true),
            (" ", false),
            ("br\"{\"", true),
            (" ", false),
            ("b\"}\"", true),
            (" ", false),
            ("'}'", true),
            (" ", false),
            ("'\\''", true),
            (" ", false),
            ("'\\x7d'", true),
            (" ", false),
            ("'\\u{7D}'", true),
            (" ", false),
            ("b'}'", true),
            (" é x<'a> {}\n", false),
        ]);
        assert_eq!(code_only(&text), left);
    }

    #[test]
    fn a_unicode_escape_with_underscores_after_its_first_digit_is_a_character_literal() {
        for escape in [
            "'\\u{7_d}'",
            "'\\u{7d_}'",
            "'\\u{7__d}'",
            "'\\u{7_d__}'",
            "'\\u{1_0_F_6_0_0}'",
        ] {
            let (text, left) = spans(&[("x ", false), (escape, true), (" {}", false)]);
            assert_eq!(code_only(&text), left, "{escape}");
        }
    }

    #[test]
    fn a_unicode_escape_starting_with_an_underscore_is_not_a_character_literal() {
        for escape in ["'\\u{_7d}'", "'\\u{__}'", "'\\u{}'"] {
            assert_eq!(character_end(escape, 0), None, "{escape}");
        }
    }

    #[test]
    fn comments_and_strings_holding_characters_of_several_bytes_are_blanked_whole() {
        let (text, left) = spans(&[
            ("x ", false),
            ("/* café */", true),
            (" y ", false),
            ("\"é\\é\"", true),
            (" ", false),
            ("\"\\", true),
        ]);
        assert_eq!(code_only(&text), left);
    }

    #[test]
    fn a_literal_or_comment_left_open_is_blanked_to_the_end() {
        for (code, open) in [
            ("x ", "/* never closed {"),
            ("x ", "\"never closed {"),
            ("x ", "r##\"never \"# closed"),
        ] {
            let (text, left) = spans(&[(code, false), (open, true)]);
            assert_eq!(code_only(&text), left, "{open}");
        }
    }

    #[test]
    fn a_module_declaration_is_read_with_any_visibility() {
        for (line, name) in [
            ("mod a;", Some("a")),
            ("    pub mod b ;", Some("b")),
            ("pub(crate) mod c;", Some("c")),
            ("pub(in crate::x) mod d;", Some("d")),
            ("mod e {", None),
            ("module f;", None),
            ("pubmod g;", None),
            ("modh;", None),
            ("mod ;", None),
        ] {
            assert_eq!(declared_module(line), name, "{line}");
        }
    }

    #[test]
    fn an_item_ends_where_its_first_brace_closes_or_at_a_semicolon_before_any() {
        assert_eq!(item_end(&["use a::b;", "fn f() {}"], 0), 0);
        assert_eq!(item_end(&["fn f() {", "    g();", "}", "x"], 0), 2);
        assert_eq!(item_end(&["struct S", "{", "    a: u8,", "}"], 0), 3);
        assert_eq!(item_end(&["fn f() { }"], 0), 0);
        assert_eq!(item_end(&["fn f() {"], 0), 0);
    }

    #[test]
    fn a_test_attribute_covers_its_item_with_any_attributes_and_blank_lines_between() {
        let lib = "#[cfg(test)]\n\n#[allow(dead_code)]\nuse x::y;\nfn after() {}\n#[cfg(test)]\n";
        // The first attribute, the blank line, the other attribute and the `use`. A last attribute with no item
        // marks nothing.
        assert_eq!(test_parts(Utf8Path::new("src/lib.rs"), lib).1, 4);
        assert_eq!(
            crate_of(&[("src/lib.rs", lib)]),
            Lines {
                production: 2,
                tests: 4
            }
        );
    }
}
