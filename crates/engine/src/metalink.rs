//! Metalink files (RFC 5854, and the older 3.0 form): one file's name, size, checksum and
//! every server it can be fetched from, so a download can use several at once.

use quick_xml::Reader;
use quick_xml::events::Event;
use reqwest::Url;

use crate::checksum::Checksum;

/// One file a Metalink describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaFile {
    pub name: String,
    pub size: Option<u64>,
    /// Where it can be fetched, best first.
    pub urls: Vec<String>,
    pub checksum: Option<Checksum>,
}

/// The most Metalink text read.
pub const MAX_BYTES: usize = 1 << 20;

/// Whether an address names a Metalink file.
pub fn is_metalink(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| {
        let path = url.path().to_ascii_lowercase();
        path.ends_with(".meta4") || path.ends_with(".metalink")
    })
}

/// How good each kind of hash is, best first, by the names Metalinks give them.
fn hash_rank(kind: &str) -> Option<u8> {
    match kind.to_ascii_lowercase().replace('-', "").as_str() {
        "sha256" => Some(0),
        "sha512" => Some(1),
        "sha1" => Some(2),
        "md5" => Some(3),
        _ => None,
    }
}

#[derive(Default)]
struct Building {
    name: String,
    size: Option<u64>,
    /// Each address with how it ranks, lower first.
    urls: Vec<(i64, String)>,
    hash: Option<(u8, Checksum)>,
}

/// What an entity such as `&amp;` stands for.
fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// An element whose text is being read: its name, its rank for a URL or kind for a hash,
/// and the text so far, which arrives in pieces around entities such as `&amp;`.
struct Reading {
    tag: String,
    rank: Option<i64>,
    kind: Option<String>,
    text: String,
}

