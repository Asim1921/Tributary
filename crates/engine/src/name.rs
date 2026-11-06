//! Works out a safe file name. Names from servers are untrusted input.

use percent_encoding::percent_decode_str;
use reqwest::Url;

const FALLBACK_NAME: &str = "download";
const MAX_NAME_CHARS: usize = 150;

/// Names Windows reserves for devices, with any extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Returns the name in a `Content-Disposition` value, preferring `filename*`.
pub fn from_content_disposition(value: &str) -> Option<String> {
    let parameters = parameters(value);
    let find = |wanted: &str| {
        parameters
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.as_str())
    };

    find("filename*")
        .and_then(decode_extended)
        .or_else(|| find("filename").map(str::to_owned))
        .filter(|name| !name.is_empty())
}

/// Returns the last path segment of a URL, decoded.
pub fn from_url(url: &Url) -> Option<String> {
    let segment = url.path_segments()?.next_back()?;
    let decoded = percent_decode_str(segment).decode_utf8().ok()?;
    (!decoded.is_empty()).then(|| decoded.into_owned())
}

/// Turns any text into a plain file name that can't leave its folder.
pub fn sanitize(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let replaced: String = base
        .chars()
        .map(|c| if is_forbidden(c) { '_' } else { c })
        .collect();

    let trimmed = replaced
        .trim_start_matches(' ')
        .trim_end_matches([' ', '.']);
    if trimmed.is_empty() {
        return FALLBACK_NAME.to_owned();
    }

    let shortened = shorten(trimmed);
    let stem = shortened.split('.').next().unwrap_or_default();
    if RESERVED.iter().any(|name| stem.eq_ignore_ascii_case(name)) {
        return format!("_{shortened}");
    }
    shortened
}

/// Blocks characters Windows rejects, and invisible ones that disguise a file's type.
fn is_forbidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        || matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Limits the length while keeping the extension.
fn shorten(name: &str) -> String {
    if name.chars().count() <= MAX_NAME_CHARS {
        return name.to_owned();
    }

    let (stem, extension) = match name.rfind('.') {
        Some(dot) if name.len() - dot <= 16 => name.split_at(dot),
        _ => (name, ""),
    };
    let keep = MAX_NAME_CHARS - extension.chars().count();
    let short_stem: String = stem.chars().take(keep).collect();
    format!("{}{extension}", short_stem.trim_end_matches([' ', '.']))
}

/// Splits `a=b; c="d;e"` into parameters, respecting quotes.
fn parameters(value: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for c in value.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if in_quotes && c == '\\' {
            escaped = true;
        } else if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ';' && !in_quotes {
            found.extend(split_parameter(&current));
            current.clear();
        } else {
            current.push(c);
        }
    }
    found.extend(split_parameter(&current));
    found
}

fn split_parameter(text: &str) -> Option<(String, String)> {
    let (name, value) = text.split_once('=')?;
    Some((name.trim().to_owned(), value.trim().to_owned()))
}

/// Decodes the RFC 8187 form `UTF-8''my%20file.zip`.
fn decode_extended(value: &str) -> Option<String> {
    let mut pieces = value.splitn(3, '\'');
    let charset = pieces.next()?;
    let _language = pieces.next()?;
    let encoded = pieces.next()?;

    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }
    let decoded = percent_decode_str(encoded).decode_utf8().ok()?;
    Some(decoded.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_quoted_names() {
        let read = from_content_disposition;
        assert_eq!(read("attachment; filename=a.zip").as_deref(), Some("a.zip"));
        assert_eq!(
            read("attachment; filename=\"a b.zip\"").as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            read("attachment; filename=\"a;b.zip\"; size=3").as_deref(),
            Some("a;b.zip")
        );
        assert_eq!(
            read(r#"attachment; filename="a\"b.zip""#).as_deref(),
            Some("a\"b.zip")
        );
        assert_eq!(read("attachment"), None);
        assert_eq!(read("attachment; filename=\"\""), None);
    }

    #[test]
    fn prefers_the_extended_form() {
        let value = "attachment; filename=\"plain.zip\"; filename*=UTF-8''caf%C3%A9.zip";
        assert_eq!(from_content_disposition(value).as_deref(), Some("café.zip"));
    }

    #[test]
    fn falls_back_when_the_extended_form_is_unusable() {
        let value = "attachment; filename=\"plain.zip\"; filename*=ISO-8859-1''caf%E9.zip";
        assert_eq!(
            from_content_disposition(value).as_deref(),
            Some("plain.zip")
        );
    }

    #[test]
    fn reads_names_from_urls() {
        let read = |url: &str| from_url(&Url::parse(url).unwrap());
        assert_eq!(
            read("https://x.test/a/my%20file.zip?token=1").as_deref(),
            Some("my file.zip")
        );
        assert_eq!(read("https://x.test/"), None);
        assert_eq!(read("https://x.test/folder/"), None);
    }

    #[test]
    fn path_traversal_stays_inside_the_folder() {
        assert_eq!(sanitize("../../evil.exe"), "evil.exe");
        assert_eq!(sanitize(r"..\..\evil.exe"), "evil.exe");
        assert_eq!(sanitize(r"C:\Windows\evil.exe"), "evil.exe");
        assert_eq!(sanitize(".."), FALLBACK_NAME);
        assert_eq!(sanitize("/"), FALLBACK_NAME);
    }

    #[test]
    fn replaces_characters_windows_rejects() {
        assert_eq!(sanitize("a<b>c:d|e?f*g\"h.txt"), "a_b_c_d_e_f_g_h.txt");
        assert_eq!(sanitize("report.txt:hidden"), "report.txt_hidden");
        assert_eq!(sanitize("a\r\nb.txt"), "a__b.txt");
    }

    #[test]
    fn removes_characters_that_disguise_a_file_type() {
        assert_eq!(sanitize("photo\u{202E}gpj.exe"), "photo_gpj.exe");
    }

    #[test]
    fn trims_ends_windows_would_silently_drop() {
        assert_eq!(sanitize("  file.txt. . "), "file.txt");
        assert_eq!(sanitize("   "), FALLBACK_NAME);
        assert_eq!(sanitize(""), FALLBACK_NAME);
    }

    #[test]
    fn avoids_reserved_device_names() {
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("nul.txt"), "_nul.txt");
        assert_eq!(sanitize("com1.tar.gz"), "_com1.tar.gz");
        assert_eq!(sanitize("console.txt"), "console.txt");
    }

    #[test]
    fn shortens_long_names_but_keeps_the_extension() {
        let long = format!("{}.zip", "a".repeat(500));
        let short = sanitize(&long);
        assert_eq!(short.chars().count(), MAX_NAME_CHARS);
        assert!(short.ends_with(".zip"));
    }

    #[test]
    fn keeps_ordinary_unicode_names() {
        assert_eq!(sanitize("документ 日本語.pdf"), "документ 日本語.pdf");
    }
}
