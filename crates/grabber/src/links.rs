//! Finds the links in web pages and stylesheets.

use scraper::{Html, Selector};
use url::Url;

/// What a link is for, which decides what happens to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// Somewhere to go: a page, or a file offered for download.
    Follow,
    /// Part of the page, such as a picture.
    Asset,
    /// A stylesheet, which can pull in more.
    Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: Url,
    pub kind: LinkKind,
}

/// Elements that link to something, and the attribute holding the address.
const LINKING_ELEMENTS: [(&str, &str, LinkKind); 15] = [
    ("a[href]", "href", LinkKind::Follow),
    ("area[href]", "href", LinkKind::Follow),
    ("iframe[src]", "src", LinkKind::Follow),
    ("frame[src]", "src", LinkKind::Follow),
    ("img[src]", "src", LinkKind::Asset),
    ("source[src]", "src", LinkKind::Asset),
    ("video[src]", "src", LinkKind::Asset),
    ("video[poster]", "poster", LinkKind::Asset),
    ("audio[src]", "src", LinkKind::Asset),
    ("track[src]", "src", LinkKind::Asset),
    ("embed[src]", "src", LinkKind::Asset),
    ("object[data]", "data", LinkKind::Asset),
    ("script[src]", "src", LinkKind::Asset),
    ("link[rel~=\"stylesheet\"][href]", "href", LinkKind::Style),
    ("link[rel~=\"icon\"][href]", "href", LinkKind::Asset),
];

fn selector(text: &str) -> Option<Selector> {
    Selector::parse(text).ok()
}

/// Makes a link absolute, keeping only web addresses and dropping any `#fragment`, which
/// doesn't change what's fetched.
pub fn absolute(base: &Url, link: &str) -> Option<Url> {
    let link = link.trim();
    if link.is_empty() || link.starts_with('#') {
        return None;
    }

    let mut url = base.join(link).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

/// The addresses in a `srcset`, which lists them with their sizes.
pub fn srcset_addresses(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(',')
        .filter_map(|candidate| candidate.split_whitespace().next())
}

/// The address a page's links are relative to.
fn page_base(document: &Html, page: &Url) -> Url {
    selector("base[href]")
        .and_then(|base| {
            let element = document.select(&base).next()?;
            page.join(element.value().attr("href")?).ok()
        })
        .unwrap_or_else(|| page.clone())
}

/// Finds where a stylesheet's `url(...)` and `@import` values are in its text.
pub fn css_reference_spans(css: &str) -> Vec<(usize, usize)> {
    let lower = css.to_ascii_lowercase();
    let mut spans = Vec::new();

    let mut from = 0;
    while let Some(found) = lower[from..].find("url(") {
        let start = from + found + 4;
        let Some(length) = css[start..].find(')') else {
            break;
        };
        spans.push(trim_quotes(css, start, start + length));
        from = start + length;
    }

    let mut from = 0;
    while let Some(found) = lower[from..].find("@import") {
        let start = from + found + 7;
        from = start;
        let rest = css[start..].trim_start();
        let opening = start + (css[start..].len() - rest.len());
        let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            continue;
        };
        if let Some(length) = rest[1..].find(quote) {
            spans.push((opening + 1, opening + 1 + length));
        }
    }

    spans.sort_unstable();
    spans
}

fn trim_quotes(css: &str, start: usize, end: usize) -> (usize, usize) {
    let text = &css[start..end];
    let without_start = text.trim_start();
    let start = start + (text.len() - without_start.len());
    let trimmed = without_start.trim_end();
    let end = start + trimmed.len();

    let quoted = trimmed.len() >= 2
        && (trimmed.starts_with('"') && trimmed.ends_with('"')
            || trimmed.starts_with('\'') && trimmed.ends_with('\''));
    if quoted {
        (start + 1, end - 1)
    } else {
        (start, end)
    }
}

/// The addresses a stylesheet refers to.
pub fn css_links(css: &str, base: &Url) -> Vec<Link> {
    css_reference_spans(css)
        .into_iter()
        .filter_map(|(start, end)| absolute(base, &css[start..end]))
        .map(|url| {
            let kind = if url.path().to_ascii_lowercase().ends_with(".css") {
                LinkKind::Style
            } else {
                LinkKind::Asset
            };
            Link { url, kind }
        })
        .collect()
}