/// The files in a Metalink. Only web and FTP addresses are kept.
pub fn parse(xml: &str) -> Result<Vec<MetaFile>, String> {
    let unreadable = || "the Metalink isn't readable".to_owned();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut files = Vec::new();
    let mut current: Option<Building> = None;
    let mut reading: Option<Reading> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let tag =
                    String::from_utf8_lossy(element.local_name().as_ref()).to_ascii_lowercase();
                let attribute = |wanted: &str| {
                    element.attributes().flatten().find_map(|attribute| {
                        let key = String::from_utf8_lossy(attribute.key.local_name().as_ref())
                            .to_ascii_lowercase();
                        (key == wanted)
                            .then(|| String::from_utf8_lossy(&attribute.value).into_owned())
                    })
                };
                let begin = |rank, kind| Reading {
                    tag: tag.clone(),
                    rank,
                    kind,
                    text: String::new(),
                };
                match tag.as_str() {
                    "file" => {
                        current = Some(Building {
                            name: attribute("name").unwrap_or_default(),
                            ..Building::default()
                        });
                    }
                    "url" => {
                        // Metalink 4 ranks by `priority`, lowest first; 3.0 by `preference`,
                        // highest first.
                        let rank = attribute("priority")
                            .and_then(|value| value.parse::<i64>().ok())
                            .or_else(|| {
                                attribute("preference")
                                    .and_then(|value| value.parse::<i64>().ok())
                                    .map(|preference| 1000 - preference)
                            })
                            .unwrap_or(999_999);
                        reading = Some(begin(Some(rank), None));
                    }
                    "hash" => reading = Some(begin(None, attribute("type"))),
                    "size" => reading = Some(begin(None, None)),
                    _ => {}
                }
            }
            Ok(Event::Text(text)) => {
                if let Some(reading) = reading.as_mut() {
                    reading
                        .text
                        .push_str(&text.decode().map_err(|_| unreadable())?);
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if let Some(reading) = reading.as_mut() {
                    let name = reference.decode().map_err(|_| unreadable())?;
                    reading.text.push(entity(&name).ok_or_else(unreadable)?);
                }
            }
            Ok(Event::End(element)) => {
                let tag =
                    String::from_utf8_lossy(element.local_name().as_ref()).to_ascii_lowercase();
                if let (Some(building), Some(done)) = (current.as_mut(), reading.take()) {
                    let value = done.text.trim();
                    match done.tag.as_str() {
                        "url" => {
                            if let Ok(url) = Url::parse(value)
                                && matches!(url.scheme(), "http" | "https" | "ftp")
                            {
                                building
                                    .urls
                                    .push((done.rank.unwrap_or(999_999), url.to_string()));
                            }
                        }
                        "size" => building.size = value.parse().ok(),
                        "hash" => {
                            let rank = done.kind.as_deref().and_then(hash_rank);
                            if let (Some(rank), Some(checksum)) = (rank, Checksum::parse(value))
                                && building.hash.as_ref().is_none_or(|(best, _)| rank < *best)
                            {
                                building.hash = Some((rank, checksum));
                            }
                        }
                        _ => {}
                    }
                }
                if tag == "file"
                    && let Some(mut building) = current.take()
                {
                    building.urls.sort_by_key(|(rank, _)| *rank);
                    let mut urls: Vec<String> = Vec::new();
                    for (_, url) in building.urls {
                        if !urls.contains(&url) {
                            urls.push(url);
                        }
                    }
                    // A name with folders in it keeps only its last part.
                    let name = building
                        .name
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    if !urls.is_empty() {
                        files.push(MetaFile {
                            name,
                            size: building.size,
                            urls,
                            checksum: building.hash.map(|(_, checksum)| checksum),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(unreadable()),
        }
    }
    if files.is_empty() {
        return Err("the Metalink lists no file Tributary can fetch".to_owned());
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn a_metalink_4_file_is_read() {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <metalink xmlns="urn:ietf:params:xml:ns:metalink">
              <file name="dir/example.iso">
                <size>14471447</size>
                <hash type="md5">900150983cd24fb0d6963f7d28e17f72</hash>
                <hash type="sha-256">{SHA256}</hash>
                <url location="de" priority="2">https://two.test/example.iso</url>
                <url priority="1">https://one.test/example.iso?a=1&amp;b=&#50;</url>
                <url priority="3">rsync://three.test/example.iso</url>
                <metaurl mediatype="torrent" priority="1">https://x.test/example.torrent</metaurl>
              </file>
            </metalink>"#
        );

        let files = parse(&xml).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "example.iso");
        assert_eq!(files[0].size, Some(14_471_447));
        // Entities in an address are read as what they stand for.
        assert_eq!(
            files[0].urls,
            [
                "https://one.test/example.iso?a=1&b=2",
                "https://two.test/example.iso"
            ]
        );
        assert_eq!(files[0].checksum.as_ref().unwrap().hex, SHA256);
    }

    #[test]
    fn the_older_metalink_3_form_is_read() {
        let xml = r#"<metalink version="3.0" xmlns="http://www.metalinker.org/">
          <files><file name="a.zip"><size>10</size>
            <verification><hash type="sha1">a9993e364706816aba3e25717850c26c9cd0d89d</hash></verification>
            <resources>
              <url type="http" preference="10">http://low.test/a.zip</url>
              <url type="http" preference="90">http://high.test/a.zip</url>
            </resources>
          </file></files></metalink>"#;

        let files = parse(xml).unwrap();

        assert_eq!(
            files[0].urls,
            ["http://high.test/a.zip", "http://low.test/a.zip"]
        );
        assert_eq!(files[0].checksum.as_ref().unwrap().hex.len(), 40);
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(parse("<html><body>Not found</body></html>").is_err());
        assert!(parse("not xml at all <").is_err());
        assert!(
            parse(r#"<metalink><file name="a"><url>javascript:alert(1)</url></file></metalink>"#)
                .is_err()
        );
    }

    #[test]
    fn metalink_addresses_are_recognised() {
        assert!(is_metalink("https://x.test/files/a.iso.meta4"));
        assert!(is_metalink("https://x.test/a.METALINK?x=1"));
        assert!(!is_metalink("https://x.test/a.iso"));
        assert!(!is_metalink("not an address"));
    }
}
