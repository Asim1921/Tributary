//! Text for the user from the Rust side, in their language.
//!
//! English text is the key, as in the window (`ui/src/lib/i18n.ts`), and both read the same
//! catalogues in `ui/src/locales`. Text sent to the window stays English, written with
//! [`text!`](crate::text): the window finds its pattern in the catalogue and says it in the
//! language it shows, so logs, the command line and stored errors keep the English. What Rust
//! shows itself, the tray menu and notifications, it translates here with [`say!`](crate::say).

use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};

/// The languages there are catalogues for, beside English.
pub const CODES: [&str; 4] = ["ur", "ar", "hi", "es"];

const CATALOGUES: [(&str, &str); 4] = [
    ("ur", include_str!("../../ui/src/locales/ur.json")),
    ("ar", include_str!("../../ui/src/locales/ar.json")),
    ("hi", include_str!("../../ui/src/locales/hi.json")),
    ("es", include_str!("../../ui/src/locales/es.json")),
];

static TRANSLATIONS: LazyLock<HashMap<&'static str, HashMap<String, String>>> =
    LazyLock::new(|| {
        CATALOGUES
            .iter()
            .map(|(code, text)| (*code, serde_json::from_str(text).unwrap_or_default()))
            .collect()
    });

/// The language the app speaks, as the window last chose it.
static LANGUAGE: RwLock<String> = RwLock::new(String::new());

pub fn set_language(code: &str) {
    if let Ok(mut language) = LANGUAGE.write() {
        *language = code.to_owned();
    }
}

pub fn language() -> String {
    LANGUAGE
        .read()
        .map(|language| language.clone())
        .unwrap_or_default()
}

/// A value in a message.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Number(f64),
    Message(Message),
}

impl From<String> for Part {
    fn from(text: String) -> Self {
        Part::Text(text)
    }
}

impl From<&str> for Part {
    fn from(text: &str) -> Self {
        Part::Text(text.to_owned())
    }
}

impl From<Message> for Part {
    fn from(message: Message) -> Self {
        Part::Message(message)
    }
}

macro_rules! number_parts {
    ($($kind:ty),*) => {$(
        impl From<$kind> for Part {
            fn from(number: $kind) -> Self {
                Part::Number(number as f64)
            }
        }
    )*};
}
number_parts!(u8, u16, u32, u64, usize, i32, i64, f64);

/// Something to tell the user: English text with `{name}` slots, and their values.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub text: &'static str,
    pub values: Vec<(&'static str, Part)>,
}

/// English for the window to translate: `format!`, marked so the check that every text has
/// its translations finds it. Its `{names}` must match the catalogue's.
#[macro_export]
macro_rules! text {
    ($($arguments:tt)*) => {
        format!($($arguments)*)
    };
}

/// English for the window to translate, as a plain `&'static str`, marked like [`text!`].
#[macro_export]
macro_rules! en {
    ($text:literal) => {
        $text
    };
}

/// Makes a [`Message`] for Rust to show itself: `say!("Saved {name}", name = file)`.
#[macro_export]
macro_rules! say {
    ($text:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::messages::Message {
            text: $text,
            values: vec![$((stringify!($name), $crate::messages::Part::from($value))),*],
        }
    };
}

impl Message {
    /// In the language the app speaks now.
    pub fn local(&self) -> String {
        self.in_language(&language())
    }

    pub fn in_language(&self, code: &str) -> String {
        let translated = TRANSLATIONS
            .get(code)
            .and_then(|catalogue| catalogue.get(self.text))
            .filter(|text| !text.is_empty());
        let (template, code) = match translated {
            Some(text) => (text.as_str(), code),
            None => (self.text, "en"),
        };
        let values: HashMap<&str, String> = self
            .values
            .iter()
            .map(|(name, part)| {
                let shown = match part {
                    Part::Text(text) => text.clone(),
                    Part::Number(number) => show_number(*number),
                    Part::Message(message) => message.in_language(code),
                };
                (*name, shown)
            })
            .collect();
        let numbers: HashMap<&str, f64> = self
            .values
            .iter()
            .filter_map(|(name, part)| match part {
                Part::Number(number) => Some((*name, *number)),
                _ => None,
            })
            .collect();
        fill(template, code, &values, &numbers)
    }
}

/// English, for logs.
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.in_language("en"))
    }
}

fn show_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number:.2}")
    }
}

