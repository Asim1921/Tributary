//! Saving a site to read with no connection: where each address goes on disk, and
//! pointing links at the saved copies.
//!
//! Addresses come from the website, so they're untrusted. A hostile one must never be
//! able to write outside the project's folder, which is why every part of a path goes
//! through the same cleaning as a file name, and the result is checked again at the end.

use std::path::{Component, Path, PathBuf};

use engine::name;
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use scraper::{Html, Node};
use url::Url;

use crate::links::{absolute, css_reference_spans, srcset_addresses};
use crate::project::extension_of;

const INDEX_PAGE: &str = "index.html";
const FALLBACK_FOLDER: &str = "site";

/// What has to be escaped in a link to a saved file.
const LINK_ESCAPES: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`');

/// A short stable fingerprint, so addresses differing only by query get separate files.
fn fingerprint(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

fn with_fingerprint(file: &str, query: &str) -> String {
    let mark = fingerprint(query);
    match file.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem}-{mark}.{extension}"),
        _ => format!("{file}-{mark}"),
    }
}

/// One part of a path, decoded and made safe.
fn clean_segment(segment: &str) -> String {
    let decoded = percent_decode_str(segment).decode_utf8_lossy();
    name::sanitize(&decoded)
}

/// Where an address goes, relative to the project's folder. `page` says it's a web page,
/// which has to end up with a name a browser opens as one.
pub fn local_path(url: &Url, page: bool) -> PathBuf {
    let host = url.host_str().unwrap_or(FALLBACK_FOLDER);
    let site = match url.port() {
        Some(port) => format!("{host}_{port}"),
        None => host.to_owned(),
    };
    let mut path = PathBuf::from(name::sanitize(&site));

    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.collect())
        .unwrap_or_default();
    let (folders, file) = match segments.split_last() {
        Some((file, folders)) => (folders, *file),
        None => (&[][..], ""),
    };

    for folder in folders.iter().filter(|folder| !folder.is_empty()) {
        path.push(clean_segment(folder));
    }

    let mut file_name = if file.is_empty() {
        INDEX_PAGE.to_owned()
    } else {
        clean_segment(file)
    };
    if page && extension_of(url).is_none() && !file.is_empty() {
        file_name.push_str(".html");
    }
    if let Some(query) = url.query() {
        file_name = with_fingerprint(&file_name, query);
    }
    path.push(file_name);

    // The cleaning above should make this impossible. It's checked anyway, because a
    // mistake here would let a website write anywhere on the computer.
    if path
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        path
    } else {
        PathBuf::from(FALLBACK_FOLDER).join(fingerprint(url.as_str()))
    }
}

/// A link from one saved file to another, both relative to the project's folder.
pub fn relative_link(from: &Path, to: &Path) -> String {
    let from_folders: Vec<_> = from
        .parent()
        .map_or(Vec::new(), |folder| folder.components().collect());
    let to_parts: Vec<_> = to.components().collect();

    let shared = from_folders
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();

    let mut parts: Vec<String> = vec!["..".to_owned(); from_folders.len() - shared];
    parts.extend(to_parts[shared..].iter().map(|part| {
        let text = part.as_os_str().to_string_lossy();
        utf8_percent_encode(&text, LINK_ESCAPES).to_string()
    }));
    parts.join("/")
}

/// Rewrites a link, keeping any `#fragment`. `saved` gives the saved copy of an address,
/// and anything not saved is pointed at the live site, so it still works.
fn rewrite_link(value: &str, base: &Url, saved: &dyn Fn(&Url) -> Option<String>) -> Option<String> {
    let target = absolute(base, value)?;
    let fragment = value.split_once('#').map(|(_, fragment)| fragment);

    let link = saved(&target).unwrap_or_else(|| target.to_string());
    Some(match fragment {
        Some(fragment) => format!("{link}#{fragment}"),
        None => link,
    })
}

/// Points a stylesheet's references at saved copies.
pub fn rewrite_css(css: &str, base: &Url, saved: &dyn Fn(&Url) -> Option<String>) -> String {
    let mut output = String::with_capacity(css.len());
    let mut position = 0;

    for (start, end) in css_reference_spans(css) {
        if start < position {
            continue;
        }
        output.push_str(&css[position..start]);
        let original = &css[start..end];
        output
            .push_str(&rewrite_link(original, base, saved).unwrap_or_else(|| original.to_owned()));
        position = end;
    }

    output.push_str(&css[position..]);
    output
}

