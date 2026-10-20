//! The references a file makes, read by a small lexer rather than the grammar, so a file the grammar cannot parse
//! still counts. It keeps whole identifier tokens and the words inside string literals, and skips comments, numbers
//! and operators. Scripts comment with `#`; scenes, resources and `project.godot` with `;`.

/// How a file's comments start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// A `.gd` script: `#` to the end of the line.
    Script,
    /// A `.tscn`, `.tres` or `.godot` file: `;` to the end of the line.
    Resource,
}

/// How a name occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Use {
    /// An identifier token followed by a plain `=`, as `x = …`, `self.x = …` or a resource's `x = …` line.
    Assigned,
    /// Any other identifier token.
    Named,
    /// A word inside a string, StringName or NodePath literal.
    Quoted,
}

/// One occurrence of a name: the name, how it occurs, and its byte offset in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Occurrence {
    pub(crate) name: String,
    pub(crate) how: Use,
    pub(crate) offset: usize,
}

/// The operators longer than one character that end in `=`, longest first, so none is read as a plain `=`.
const OPERATORS: [&str; 16] = [
    "**=", "<<=", ">>=", "==", "!=", "<=", ">=", ":=", "+=", "-=", "*=", "/=", "%=", "&=", "|=",
    "^=",
];

enum Token {
    Identifier { start: usize, end: usize },
    Assign,
    Other,
}

fn identifier_start(character: char) -> bool {
    character == '_' || character.is_alphabetic()
}

fn identifier_part(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

/// Every name `text` uses, in order.
pub(crate) fn occurrences(text: &str, dialect: Dialect) -> Vec<Occurrence> {
    let comment = match dialect {
        Dialect::Script => '#',
        Dialect::Resource => ';',
    };
    let mut found = Vec::new();
    let mut tokens: Vec<Token> = Vec::new();
    let mut at = 0;
    while let Some(character) = text[at..].chars().next() {
        let rest = &text[at..];
        if character == comment {
            at += rest.find('\n').unwrap_or(rest.len());
        } else if let Some(continuation) = ["\\\n", "\\\r\n"]
            .iter()
            .find(|ending| rest.starts_with(**ending))
        {
            // A line continuation joins two lines, as whitespace does.
            at += continuation.len();
        } else if character.is_whitespace() {
            at += character.len_utf8();
        } else if let Some(Opening { prefix, quote, raw }) = string_start(rest) {
            let opened = at + prefix + quote.len();
            let (contents, end) = string_body(text, opened, quote, raw);
            words(&contents, opened, &mut found);
            tokens.push(Token::Other);
            at = end;
        } else if identifier_start(character) {
            let end = rest
                .char_indices()
                .find(|&(_, next)| !identifier_part(next))
                .map_or(text.len(), |(index, _)| at + index);
            tokens.push(Token::Identifier { start: at, end });
            at = end;
        } else if character.is_ascii_digit() {
            at += number_length(rest);
            tokens.push(Token::Other);
        } else if let Some(operator) = OPERATORS
            .iter()
            .find(|operator| rest.starts_with(**operator))
        {
            at += operator.len();
            tokens.push(Token::Other);
        } else {
            at += character.len_utf8();
            tokens.push(if character == '=' {
                Token::Assign
            } else {
                Token::Other
            });
        }
    }
    for (index, token) in tokens.iter().enumerate() {
        if let Token::Identifier { start, end } = *token {
            let assigned = matches!(tokens.get(index + 1), Some(Token::Assign));
            found.push(Occurrence {
                name: text[start..end].to_owned(),
                how: if assigned { Use::Assigned } else { Use::Named },
                offset: start,
            });
        }
    }
    found.sort_by_key(|occurrence| occurrence.offset);
    found
}

/// How a string literal opens: the length of its prefix, its quote, and whether it is raw.
struct Opening {
    prefix: usize,
    quote: &'static str,
    raw: bool,
}

/// The string literal starting here, if one does. `r` before a quote makes a raw string. A StringName's `&` and a
/// NodePath's `^` are tokens of their own, so the quote after them starts an ordinary string.
fn string_start(rest: &str) -> Option<Opening> {
    let (prefix, raw) = match rest.as_bytes().first() {
        Some(b'r') => (1, true),
        _ => (0, false),
    };
    quote(&rest[prefix..]).map(|quote| Opening { prefix, quote, raw })
}

fn quote(rest: &str) -> Option<&'static str> {
    ["\"\"\"", "'''", "\"", "'"]
        .into_iter()
        .find(|quote| rest.starts_with(*quote))
}

/// A string's decoded contents from `opened`, just after its opening quote, and the offset just after its closing one.
/// An escape becomes a space, which ends a word, except an escaped quote or backslash, which is itself. An unclosed
/// string runs to the end of the text.
fn string_body(text: &str, opened: usize, quote: &str, raw: bool) -> (String, usize) {
    let mut contents = String::new();
    let mut at = opened;
    loop {
        let rest = &text[at..];
        if rest.starts_with(quote) {
            return (contents, at + quote.len());
        }
        let mut characters = rest.chars();
        let Some(character) = characters.next() else {
            break;
        };
        if character == '\\' && !raw {
            match characters.next() {
                Some(escaped @ ('"' | '\'' | '\\')) => {
                    contents.push(escaped);
                    at += 1 + escaped.len_utf8();
                }
                Some(other) => {
                    contents.push(' ');
                    at += 1 + other.len_utf8();
                }
                None => at += 1,
            }
        } else {
            contents.push(character);
            at += character.len_utf8();
        }
    }
    (contents, text.len())
}