/// Which plural form a number takes in a language, as CLDR names them.
pub fn plural_category(code: &str, number: f64) -> &'static str {
    let whole = number.fract() == 0.0;
    let n = number.abs();
    match code {
        "ar" => {
            let hundreds = n % 100.0;
            if n == 0.0 {
                "zero"
            } else if n == 1.0 {
                "one"
            } else if n == 2.0 {
                "two"
            } else if whole && (3.0..=10.0).contains(&hundreds) {
                "few"
            } else if whole && (11.0..=99.0).contains(&hundreds) {
                "many"
            } else {
                "other"
            }
        }
        // Hindi treats 0 and 1 alike.
        "hi" => {
            if n.floor() == 0.0 || n == 1.0 {
                "one"
            } else {
                "other"
            }
        }
        _ => {
            if n == 1.0 && whole {
                "one"
            } else {
                "other"
            }
        }
    }
}

fn closing(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (index, character) in text[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

fn choices(text: &str) -> HashMap<String, String> {
    let mut found = HashMap::new();
    let mut index = 0;
    while let Some(relative) = text[index..].find('{') {
        let open = index + relative;
        let selector = text[index..open].trim().to_owned();
        let Some(end) = closing(text, open) else {
            break;
        };
        found.insert(selector, text[open + 1..end].to_owned());
        index = end + 1;
    }
    found
}

/// Text filled into a right-to-left sentence keeps its own direction, between Unicode's
/// first-strong isolate and pop directional isolate, as `isolate` in the window does.
fn isolate(text: &str, code: &str) -> String {
    if matches!(code, "ur" | "ar") {
        format!("\u{2068}{text}\u{2069}")
    } else {
        text.to_owned()
    }
}

/// Fills `{name}` and `{name, plural, one {…} other {…}}`, as `fill` in the window does.
pub fn fill(
    template: &str,
    code: &str,
    values: &HashMap<&str, String>,
    numbers: &HashMap<&str, f64>,
) -> String {
    let mut out = String::new();
    let mut index = 0;
    while let Some(relative) = template[index..].find('{') {
        let open = index + relative;
        out.push_str(&template[index..open]);
        let Some(end) = closing(template, open) else {
            out.push_str(&template[open..]);
            return out;
        };
        let inner = &template[open + 1..end];
        let mut parts = inner.splitn(3, ',');
        let name = parts.next().unwrap_or_default().trim();
        match (parts.next().map(str::trim), parts.next()) {
            (Some("plural"), Some(rest)) => {
                let count = numbers.get(name).copied().unwrap_or_default();
                let options = choices(rest);
                let exact = format!("={}", show_number(count));
                let chosen = options
                    .get(&exact)
                    .or_else(|| options.get(plural_category(code, count)))
                    .or_else(|| options.get("other"))
                    .cloned()
                    .unwrap_or_default();
                out.push_str(&fill(
                    &chosen.replace('#', &show_number(count)),
                    code,
                    values,
                    numbers,
                ));
            }
            _ => match values.get(name) {
                Some(value) => out.push_str(&isolate(value, code)),
                None => out.push_str(&template[open..=end]),
            },
        }
        index = end + 1;
    }
    out.push_str(&template[index..]);
    out
}

/// A piece of a catalogue's text: words, or a `{name}` to fill in.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Words(String),
    Name(String),
}

/// A catalogue's text with `{names}` in it, as a pattern to find in English from the app.
#[derive(Debug)]
struct Pattern {
    text: String,
    pieces: Vec<Piece>,
    letters: usize,
}

impl Pattern {
    fn new(text: &str) -> Option<Self> {
        // Plurals are chosen where the text is made, never sent as English.
        if !text.contains('{') || text.contains(", plural,") {
            return None;
        }
        let mut pieces = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let close = open + rest[open..].find('}')?;
            if open > 0 {
                pieces.push(Piece::Words(rest[..open].to_owned()));
            }
            pieces.push(Piece::Name(rest[open + 1..close].trim().to_owned()));
            rest = &rest[close + 1..];
        }
        if !rest.is_empty() {
            pieces.push(Piece::Words(rest.to_owned()));
        }
        // Two names side by side can't be told apart, and a pattern without words would
        // match anything.
        let adjacent = pieces
            .windows(2)
            .any(|pair| matches!(pair, [Piece::Name(_), Piece::Name(_)]));
        let letters = pieces
            .iter()
            .map(|piece| match piece {
                Piece::Words(words) => words.chars().filter(|c| c.is_alphabetic()).count(),
                Piece::Name(_) => 0,
            })
            .sum();
        (!adjacent && letters > 0).then(|| Self {
            text: text.to_owned(),
            pieces,
            letters,
        })
    }

    /// The text filling each name, if `text` follows this pattern.
    fn find(&self, text: &str) -> Option<Vec<(String, String)>> {
        fn walk(text: &str, pieces: &[Piece]) -> Option<Vec<(String, String)>> {
            match pieces {
                [] => text.is_empty().then(Vec::new),
                [Piece::Words(words), rest @ ..] => walk(text.strip_prefix(words.as_str())?, rest),
                [Piece::Name(name)] => {
                    (!text.is_empty()).then(|| vec![(name.clone(), text.to_owned())])
                }
                [Piece::Name(name), rest @ ..] => {
                    let Some(Piece::Words(next)) = rest.first() else {
                        return None;
                    };
                    // The shortest filling that lets the rest follow.
                    let mut from = 1;
                    while let Some(found) =
                        text.get(from..).and_then(|tail| tail.find(next.as_str()))
                    {
                        let at = from + found;
                        if let Some(mut found) = walk(&text[at..], rest) {
                            found.insert(0, (name.clone(), text[..at].to_owned()));
                            return Some(found);
                        }
                        from = at + 1;
                        while !text.is_char_boundary(from) {
                            from += 1;
                        }
                    }
                    None
                }
            }
        }
        walk(text, &self.pieces)
    }
}

