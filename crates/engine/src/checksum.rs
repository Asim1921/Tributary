//! Checking a finished file against the checksum its publisher gave for it.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use md5::Md5;
use reqwest::Url;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

/// How much of a file is read at a time while it's checked.
const READ_SIZE: usize = 1 << 20;

/// The kinds of checksum publishers give, told apart by their length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    const ALL: [Self; 4] = [Self::Md5, Self::Sha1, Self::Sha256, Self::Sha512];

    fn for_length(hex_digits: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|algorithm| algorithm.hex_length() == hex_digits)
    }

    fn hex_length(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha1 => 40,
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }

    /// How it's written when saved, such as `sha256`.
    fn key(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }

    /// How people know it, such as `SHA-256`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }

    /// The name at the start of `text`, if it names one, and the rest of `text`.
    fn named_at_start(text: &str) -> Option<(Self, &str)> {
        let lower = text.to_ascii_lowercase();
        Self::ALL.into_iter().find_map(|algorithm| {
            // Longest spellings first, so `sha-256` isn't read as `sha` and a remainder.
            let label = algorithm.label().to_ascii_lowercase();
            [label.as_str(), algorithm.key()]
                .into_iter()
                .find(|name| lower.starts_with(name))
                .map(|name| (algorithm, &text[name.len()..]))
        })
    }
}

/// A file's expected checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checksum {
    pub algorithm: Algorithm,
    /// Lower-case hex.
    pub hex: String,
}

impl Checksum {
    /// Reads a checksum as people paste it: the hex alone, or with its name in front, as
    /// in `sha256:…` or `SHA-256 …`. Case and spaces don't matter. A name that doesn't fit
    /// the length makes it unreadable, since one of the two must be a mistake.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (named, rest) = match Algorithm::named_at_start(text) {
            Some((algorithm, rest)) => (Some(algorithm), rest),
            None => (None, text),
        };
        let rest = rest.trim_start_matches([':', '=', ' ', '\t']);
        let hex: String = rest
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return None;
        }
        let algorithm = Algorithm::for_length(hex.len())?;
        if named.is_some_and(|named| named != algorithm) {
            return None;
        }
        Some(Self { algorithm, hex })
    }

    /// Whether `actual`, in hex, is this checksum.
    pub fn matches(&self, actual: &str) -> bool {
        self.hex.eq_ignore_ascii_case(actual)
    }
}

/// Saved as `sha256:…`, which [`Checksum::parse`] reads back.
impl fmt::Display for Checksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm.key(), self.hex)
    }
}

/// A file's checksum, in lower-case hex.
pub fn hash_file(path: &Path, algorithm: Algorithm) -> io::Result<String> {
    match algorithm {
        Algorithm::Md5 => hash_with::<Md5>(path),
        Algorithm::Sha1 => hash_with::<Sha1>(path),
        Algorithm::Sha256 => hash_with::<Sha256>(path),
        Algorithm::Sha512 => hash_with::<Sha512>(path),
    }
}

fn hash_with<D: Digest>(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = D::new();
    let mut buffer = vec![0; READ_SIZE];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Finds the checksum given for a file in a published list, such as `SHA256SUMS` or a
/// `.sha256` file. Lines are read as `hex  name`, `hex *name` or `SHA256 (name) = hex`.
/// A list holding one checksum with no name at all counts for whatever it sits beside.
pub fn find_in_list(text: &str, names: &[&str]) -> Option<Checksum> {
    let entries: Vec<(Option<String>, Checksum)> = text.lines().filter_map(list_entry).collect();
    if let [(None, checksum)] = entries.as_slice() {
        return Some(checksum.clone());
    }
    entries.into_iter().find_map(|(name, checksum)| {
        let name = name?;
        // A name may come with a folder in front, as in `./file.iso` or `dir/file.iso`.
        let base = name.rsplit(['/', '\\']).next().unwrap_or(&name);
        names
            .iter()
            .any(|wanted| !wanted.is_empty() && base.eq_ignore_ascii_case(wanted))
            .then_some(checksum)
    })
}

/// One line of a checksum list, and the file it names if it names one.
fn list_entry(line: &str) -> Option<(Option<String>, Checksum)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    // BSD style: `SHA256 (file.iso) = abc…`
    if let Some((algorithm, rest)) = Algorithm::named_at_start(line)
        && let Some(rest) = rest.trim_start().strip_prefix('(')
        && let Some((name, hex)) = rest.rsplit_once(") =")
    {
        let checksum = Checksum::parse(hex)?;
        return (checksum.algorithm == algorithm).then(|| (Some(name.to_owned()), checksum));
    }
    // GNU style: `abc…  file.iso`, with `*` before the name for binary mode.
    let (hex, name) = match line.split_once(char::is_whitespace) {
        Some((hex, name)) => (hex, Some(name.trim().trim_start_matches('*').to_owned())),
        None => (line, None),
    };
    let checksum = Checksum::parse(hex)?;
    Some((name.filter(|name| !name.is_empty()), checksum))
}