fn rewrite_srcset(value: &str, base: &Url, saved: &dyn Fn(&Url) -> Option<String>) -> String {
    let addresses: Vec<&str> = srcset_addresses(value).collect();
    let candidates = value.split(',').zip(addresses);

    candidates
        .map(|(candidate, address)| {
            let link = rewrite_link(address, base, saved).unwrap_or_else(|| address.to_owned());
            candidate.trim().replacen(address, &link, 1)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The attributes that hold a single address.
fn holds_address(attribute: &str) -> bool {
    matches!(attribute, "href" | "src" | "poster" | "data")
}

/// Points a page's links at saved copies. A `<base>` tag goes too, because it would send
/// the new relative links back to the website.
pub fn rewrite_html(html: &str, page: &Url, saved: &dyn Fn(&Url) -> Option<String>) -> String {
    let mut document = Html::parse_document(html);

    let base = scraper::Selector::parse("base[href]")
        .ok()
        .and_then(|query| {
            let element = document.select(&query).next()?;
            page.join(element.value().attr("href")?).ok()
        })
        .unwrap_or_else(|| page.clone());

    let style_texts: Vec<_> = document
        .tree
        .nodes()
        .filter(|node| {
            node.value()
                .as_element()
                .is_some_and(|element| element.name() == "style")
        })
        .flat_map(|node| node.children().map(|child| child.id()).collect::<Vec<_>>())
        .collect();

    for id in style_texts {
        if let Some(mut node) = document.tree.get_mut(id)
            && let Node::Text(text) = node.value()
        {
            text.text = rewrite_css(&text.text, &base, saved).into();
        }
    }

    for node in document.tree.values_mut() {
        let Node::Element(element) = node else {
            continue;
        };
        let is_base = element.name() == "base";

        for (attribute, value) in element.attrs.iter_mut() {
            let attribute = &*attribute.local;
            let rewritten = match attribute {
                "href" if is_base => None,
                "srcset" => Some(rewrite_srcset(value, &base, saved)),
                "style" => Some(rewrite_css(value, &base, saved)),
                _ if holds_address(attribute) => rewrite_link(value, &base, saved),
                _ => None,
            };
            if let Some(rewritten) = rewritten {
                *value = rewritten.into();
            }
        }

        if is_base {
            element
                .attrs
                .retain(|(attribute, _)| &*attribute.local != "href");
        }
    }

    document.html()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(address: &str) -> PathBuf {
        local_path(&Url::parse(address).unwrap(), true)
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn a_site_keeps_its_shape_on_disk() {
        assert_eq!(text(&path_of("https://x.test/")), "x.test/index.html");
        assert_eq!(text(&path_of("https://x.test/a/")), "x.test/a/index.html");
        assert_eq!(text(&path_of("https://x.test/a/b.html")), "x.test/a/b.html");
        assert_eq!(text(&path_of("https://x.test/a/b")), "x.test/a/b.html");
        assert_eq!(
            text(&path_of("http://x.test:8080/a.png")),
            "x.test_8080/a.png"
        );
    }

    #[test]
    fn a_file_with_no_extension_stays_as_it_is() {
        let address = Url::parse("https://x.test/download").unwrap();
        assert_eq!(text(&local_path(&address, false)), "x.test/download");
    }

    #[test]
    fn different_queries_get_different_files() {
        let first = path_of("https://x.test/a.html?page=1");
        let second = path_of("https://x.test/a.html?page=2");

        assert_ne!(first, second);
        assert!(text(&first).ends_with(".html"));
        assert_eq!(first, path_of("https://x.test/a.html?page=1"));
    }

    /// Every way of climbing out of the folder that could arrive in an address.
    const HOSTILE: [&str; 21] = [
        "https://x.test/../../../../Windows/evil.html",
        "https://x.test/%2e%2e/%2e%2e/evil.html",
        "https://x.test/%2E%2E%2F%2E%2E%2Fevil.html",
        "https://x.test/..%2f..%2f..%2fevil.html",
        "https://x.test/..%5c..%5c..%5cevil.html",
        "https://x.test/a/..%5cevil.html",
        "https://x.test/%2e%2e%5c%2e%2e%5cWindows%5cevil.exe",
        "https://x.test/C:/Windows/evil.html",
        "https://x.test/C%3A%5CWindows%5Cevil.html",
        "https://x.test/%5C%5Cserver%5Cshare%5Cevil.html",
        "https://x.test/./././evil.html",
        "https://x.test/.../evil.html",
        "https://x.test/..../evil.html",
        "https://x.test/..%00/evil.html",
        "https://x.test/evil.html%00.png",
        "https://x.test/con/aux/nul.html",
        "https://x.test/a:b.html",
        "https://x.test/evil.html::$DATA",
        "https://x.test/trailing./dots.../evil.",
        "https://x.test/spaces%20/%20evil.html%20",
        "https://x.test/%E2%80%AEgnp.exe",
    ];

    #[test]
    fn no_address_can_climb_out_of_the_project_folder() {
        let root = Path::new(r"D:\Saved");

        for address in HOSTILE {
            for page in [true, false] {
                let local = local_path(&Url::parse(address).unwrap(), page);

                assert!(
                    local
                        .components()
                        .all(|part| matches!(part, Component::Normal(_))),
                    "{address} gave {local:?}"
                );
                assert!(root.join(&local).starts_with(root), "{address}");
            }
        }
    }

    #[test]
    fn no_part_of_a_saved_path_can_mean_something_special_to_windows() {
        for address in HOSTILE {
            let local = local_path(&Url::parse(address).unwrap(), true);

            for part in local.components() {
                let part = part.as_os_str().to_string_lossy();
                assert!(
                    !part.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']),
                    "{part}"
                );
                assert!(!part.ends_with(['.', ' ']), "{part:?} from {address}");
                let stem = part
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                assert!(
                    !matches!(stem.as_str(), "CON" | "AUX" | "NUL" | "PRN"),
                    "{part}"
                );
            }
        }
    }

    #[test]
    fn a_hostile_host_cannot_escape_either() {
        let address = Url::parse("https://..%2f..%2fevil/").ok();
        if let Some(address) = address {
            assert!(
                local_path(&address, true)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            );
        }
    }

    #[test]
    fn links_between_saved_files_are_relative() {
        let link = |from: &str, to: &str| relative_link(Path::new(from), Path::new(to));

        assert_eq!(link("x/index.html", "x/a.html"), "a.html");
        assert_eq!(link("x/a/b.html", "x/c.html"), "../c.html");
        assert_eq!(link("x/a/b.html", "x/a/c/d.png"), "c/d.png");
        assert_eq!(link("x/a/b/c.html", "x/d/e.png"), "../../d/e.png");
        assert_eq!(link("x/a.html", "y/b.html"), "../y/b.html");
    }

    #[test]
    fn saved_names_are_escaped_in_links() {
        assert_eq!(
            relative_link(Path::new("x/a.html"), Path::new("x/my file#1.png")),
            "my%20file%231.png"
        );
    }

    fn page() -> Url {
        Url::parse("https://x.test/a/page.html").unwrap()
    }

    /// Stands in for the saved copies: only `/a/known.*` is saved.
    fn saved(url: &Url) -> Option<String> {
        url.path()
            .contains("known")
            .then(|| "local/known".to_owned())
    }

    #[test]
    fn links_to_saved_pages_become_local_and_others_stay_live() {
        let html = "<a href='known.html'>a</a><a href='other.html'>b</a>\
                    <a href='mailto:a@x.test'>c</a>";
        let output = rewrite_html(html, &page(), &saved);

        assert!(output.contains(r#"href="local/known""#), "{output}");
        assert!(
            output.contains(r#"href="https://x.test/a/other.html""#),
            "{output}"
        );
        assert!(output.contains(r#"href="mailto:a@x.test""#), "{output}");
    }

    #[test]
    fn a_fragment_survives_being_pointed_at_a_saved_page() {
        let output = rewrite_html("<a href='known.html#part'>a</a>", &page(), &saved);
        assert!(output.contains(r#"href="local/known#part""#), "{output}");
    }

    #[test]
    fn a_link_within_the_page_is_left_alone() {
        let output = rewrite_html("<a href='#top'>a</a>", &page(), &saved);
        assert!(output.contains(r##"href="#top""##), "{output}");
    }

    #[test]
    fn pictures_and_their_alternatives_are_pointed_at_saved_copies() {
        let html = "<img src='known.png' srcset='known.png 1x, known2.png 2x, other.png 3x'>";
        let output = rewrite_html(html, &page(), &saved);

        assert!(output.contains(r#"src="local/known""#), "{output}");
        assert!(output.contains("local/known 1x"), "{output}");
        assert!(output.contains("local/known 2x"), "{output}");
        assert!(output.contains("https://x.test/a/other.png 3x"), "{output}");
    }

    #[test]
    fn a_base_tag_is_honoured_and_then_removed() {
        let html = "<base href='/known/'><a href='page.html'>a</a>";
        let output = rewrite_html(html, &page(), &saved);

        assert!(output.contains(r#"href="local/known""#), "{output}");
        assert!(!output.contains("/known/"), "{output}");
    }

    #[test]
    fn styles_in_a_page_are_pointed_at_saved_copies() {
        let html = "<style>.a { background: url(known.png) }</style>\
                    <div style='background: url(\"known.png\")'></div>";
        let output = rewrite_html(html, &page(), &saved);

        assert_eq!(output.matches("local/known").count(), 2, "{output}");
    }

    #[test]
    fn stylesheets_keep_everything_but_their_addresses() {
        let css = ".a { color: red; background: url(known.png) no-repeat; }\n\
                   @import \"known.css\";\n.b { background: url(data:image/png;base64,AAAA); }";
        let output = rewrite_css(css, &page(), &saved);

        assert_eq!(
            output,
            ".a { color: red; background: url(local/known) no-repeat; }\n\
             @import \"local/known\";\n.b { background: url(data:image/png;base64,AAAA); }"
        );
    }

    #[test]
    fn scripts_in_a_page_are_not_touched() {
        let html = "<script>var a = 'url(known.png)';</script>";
        assert!(rewrite_html(html, &page(), &saved).contains("url(known.png)"));
    }
}
