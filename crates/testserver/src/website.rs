//! A small website for testing a crawler, served under `/site/`.
//!
//! It has what a crawler has to cope with: links that go round in circles, pages several
//! levels down, a private area in `robots.txt`, broken and off-site links, a redirect, a
//! `<base>` tag, stylesheets that pull in pictures, a file with no extension, and a trap
//! that generates pages for ever.

/// Keeps people out of `/site/private/`.
pub const ROBOTS: &str = "User-agent: *\nDisallow: /site/private/\n";

/// A generated file on the site, as its path below `/site/`, size and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteFile {
    pub path: &'static str,
    pub size_kib: u64,
    pub seed: u64,
}

const fn file(path: &'static str, size_kib: u64, seed: u64) -> SiteFile {
    SiteFile {
        path,
        size_kib,
        seed,
    }
}

pub const FILES: [SiteFile; 15] = [
    file("images/logo.png", 12, 50_001),
    file("images/logo-2x.png", 24, 50_002),
    file("images/bg.png", 8, 50_003),
    file("images/extra.png", 9, 50_004),
    file("gallery/a.jpg", 40, 50_005),
    file("gallery/b.jpg", 41, 50_006),
    file("gallery/c.png", 42, 50_007),
    file("files/report.pdf", 60, 50_008),
    file("files/data.zip", 700, 50_009),
    file("files/notes.txt", 5, 50_010),
    file("files/clip.mp4", 90, 50_011),
    file("files/song.mp3", 30, 50_012),
    file("files/via-base.pdf", 11, 50_013),
    file("files/deep-report.pdf", 13, 50_014),
    file("private/hidden.zip", 20, 50_015),
];

/// A file that doesn't show what it is in its address.
pub const UNNAMED_FILE: SiteFile = file("files/download", 15, 50_016);

fn html(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><title>{title}</title></head><body>\n{body}\n</body></html>\n"
    )
}

const HOME: &str = r##"<link rel="stylesheet" href="style.css">
<h1 style="background: url(images/logo.png)">Home</h1>
<img src="images/logo.png" srcset="images/logo-2x.png 2x, images/logo.png 1x" alt="">
<ul>
  <li><a href="about.html">About</a></li>
  <li><a href="gallery/">Gallery</a></li>
  <li><a href="files/">Files</a></li>
  <li><a href="moved.html">Moved</a></li>
  <li><a href="private/secret.html">Private</a></li>
  <li><a href="broken.html">Broken</a></li>
  <li><a href="http://other.test/elsewhere.html">Elsewhere</a></li>
  <li><a href="http://other.test/elsewhere.zip">Elsewhere file</a></li>
  <li><a href="mailto:someone@x.test">Mail</a></li>
  <li><a href="javascript:void(0)">Script</a></li>
  <li><a href="#top">Top</a></li>
  <li><a href="trap/1">Trap</a></li>
</ul>"##;

const ABOUT: &str = r##"<p>About this site.</p>
<a href="index.html">Home</a>
<a href="/site/">Home again</a>
<a href="about.html#team">Same page</a>
<a href="deep/one.html">Deeper</a>"##;

const GALLERY: &str = r#"<img src="a.jpg" alt=""><img src="b.jpg" alt="">
<a href="c.png">Large</a>
<a href="based.html">A page with a base</a>"#;

const BASED: &str = r#"<base href="/site/files/">
<a href="via-base.pdf">Through the base</a>"#;

const FILES_PAGE: &str = r#"<a href="report.pdf">Report</a>
<a href="data.zip">Data</a>
<a href="notes.txt">Notes</a>
<a href="clip.mp4">Clip</a>
<a href="song.mp3">Song</a>
<a href="download">Download</a>
<a href="../private/hidden.zip">Hidden</a>"#;

const STYLE: &str = r#"body { background: url(images/bg.png); }
@import "extra.css";
.logo { background: url("images/logo.png"); }
.inline { background: url(data:image/png;base64,AAAA); }
"#;

const EXTRA_STYLE: &str = ".extra { background: url('images/extra.png'); }\n";

/// A page or stylesheet, with its content type.
pub fn page(path: &str) -> Option<(&'static str, String)> {
    let page = |text| Some(("text/html; charset=utf-8", text));
    let style = |text: &str| Some(("text/css", text.to_owned()));

    match path {
        "" | "index.html" => page(html("Home", HOME)),
        "about.html" => page(html("About", ABOUT)),
        "gallery/" => page(html("Gallery", GALLERY)),
        "gallery/based.html" => page(html("Based", BASED)),
        "files/" => page(html("Files", FILES_PAGE)),
        "private/secret.html" => page(html("Secret", "<a href=\"more.html\">More</a>")),
        "deep/one.html" => page(html("One", "<a href=\"two.html\">Two</a>")),
        "deep/two.html" => page(html("Two", "<a href=\"three.html\">Three</a>")),
        "deep/three.html" => page(html("Three", "<a href=\"four.html\">Four</a>")),
        "deep/four.html" => page(html(
            "Four",
            "<a href=\"../files/deep-report.pdf\">Report</a>",
        )),
        "style.css" => style(STYLE),
        "extra.css" => style(EXTRA_STYLE),
        _ => {
            let number: u64 = path.strip_prefix("trap/")?.parse().ok()?;
            let next = number + 1;
            page(html("Trap", &format!("<a href=\"{next}\">Next</a>")))
        }
    }
}

/// The generated file behind a path, as a name and query for `/file/`.
pub fn generated(path: &str) -> Option<(String, String)> {
    let found = FILES
        .iter()
        .chain([&UNNAMED_FILE])
        .find(|item| item.path == path)?;
    let extension = found.path.rsplit_once('.').map_or("bin", |(_, end)| end);
    Some((
        format!("{}K.{extension}", found.size_kib),
        format!("seed={}", found.seed),
    ))
}

/// Where a moved page went.
pub fn redirect(path: &str) -> Option<&'static str> {
    (path == "moved.html").then_some("/site/about.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_content() {
        for path in [
            "",
            "about.html",
            "gallery/",
            "gallery/based.html",
            "files/",
            "deep/one.html",
            "deep/four.html",
            "style.css",
            "extra.css",
            "trap/5",
        ] {
            assert!(page(path).is_some(), "{path}");
        }
        assert!(page("broken.html").is_none());
        assert!(page("trap/nonsense").is_none());
    }

    #[test]
    fn the_trap_always_leads_further() {
        let (_, text) = page("trap/41").unwrap();
        assert!(text.contains("href=\"42\""));
    }

    #[test]
    fn files_keep_their_type_and_differ() {
        assert_eq!(
            generated("files/report.pdf").unwrap(),
            ("60K.pdf".to_owned(), "seed=50008".to_owned())
        );
        assert!(generated("files/download").unwrap().0.ends_with(".bin"));
        assert!(generated("missing.zip").is_none());

        let mut seeds: Vec<u64> = FILES.iter().map(|item| item.seed).collect();
        seeds.sort_unstable();
        seeds.dedup();
        assert_eq!(seeds.len(), FILES.len());
    }

    #[test]
    fn only_the_moved_page_redirects() {
        assert_eq!(redirect("moved.html"), Some("/site/about.html"));
        assert_eq!(redirect("about.html"), None);
    }
}