static PATTERNS: LazyLock<HashMap<&'static str, Vec<Pattern>>> = LazyLock::new(|| {
    TRANSLATIONS
        .iter()
        .map(|(code, catalogue)| {
            let mut patterns: Vec<Pattern> = catalogue
                .keys()
                .filter_map(|text| Pattern::new(text))
                .collect();
            // The most specific first.
            patterns.sort_by_key(|pattern| std::cmp::Reverse(pattern.letters));
            (*code, patterns)
        })
        .collect()
});

/// English from anywhere in the app, such as a download's error, in the language it speaks
/// now: the catalogue's own text, or the pattern it follows with its parts said too. Text
/// the catalogue doesn't know, such as a server's own words, stays as it is.
pub fn describe(text: &str) -> String {
    describe_in(text, &language())
}

pub fn describe_in(text: &str, code: &str) -> String {
    let (Some(catalogue), Some(patterns)) = (TRANSLATIONS.get(code), PATTERNS.get(code)) else {
        return text.to_owned();
    };
    let say = |line: &str| describe_line(line, code, catalogue, patterns, 0);
    text.split('\n').map(say).collect::<Vec<_>>().join("\n")
}

fn describe_line(
    text: &str,
    code: &str,
    catalogue: &HashMap<String, String>,
    patterns: &[Pattern],
    depth: usize,
) -> String {
    let none = HashMap::new();
    if let Some(own) = catalogue.get(text).filter(|own| !own.is_empty()) {
        return fill(own, code, &none, &HashMap::new());
    }
    if depth > 4 || text.trim().is_empty() {
        return text.to_owned();
    }
    for pattern in patterns {
        let Some(found) = pattern.find(text) else {
            continue;
        };
        let Some(translated) = catalogue.get(&pattern.text).filter(|own| !own.is_empty()) else {
            continue;
        };
        let values: HashMap<&str, String> = found
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str(),
                    describe_line(value, code, catalogue, patterns, depth + 1),
                )
            })
            .collect();
        return fill(translated, code, &values, &HashMap::new());
    }
    text.to_owned()
}