/// The links in a web page.
pub fn page_links(html: &str, page: &Url) -> Vec<Link> {
    let document = Html::parse_document(html);
    let base = page_base(&document, page);
    let mut links = Vec::new();

    for (query, attribute, kind) in LINKING_ELEMENTS {
        let Some(query) = selector(query) else {
            continue;
        };
        for element in document.select(&query) {
            let found = element
                .value()
                .attr(attribute)
                .and_then(|value| absolute(&base, value));
            if let Some(url) = found {
                links.push(Link { url, kind });
            }
        }
    }

    if let Some(query) = selector("img[srcset], source[srcset]") {
        for element in document.select(&query) {
            let value = element.value().attr("srcset").unwrap_or_default();
            links.extend(
                srcset_addresses(value)
                    .filter_map(|address| absolute(&base, address))
                    .map(|url| Link {
                        url,
                        kind: LinkKind::Asset,
                    }),
            );
        }
    }

    if let Some(query) = selector("[style]") {
        for element in document.select(&query) {
            let style = element.value().attr("style").unwrap_or_default();
            links.extend(css_links(style, &base));
        }
    }
    if let Some(query) = selector("style") {
        for element in document.select(&query) {
            links.extend(css_links(&element.text().collect::<String>(), &base));
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Url {
        Url::parse("https://x.test/a/b.html").unwrap()
    }

    fn found(html: &str) -> Vec<(String, LinkKind)> {
        page_links(html, &page())
            .into_iter()
            .map(|link| (link.url.to_string(), link.kind))
            .collect()
    }

    #[test]
    fn links_are_made_absolute() {
        assert_eq!(
            found("<a href='c.html'>x</a><a href='/d.html'>y</a><a href='https://y.test/'>z</a>"),
            [
                ("https://x.test/a/c.html".to_owned(), LinkKind::Follow),
                ("https://x.test/d.html".to_owned(), LinkKind::Follow),
                ("https://y.test/".to_owned(), LinkKind::Follow),
            ]
        );
    }

    #[test]
    fn links_that_are_not_web_addresses_are_skipped() {
        let html = "<a href='mailto:a@x.test'>a</a><a href='javascript:void(0)'>b</a>\
                    <a href='tel:1'>c</a><a href='#top'>d</a><a href=''>e</a>\
                    <a href='data:text/html,hi'>f</a><a href='file:///C:/a'>g</a>";
        assert!(found(html).is_empty());
    }

    #[test]
    fn a_fragment_does_not_make_a_new_address() {
        assert_eq!(
            found("<a href='c.html#part'>x</a>")[0].0,
            "https://x.test/a/c.html"
        );
    }

    #[test]
    fn a_base_tag_changes_where_links_lead() {
        assert_eq!(
            found("<base href='/other/'><a href='c.html'>x</a>")[0].0,
            "https://x.test/other/c.html"
        );
    }

    #[test]
    fn everything_a_page_is_built_from_is_found() {
        let html = r#"<link rel="stylesheet" href="s.css"><link rel="icon" href="i.ico">
            <img src="1.jpg"><video src="2.mp4" poster="3.jpg"></video>
            <audio src="4.mp3"></audio><script src="5.js"></script>
            <object data="6.swf"></object><embed src="7.pdf">"#;
        let links = found(html);

        let assets = links
            .iter()
            .filter(|(_, kind)| *kind == LinkKind::Asset)
            .count();
        assert_eq!(assets, 8);
        assert!(links.contains(&("https://x.test/a/s.css".to_owned(), LinkKind::Style)));
    }

    #[test]
    fn every_size_in_a_srcset_is_found() {
        let links = found("<img srcset='small.jpg 1x, large.jpg 2x'>");
        assert_eq!(links.len(), 2);
        assert!(links[1].0.ends_with("large.jpg"));
    }

    #[test]
    fn pictures_in_inline_styles_are_found() {
        let html = "<div style='background: url(a.png)'></div>\
                    <style>.b { background: url(\"b.png\") }</style>";
        let links = found(html);

        assert!(links.iter().any(|(url, _)| url.ends_with("/a/a.png")));
        assert!(links.iter().any(|(url, _)| url.ends_with("/a/b.png")));
    }

    #[test]
    fn broken_html_still_gives_its_links() {
        assert_eq!(found("<p><a href=c.html>x<div><a href=d.html>y").len(), 2);
    }

    #[test]
    fn stylesheets_give_pictures_and_more_stylesheets() {
        let css = r#"@import "more.css"; @import url(other.css);
            .a { background: URL( 'a.png' ); } .b { background: url(b.png); }
            .c { background: url(data:image/png;base64,AAAA); }"#;
        let links = css_links(css, &page());

        let kinds: Vec<(&str, LinkKind)> = links
            .iter()
            .map(|link| (link.url.path(), link.kind))
            .collect();
        assert!(kinds.contains(&("/a/more.css", LinkKind::Style)));
        assert!(kinds.contains(&("/a/other.css", LinkKind::Style)));
        assert!(kinds.contains(&("/a/a.png", LinkKind::Asset)));
        assert!(kinds.contains(&("/a/b.png", LinkKind::Asset)));
        assert_eq!(links.len(), 4, "data: addresses aren't links");
    }

    #[test]
    fn an_unfinished_stylesheet_does_not_crash() {
        assert!(css_links(".a { background: url(a.png", &page()).is_empty());
        assert!(css_links("@import", &page()).is_empty());
        assert!(css_links("@import \"a.css", &page()).is_empty());
    }
}