/// Where a file's checksum is commonly published, beside the address it comes from.
pub fn list_addresses(url: &str) -> Vec<String> {
    let Ok(mut url) = Url::parse(url) else {
        return Vec::new();
    };
    if !matches!(url.scheme(), "http" | "https") || url.path().ends_with('/') {
        return Vec::new();
    }
    url.set_query(None);
    url.set_fragment(None);
    let _ = url.set_username("");
    let _ = url.set_password(None);

    let path = url.path().to_owned();
    let mut found: Vec<String> = [".sha256", ".sha256sum", ".sha512", ".sha1", ".md5"]
        .iter()
        .map(|extension| {
            let mut beside = url.clone();
            beside.set_path(&format!("{path}{extension}"));
            beside.into()
        })
        .collect();
    for list in ["SHA256SUMS", "SHA512SUMS"] {
        if let Ok(address) = url.join(list) {
            found.push(address.into());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const SHA256_OF_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn a_pasted_checksum_is_read_however_it_is_written() {
        let expected = Checksum {
            algorithm: Algorithm::Sha256,
            hex: SHA256_OF_ABC.to_owned(),
        };
        let upper = SHA256_OF_ABC.to_ascii_uppercase();
        let spaced = SHA256_OF_ABC
            .as_bytes()
            .chunks(8)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        for text in [
            SHA256_OF_ABC.to_owned(),
            format!("  {upper}\n"),
            format!("sha256:{SHA256_OF_ABC}"),
            format!("SHA-256: {SHA256_OF_ABC}"),
            format!("SHA256 {spaced}"),
        ] {
            assert_eq!(Checksum::parse(&text), Some(expected.clone()), "{text}");
        }
    }

    #[test]
    fn the_kind_comes_from_the_length() {
        let kind = |text: &str| Checksum::parse(text).map(|checksum| checksum.algorithm);
        assert_eq!(kind(&"a".repeat(32)), Some(Algorithm::Md5));
        assert_eq!(kind(&"a".repeat(40)), Some(Algorithm::Sha1));
        assert_eq!(kind(&"a".repeat(64)), Some(Algorithm::Sha256));
        assert_eq!(kind(&"a".repeat(128)), Some(Algorithm::Sha512));
    }

    #[test]
    fn something_that_is_not_a_checksum_is_refused() {
        for text in [
            "",
            "abc",
            &"a".repeat(63),
            &"g".repeat(64),
            // A name that doesn't fit the length means one of them is wrong.
            &format!("md5:{SHA256_OF_ABC}"),
            "https://example.com/file.iso",
        ] {
            assert_eq!(Checksum::parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_checksum_is_saved_in_a_form_it_reads_back() {
        let checksum = Checksum::parse(&format!("SHA-256 {SHA256_OF_ABC}")).unwrap();
        assert_eq!(checksum.to_string(), format!("sha256:{SHA256_OF_ABC}"));
        assert_eq!(Checksum::parse(&checksum.to_string()), Some(checksum));
    }

    #[test]
    fn files_are_hashed_with_each_kind() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("abc.txt");
        fs::write(&path, b"abc").unwrap();

        assert_eq!(
            hash_file(&path, Algorithm::Md5).unwrap(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            hash_file(&path, Algorithm::Sha1).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(hash_file(&path, Algorithm::Sha256).unwrap(), SHA256_OF_ABC);
        assert!(
            hash_file(&path, Algorithm::Sha512)
                .unwrap()
                .starts_with("ddaf35a193617aba")
        );
    }

    #[test]
    fn a_file_larger_than_one_read_is_hashed_whole() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("big.bin");
        let data: Vec<u8> = (0..READ_SIZE * 3 + 7).map(|index| index as u8).collect();
        fs::write(&path, &data).unwrap();

        let expected: String = Sha256::digest(&data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hash_file(&path, Algorithm::Sha256).unwrap(), expected);
    }

    #[test]
    fn a_list_gives_the_checksum_of_the_file_it_names() {
        let other = "b".repeat(64);
        let list = format!(
            "# checksums\n{other}  other.iso\n{SHA256_OF_ABC} *ubuntu-24.04-desktop-amd64.iso\n"
        );

        let found = find_in_list(&list, &["ubuntu-24.04-desktop-amd64.iso"]).unwrap();
        assert_eq!(found.hex, SHA256_OF_ABC);
        assert_eq!(find_in_list(&list, &["missing.iso"]), None);
    }

    #[test]
    fn bsd_style_lists_and_folders_in_names_are_understood() {
        let list = format!(
            "SHA256 (./dist/app-1.2.zip) = {SHA256_OF_ABC}\nMD5 (app-1.2.zip) = {}\n",
            "c".repeat(32)
        );

        let found = find_in_list(&list, &["APP-1.2.ZIP"]).unwrap();
        assert_eq!(found.algorithm, Algorithm::Sha256);
        assert_eq!(found.hex, SHA256_OF_ABC);
    }

    #[test]
    fn a_lone_unnamed_checksum_counts_for_the_file_beside_it() {
        assert_eq!(
            find_in_list(&format!("{SHA256_OF_ABC}\n"), &["anything.bin"])
                .unwrap()
                .hex,
            SHA256_OF_ABC
        );
        // A web page standing in for a missing file has no checksum in it.
        assert_eq!(
            find_in_list("<html><body>Not found</body></html>", &["a.iso"]),
            None
        );
    }

    #[test]
    fn checksums_are_looked_for_beside_the_file() {
        assert_eq!(
            list_addresses("https://user:pw@example.com/pub/a.iso?token=1#x"),
            [
                "https://example.com/pub/a.iso.sha256",
                "https://example.com/pub/a.iso.sha256sum",
                "https://example.com/pub/a.iso.sha512",
                "https://example.com/pub/a.iso.sha1",
                "https://example.com/pub/a.iso.md5",
                "https://example.com/pub/SHA256SUMS",
                "https://example.com/pub/SHA512SUMS",
            ]
        );
        assert!(list_addresses("ftp://example.com/a.iso").is_empty());
        assert!(list_addresses("https://example.com/folder/").is_empty());
    }
}