/// Each identifier-shaped word of a string's contents, at the string's offset.
fn words(contents: &str, offset: usize, found: &mut Vec<Occurrence>) {
    let mut start = None;
    for (index, character) in contents.char_indices().chain([(contents.len(), ' ')]) {
        match start {
            None if identifier_start(character) => start = Some(index),
            Some(begun) if !identifier_part(character) => {
                found.push(Occurrence {
                    name: contents[begun..index].to_owned(),
                    how: Use::Quoted,
                    offset,
                });
                start = None;
            }
            _ => {}
        }
    }
}

/// A number's leading run of digits, letters and underscores (`0x1F`, `1_000`, `1e5`), so none of its letters reads
/// as a name. A fraction or an exponent's sign ends the run, and what follows is digits and letters again.
fn number_length(rest: &str) -> usize {
    rest.char_indices()
        .find(|&(_, character)| !identifier_part(character))
        .map_or(rest.len(), |(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uses(text: &str, dialect: Dialect) -> Vec<(String, Use)> {
        occurrences(text, dialect)
            .into_iter()
            .map(|occurrence| (occurrence.name, occurrence.how))
            .collect()
    }

    fn named(names: &[(&str, Use)]) -> Vec<(String, Use)> {
        names
            .iter()
            .map(|(name, how)| ((*name).to_owned(), *how))
            .collect()
    }

    #[test]
    fn a_plain_assignment_target_is_assigned_and_every_other_identifier_is_named() {
        assert_eq!(
            uses(
                "self.a = b\nc += 1\nd == e\nf := 2\ng[0] = 1\n",
                Dialect::Script
            ),
            named(&[
                ("self", Use::Named),
                ("a", Use::Assigned),
                ("b", Use::Named),
                ("c", Use::Named),
                ("d", Use::Named),
                ("e", Use::Named),
                ("f", Use::Named),
                ("g", Use::Named),
            ])
        );
    }

    #[test]
    fn comments_are_skipped_and_strings_give_their_identifier_shaped_words() {
        assert_eq!(
            uses(
                "call(\"set_size\", \"2d view\") # not_this [member enabled]\n## nor_this\n&'moved' ^\"a:b\"",
                Dialect::Script
            ),
            named(&[
                ("call", Use::Named),
                ("set_size", Use::Quoted),
                ("d", Use::Quoted),
                ("view", Use::Quoted),
                ("moved", Use::Quoted),
                ("a", Use::Quoted),
                ("b", Use::Quoted),
            ])
        );
    }

    #[test]
    fn a_string_holds_its_comment_character_and_its_escapes_end_words() {
        assert_eq!(
            uses(
                "x = \"a#b\\nc\\\"d\"\ny = r\"e\\f\" + \"\"\"g\nh\"\"\"",
                Dialect::Script
            ),
            named(&[
                ("x", Use::Assigned),
                ("a", Use::Quoted),
                ("b", Use::Quoted),
                ("c", Use::Quoted),
                ("d", Use::Quoted),
                ("y", Use::Assigned),
                ("e", Use::Quoted),
                ("f", Use::Quoted),
                ("g", Use::Quoted),
                ("h", Use::Quoted),
            ])
        );
    }

    #[test]
    fn an_unclosed_string_runs_to_the_end_even_after_a_last_backslash() {
        assert_eq!(
            uses("x = \"abc\\", Dialect::Script),
            named(&[("x", Use::Assigned), ("abc", Use::Quoted)])
        );
    }

    #[test]
    fn a_line_continuation_is_whitespace_so_an_assignment_stays_one() {
        for text in ["flag \\\n\t= true", "flag \\\r\n\t= true"] {
            assert_eq!(
                uses(text, Dialect::Script),
                named(&[("flag", Use::Assigned), ("true", Use::Named)]),
                "{text:?}"
            );
        }
    }

    #[test]
    fn numbers_are_not_names() {
        assert_eq!(
            uses("a = 0x1F + 1_000 + 1.5e-05 + b.c", Dialect::Script),
            named(&[("a", Use::Assigned), ("b", Use::Named), ("c", Use::Named)])
        );
    }

    #[test]
    fn a_resource_comments_with_a_semicolon_and_its_property_lines_are_assignments() {
        assert_eq!(
            uses(
                "; a comment\n[node name=\"Root\"]\nlocomotion = true\n[connection method=\"_on_moved\"]\n",
                Dialect::Resource
            ),
            named(&[
                ("node", Use::Named),
                ("name", Use::Assigned),
                ("Root", Use::Quoted),
                ("locomotion", Use::Assigned),
                ("true", Use::Named),
                ("connection", Use::Named),
                ("method", Use::Assigned),
                ("_on_moved", Use::Quoted),
            ])
        );
    }
}