/// The language to speak: the one chosen in Options, or else the first of Windows' own
/// display languages there's a catalogue for, or else English.
pub fn pick_language(chosen: &str, system: &[String]) -> String {
    let known = |code: &str| code == "en" || CATALOGUES.iter().any(|(own, _)| *own == code);
    if known(chosen) {
        return chosen.to_owned();
    }
    system
        .iter()
        .map(|tag| {
            tag.split(['-', '_'])
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .find(|base| known(base))
        .unwrap_or_else(|| "en".to_owned())
}

/// Windows' display languages, most preferred first, as tags such as `ur-PK`.
pub fn windows_languages() -> Vec<String> {
    use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let mut count = 0u32;
    let mut size = 0u32;
    // SAFETY: a null buffer asks only for the size needed.
    let sized = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if sized == 0 || size == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: the buffer holds the `size` characters asked for.
    let read = unsafe {
        GetUserPreferredUILanguages(
            MUI_LANGUAGE_NAME,
            &mut count,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if read == 0 {
        return Vec::new();
    }
    buffer
        .split(|unit| *unit == 0)
        .filter(|tag| !tag.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_fills_its_values_in_english_for_logs() {
        let message = say!("Saved {name} in {folder}", name = "a.zip", folder = r"C:\x");
        assert_eq!(message.to_string(), r"Saved a.zip in C:\x");
        let files = say!(
            "{count, plural, one {# file finished} other {# files finished}}",
            count = 3usize
        );
        assert_eq!(files.to_string(), "3 files finished");
        assert_eq!(
            say!(
                "{count, plural, one {# file finished} other {# files finished}}",
                count = 1u32
            )
            .to_string(),
            "1 file finished"
        );
    }

    #[test]
    fn a_message_inside_another_is_said_in_the_same_language() {
        let inner = say!("the server answered {status}", status = 404u16);
        let message = say!("the torrent couldn't be added: {error}", error = inner);
        assert_eq!(
            message.to_string(),
            "the torrent couldn't be added: the server answered 404"
        );
        // Text for the window is the English, filled in as `format!` would.
        let name = "a.zip";
        assert_eq!(text!("couldn't save {name}"), "couldn't save a.zip");
    }

    #[test]
    fn english_from_the_app_is_matched_to_its_pattern_and_translated() {
        let pattern = Pattern::new("the sound couldn't be saved: {problem}").unwrap();
        assert_eq!(
            pattern.find("the sound couldn't be saved: the server answered 403 Forbidden"),
            Some(vec![(
                "problem".to_owned(),
                "the server answered 403 Forbidden".to_owned()
            )])
        );
        assert_eq!(pattern.find("the sound was saved"), None);

        let pair = Pattern::new("{name} and {count} more").unwrap();
        assert_eq!(
            pair.find("a and b.zip and 3 more"),
            Some(vec![
                ("name".to_owned(), "a".to_owned()),
                ("count".to_owned(), "b.zip and 3".to_owned())
            ])
        );
        // No words to anchor on, or names side by side: not patterns at all.
        assert!(Pattern::new("{name}: {reason}").is_none());
        assert!(Pattern::new("{a}{b} files").is_none());
        assert!(Pattern::new("{count, plural, one {# file} other {# files}}").is_none());
        // A language without a catalogue, or unknown text, comes back as it was.
        assert_eq!(describe_in("the file is gone", "xx"), "the file is gone");
    }

    #[test]
    fn the_real_catalogues_say_the_app_in_every_language() {
        // Shown by Rust itself.
        assert_eq!(say!("Quit").in_language("es"), "Salir");
        assert_eq!(say!("Open Tributary").in_language("ur"), "Tributary کھولیں");
        // A file name in an Urdu sentence keeps its own direction.
        assert_eq!(
            say!("Saved {name}", name = "a.zip").in_language("ur"),
            "\u{2068}a.zip\u{2069} محفوظ ہو گیا"
        );
        // English from the engine, with English from the server inside it.
        assert_eq!(
            describe_in(
                "the sound couldn't be saved: the server answered 403 Forbidden",
                "es"
            ),
            "no se pudo guardar el sonido: el servidor respondió 403 Forbidden"
        );
        for code in CODES {
            let said = describe_in("the file is no longer there", code);
            assert_ne!(said, "the file is no longer there", "{code}");
            // A queue's name in a notification is said too.
            assert!(
                !describe_in("Main queue is done", code).contains("Main queue"),
                "{code}"
            );
        }
    }

    #[test]
    fn plurals_follow_each_language() {
        assert_eq!(plural_category("en", 1.0), "one");
        assert_eq!(plural_category("en", 0.0), "other");
        assert_eq!(plural_category("hi", 0.0), "one");
        assert_eq!(plural_category("ar", 0.0), "zero");
        assert_eq!(plural_category("ar", 2.0), "two");
        assert_eq!(plural_category("ar", 7.0), "few");
        assert_eq!(plural_category("ar", 103.0), "few");
        assert_eq!(plural_category("ar", 15.0), "many");
        assert_eq!(plural_category("ar", 100.0), "other");
        assert_eq!(plural_category("ur", 1.0), "one");
        assert_eq!(plural_category("es", 2.0), "other");
    }

    #[test]
    fn an_exact_number_comes_before_a_category() {
        let values = HashMap::from([("queue", "Main".to_owned())]);
        let numbers = HashMap::from([("count", 0.0)]);
        assert_eq!(
            fill(
                "{count, plural, =0 {Nothing in {queue}} other {# in {queue}}}",
                "en",
                &values,
                &numbers
            ),
            "Nothing in Main"
        );
    }

    #[test]
    fn the_language_follows_the_choice_then_windows() {
        let windows = vec!["fr-FR".to_owned(), "ur-PK".to_owned()];
        assert_eq!(pick_language("es", &windows), "es");
        assert_eq!(pick_language("", &windows), "ur");
        assert_eq!(pick_language("xx", &["de-DE".to_owned()]), "en");
    }
}
