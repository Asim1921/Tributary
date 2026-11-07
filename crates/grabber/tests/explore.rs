//! Explores the test server's website through the real engine.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use engine::{Config, Engine, NewDownload, State};
use grabber::{Crawl, Ended, Fetched, Project, Scope, Source, Template, explore};
use tempfile::TempDir;
use testserver::TestServer;
use tokio_util::sync::CancellationToken;
use url::Url;

struct Site {
    server: TestServer,
    engine: Engine,
    folder: TempDir,
}

impl Source for Site {
    async fn fetch(&self, url: &str) -> Result<Fetched, String> {
        let page = self.engine.fetch_page(url, &[], None).await?;
        Ok(Fetched {
            url: page.url,
            found: page.found,
            content_type: page.content_type,
            text: page.text,
        })
    }

    async fn size(&self, url: &str) -> Option<u64> {
        self.engine.inspect(url, &[], None).await.ok()?.size
    }
}

async fn site() -> Site {
    let folder = tempfile::tempdir().unwrap();
    let config = Config {
        retry: engine::RetryPolicy {
            attempts: 2,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
        },
        ..Config::default()
    };
    Site {
        server: TestServer::start(testserver::Config::default())
            .await
            .unwrap(),
        engine: Engine::open(&folder.path().join("downloads.db"), config).unwrap(),
        folder,
    }
}

impl Site {
    fn out(&self) -> PathBuf {
        self.folder.path().join("out")
    }

    fn project(&self, template: Template) -> Project {
        Project {
            delay_ms: 0,
            max_depth: 6,
            max_pages: 30,
            ..Project::from_template(template, &self.server.url("/site/"), &self.out())
        }
    }

    async fn explored(&self, project: &Project) -> Crawl {
        let mut crawl = Crawl::start(project);
        let stop = CancellationToken::new();
        assert_eq!(
            explore(project, &mut crawl, self, &stop, |_| {}).await,
            Ended::Finished
        );
        crawl
    }

    /// How many times the server was asked for a path.
    fn requests(&self, path: &str) -> usize {
        self.server
            .records()
            .iter()
            .filter(|record| record.target == path)
            .count()
    }

    fn requested(&self) -> Vec<String> {
        self.server
            .records()
            .iter()
            .map(|record| record.target.clone())
            .collect()
    }
}

/// The paths of what was found, below `/site/`.
fn found(crawl: &Crawl) -> HashSet<String> {
    crawl
        .found
        .iter()
        .map(|file| {
            let url = Url::parse(&file.url).unwrap();
            url.path().trim_start_matches("/site/").to_owned()
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn exploring_finds_the_sites_files() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;
    let files = found(&crawl);

    for expected in [
        "images/logo.png",
        "images/logo-2x.png",
        "gallery/a.jpg",
        "gallery/c.png",
        "files/report.pdf",
        "files/data.zip",
        "files/notes.txt",
        "files/clip.mp4",
        "files/song.mp3",
        "files/via-base.pdf",
        "files/deep-report.pdf",
    ] {
        assert!(
            files.contains(expected),
            "{expected} missing from {files:?}"
        );
    }
    assert!(crawl.finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_with_no_extension_turns_out_to_be_a_file() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;

    assert!(found(&crawl).contains("files/download"));
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_page_is_built_from_is_not_listed_as_a_file() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;

    let files = found(&crawl);
    assert!(!files.contains("style.css"), "{files:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_private_area_is_never_requested() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;

    assert!(crawl.pages_refused >= 1);
    assert_eq!(site.requests("/site/private/secret.html"), 0);
    assert!(!found(&crawl).contains("private/hidden.zip"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_sites_rules_are_read_once() {
    let site = site().await;
    site.explored(&site.project(Template::AllFiles)).await;

    assert_eq!(site.requests("/robots.txt"), 1);
    assert_eq!(site.requested()[0], "/robots.txt", "rules come first");
}

#[tokio::test(flavor = "multi_thread")]
async fn links_going_round_in_circles_read_each_page_once() {
    let site = site().await;
    // A redirect has to be asked for before anyone can know where it leads.
    let project = Project {
        exclude_pages: vec!["*/moved.html".to_owned()],
        ..site.project(Template::AllFiles)
    };
    site.explored(&project).await;

    let requested = site.requested();
    let unique: HashSet<&String> = requested.iter().collect();
    assert_eq!(requested.len(), unique.len(), "{requested:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redirect_to_a_page_already_read_adds_nothing() {
    let site = site().await;
    let with_redirect = site.explored(&site.project(Template::AllFiles)).await;

    let without_redirect = Project {
        exclude_pages: vec!["*/moved.html".to_owned()],
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&without_redirect).await;

    assert_eq!(with_redirect.pages_read, crawl.pages_read);
    assert_eq!(found(&with_redirect), found(&crawl));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_link_is_noted_and_exploring_carries_on() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;

    assert!(crawl.pages_failed >= 1);
    assert!(crawl.pages_read > 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn other_sites_are_left_alone_by_default() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::AllFiles)).await;

    assert!(
        crawl
            .found
            .iter()
            .all(|file| !file.url.contains("other.test"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn files_on_other_sites_can_be_included_without_exploring_them() {
    let site = site().await;
    let project = Project {
        files_anywhere: true,
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&project).await;

    assert!(
        crawl
            .found
            .iter()
            .any(|file| file.url.ends_with("other.test/elsewhere.zip"))
    );
    assert!(
        !crawl
            .found
            .iter()
            .any(|file| file.url.ends_with("elsewhere.html"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn depth_limits_how_far_links_are_followed() {
    let site = site().await;
    let project = Project {
        max_depth: 1,
        ..site.project(Template::AllFiles)
    };
    site.explored(&project).await;

    assert!(site.requests("/site/about.html") >= 1);
    assert_eq!(site.requests("/site/deep/one.html"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_site_that_generates_pages_for_ever_is_stopped_by_the_limit() {
    let site = site().await;
    let project = Project {
        max_depth: 100_000,
        max_pages: 12,
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&project).await;

    assert!(crawl.pages_read <= 12, "read {}", crawl.pages_read);
    assert!(crawl.finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_scope_stays_within_its_folder() {
    let site = site().await;
    let project = Project {
        start_url: site.server.url("/site/gallery/"),
        scope: Scope::Folder,
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&project).await;

    assert!(found(&crawl).contains("gallery/a.jpg"));
    assert_eq!(site.requests("/site/about.html"), 0);
    assert!(
        !found(&crawl).contains("files/via-base.pdf"),
        "outside the folder"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_template_keeps_only_its_kind_of_file() {
    let site = site().await;
    let crawl = site.explored(&site.project(Template::Pictures)).await;
    let files = found(&crawl);

    assert!(files.contains("gallery/a.jpg"));
    assert!(files.contains("images/logo.png"));
    assert!(!files.contains("files/report.pdf"));
    assert!(!files.contains("files/data.zip"));
}

#[tokio::test(flavor = "multi_thread")]
async fn excluded_pages_are_not_read_and_excluded_files_are_not_listed() {
    let site = site().await;
    let project = Project {
        exclude_pages: vec!["*/gallery/*".to_owned()],
        exclude_files: vec!["*.mp3".to_owned()],
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&project).await;

    assert_eq!(site.requests("/site/gallery/"), 0);
    assert!(!found(&crawl).contains("gallery/a.jpg"));
    assert!(!found(&crawl).contains("files/song.mp3"));
    assert!(found(&crawl).contains("files/report.pdf"));
}

#[tokio::test(flavor = "multi_thread")]
async fn size_limits_drop_files_outside_them() {
    let site = site().await;
    let project = Project {
        min_size: Some(10 * 1024),
        max_size: Some(100 * 1024),
        ..site.project(Template::AllFiles)
    };
    let crawl = site.explored(&project).await;
    let files = found(&crawl);

    assert!(files.contains("files/report.pdf"), "60K fits");
    assert!(!files.contains("files/data.zip"), "700K is too large");
    assert!(!files.contains("files/notes.txt"), "5K is too small");

    let report = crawl
        .found
        .iter()
        .find(|file| file.url.ends_with("report.pdf"));
    assert_eq!(report.unwrap().size, Some(60 * 1024));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_delay_between_requests_is_kept() {
    let site = site().await;
    let delay = 150;
    let project = Project {
        delay_ms: delay,
        max_depth: 1,
        max_pages: 4,
        ..site.project(Template::AllFiles)
    };

    let started = Instant::now();
    let crawl = site.explored(&project).await;

    let wanted = Duration::from_millis(delay * crawl.pages_read as u64);
    assert!(started.elapsed() >= wanted, "took {:?}", started.elapsed());

    let mut times: Vec<u64> = site
        .server
        .records()
        .iter()
        .filter(|record| record.target != "/robots.txt")
        .map(|record| record.at_ms)
        .collect();
    times.sort_unstable();
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= delay - 20, "gaps were {times:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_project_can_ask_for_no_delay_at_all() {
    let site = site().await;
    let project = Project {
        delay_ms: 0,
        max_depth: 1,
        max_pages: 4,
        ..site.project(Template::AllFiles)
    };

    let started = Instant::now();
    let crawl = site.explored(&project).await;

    let least = Duration::from_millis(100 * crawl.pages_read as u64);
    assert!(started.elapsed() >= least, "took {:?}", started.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_exploration_carries_on_from_a_saved_copy() {
    let site = site().await;
    let project = Project {
        exclude_pages: vec!["*/moved.html".to_owned()],
        ..site.project(Template::AllFiles)
    };

    let mut crawl = Crawl::start(&project);
    let stop = CancellationToken::new();
    let stopper = stop.clone();
    let ended = explore(&project, &mut crawl, &site, &stop, |progress| {
        if progress.pages_read >= 3 {
            stopper.cancel();
        }
    })
    .await;
    assert_eq!(ended, Ended::Stopped);
    assert!(!crawl.finished);
    assert!(crawl.pages_waiting() > 0);

    // Saving and reopening is what a project does between sessions.
    let saved = serde_json::to_string(&crawl).unwrap();
    let mut reopened: Crawl = serde_json::from_str(&saved).unwrap();

    let stop = CancellationToken::new();
    assert_eq!(
        explore(&project, &mut reopened, &site, &stop, |_| {}).await,
        Ended::Finished
    );

    let requested = site.requested();
    let unique: HashSet<&String> = requested.iter().collect();
    assert_eq!(
        requested.len(),
        unique.len(),
        "nothing is read twice: {requested:?}"
    );

    let whole = site.explored(&project).await;
    assert_eq!(found(&reopened), found(&whole), "the result is the same");
}

/// A site whose answers are written out by a test.
#[derive(Default)]
struct Scripted {
    pages: HashMap<String, Result<Fetched, String>>,
    asked: Mutex<Vec<String>>,
}

impl Scripted {
    fn page(mut self, url: &str, html: &str) -> Self {
        self.pages.insert(
            url.to_owned(),
            Ok(Fetched {
                url: url.to_owned(),
                found: true,
                content_type: Some("text/html".to_owned()),
                text: Some(html.to_owned()),
            }),
        );
        self
    }

    fn failing(mut self, url: &str) -> Self {
        self.pages
            .insert(url.to_owned(), Err("server down".to_owned()));
        self
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Source for Scripted {
    async fn fetch(&self, url: &str) -> Result<Fetched, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        self.pages.get(url).cloned().unwrap_or(Ok(Fetched {
            url: url.to_owned(),
            found: false,
            content_type: None,
            text: None,
        }))
    }

    async fn size(&self, _url: &str) -> Option<u64> {
        None
    }
}

fn scripted_project(folder: &Path) -> Project {
    Project {
        delay_ms: 0,
        ..Project::from_template(Template::AllFiles, "https://x.test/", folder)
    }
}

async fn run(source: &Scripted, project: &Project) -> Crawl {
    let mut crawl = Crawl::start(project);
    explore(
        project,
        &mut crawl,
        source,
        &CancellationToken::new(),
        |_| {},
    )
    .await;
    crawl
}

#[tokio::test]
async fn a_site_with_no_rules_can_be_explored() {
    let folder = tempfile::tempdir().unwrap();
    let source = Scripted::default().page("https://x.test/", "<a href='a.zip'>a</a>");

    let crawl = run(&source, &scripted_project(folder.path())).await;

    assert_eq!(crawl.found.len(), 1);
}

#[tokio::test]
async fn a_site_whose_rules_cannot_be_read_is_left_alone() {
    let folder = tempfile::tempdir().unwrap();
    let source = Scripted::default()
        .failing("https://x.test/robots.txt")
        .page("https://x.test/", "<a href='a.zip'>a</a>");

    let crawl = run(&source, &scripted_project(folder.path())).await;

    assert_eq!(crawl.pages_read, 0);
    assert_eq!(crawl.pages_refused, 1);
    assert_eq!(source.asked(), ["https://x.test/robots.txt"]);
}

#[tokio::test]
async fn a_requested_delay_longer_than_the_projects_is_obeyed() {
    let folder = tempfile::tempdir().unwrap();
    let source = Scripted::default()
        .page(
            "https://x.test/robots.txt",
            "User-agent: *\nCrawl-delay: 0.4\n",
        )
        .page("https://x.test/", "<a href='b.html'>b</a>")
        .page("https://x.test/b.html", "");

    let started = Instant::now();
    run(&source, &scripted_project(folder.path())).await;

    assert!(started.elapsed() >= Duration::from_millis(800));
}

#[tokio::test]
async fn being_redirected_off_the_site_does_not_lead_exploring_there() {
    let folder = tempfile::tempdir().unwrap();
    let mut source = Scripted::default().page("https://x.test/", "<a href='away.html'>a</a>");
    source.pages.insert(
        "https://x.test/away.html".to_owned(),
        Ok(Fetched {
            url: "https://y.test/landing.html".to_owned(),
            found: true,
            content_type: Some("text/html".to_owned()),
            text: Some("<a href='more.html'>more</a>".to_owned()),
        }),
    );

    let crawl = run(&source, &scripted_project(folder.path())).await;

    assert!(!source.asked().iter().any(|url| url.contains("y.test")));
    assert_eq!(crawl.pages_read, 1);
}

#[tokio::test]
async fn absurdly_long_addresses_are_ignored() {
    let folder = tempfile::tempdir().unwrap();
    let long = "a".repeat(5000);
    let html = format!("<a href='{long}.html'>a</a><a href='{long}.zip'>b</a>");
    let source = Scripted::default().page("https://x.test/", &html);

    let crawl = run(&source, &scripted_project(folder.path())).await;

    assert!(crawl.found.is_empty());
    assert_eq!(crawl.pages_read, 1);
}

fn files_under(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[tokio::test]
async fn a_hostile_site_cannot_write_outside_the_project_folder() {
    let outer = tempfile::tempdir().unwrap();
    let inside = outer.path().join("a").join("b").join("project");

    let html = r#"<a href="/%2e%2e/%2e%2e/%2e%2e/escaped1.html">1</a>
        <a href="/..%2f..%2f..%2fescaped2.html">2</a>
        <a href="/..%5c..%5c..%5cescaped3.html">3</a>
        <a href="/x/%2e%2e%5c%2e%2e%5c%2e%2e%5c%2e%2e%5cescaped4.html">4</a>"#;

    let mut source = Scripted::default().page("https://x.test/", html);
    for link in [
        "https://x.test/..%2f..%2f..%2fescaped2.html",
        "https://x.test/..%5c..%5c..%5cescaped3.html",
        "https://x.test/x/%2e%2e%5c%2e%2e%5c%2e%2e%5c%2e%2e%5cescaped4.html",
        "https://x.test/escaped1.html",
    ] {
        source = source.page(link, "<p>written by the site</p>");
    }

    let project = Project {
        offline: true,
        keep_folders: true,
        folder: inside.clone(),
        ..scripted_project(&inside)
    };
    let crawl = run(&source, &project).await;

    assert!(crawl.saved_pages.len() >= 4, "{:?}", crawl.saved_pages);
    for file in files_under(outer.path()) {
        assert!(file.starts_with(&inside), "escaped to {file:?}");
    }
}

/// Every local link in a saved page, as files it should lead to. The saved folder is
/// mirrored under a made-up address, so links resolve exactly as they would on disk.
fn local_targets(folder: &Path, page: &Path) -> Vec<PathBuf> {
    let html = fs::read_to_string(folder.join(page)).unwrap();
    let mirrored = page.to_string_lossy().replace('\\', "/");
    let base = Url::parse(&format!("https://saved.test/{mirrored}")).unwrap();

    grabber::links::page_links(&html, &base)
        .into_iter()
        .filter(|link| link.url.host_str() == Some("saved.test"))
        .map(|link| {
            let path = percent_encoding::percent_decode_str(link.url.path())
                .decode_utf8_lossy()
                .into_owned();
            folder.join(path.trim_start_matches('/'))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_site_saved_for_offline_reading_works_with_no_connection() {
    let site = site().await;
    let project = Project {
        max_pages: 16,
        max_depth: 6,
        exclude_pages: vec!["*/trap/*".to_owned()],
        ..site.project(Template::OfflineSite)
    };
    let crawl = site.explored(&project).await;

    // The files go through the normal download route, into their places.
    for file in &crawl.found {
        let (folder, name) = project.destination(&Url::parse(&file.url).unwrap());
        let id = site
            .engine
            .add(NewDownload {
                url: file.url.clone(),
                dest_dir: folder,
                file_name: Some(name),
                ..NewDownload::default()
            })
            .unwrap();
        let saved = site.engine.wait(id).await.unwrap();
        assert_eq!(saved.state, State::Completed, "{}", file.url);
    }

    let home = crawl
        .saved_pages
        .iter()
        .find(|page| page.url.ends_with("/site/"))
        .unwrap();
    assert_eq!(home.path.file_name().unwrap(), "index.html");

    let html = fs::read_to_string(project.folder.join(&home.path)).unwrap();
    assert!(html.contains(r#"href="about.html""#), "{html}");
    assert!(html.contains(r#"src="images/logo.png""#), "{html}");
    assert!(
        html.contains("about.html"),
        "the redirect should lead to the saved page"
    );

    let mut checked = 0;
    for page in crawl.saved_pages.iter().filter(|page| !page.style) {
        for target in local_targets(&project.folder, &page.path) {
            assert!(
                target.exists(),
                "{:?} links to missing {target:?}",
                page.path
            );
            checked += 1;
        }
    }
    assert!(checked > 10, "only {checked} links were checked");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_saved_stylesheet_points_at_saved_pictures() {
    let site = site().await;
    let project = Project {
        exclude_pages: vec!["*/trap/*".to_owned()],
        ..site.project(Template::OfflineSite)
    };
    let crawl = site.explored(&project).await;

    let files = found(&crawl);
    assert!(files.contains("images/bg.png"), "{files:?}");
    assert!(
        files.contains("images/extra.png"),
        "through an @import: {files:?}"
    );

    let style = crawl
        .saved_pages
        .iter()
        .find(|page| page.url.ends_with("style.css"))
        .unwrap();
    let css = fs::read_to_string(project.folder.join(&style.path)).unwrap();
    assert!(css.contains("url(images/bg.png)"), "{css}");
    assert!(css.contains("@import \"extra.css\""), "{css}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_saved_page_links_to_what_was_not_saved_on_the_live_site() {
    let site = site().await;
    let project = Project {
        exclude_pages: vec!["*/trap/*".to_owned()],
        ..site.project(Template::OfflineSite)
    };
    let crawl = site.explored(&project).await;

    let home = crawl
        .saved_pages
        .iter()
        .find(|page| page.url.ends_with("/site/"))
        .unwrap();
    let html = fs::read_to_string(project.folder.join(&home.path)).unwrap();

    assert!(html.contains("http://other.test/elsewhere.html"), "{html}");
    assert!(
        html.contains(&site.server.url("/site/private/secret.html")),
        "{html}"
    );
}

fn offline_project(folder: &Path) -> Project {
    Project {
        offline: true,
        keep_folders: true,
        ..scripted_project(folder)
    }
}

#[tokio::test]
async fn stylesheets_do_not_use_up_the_page_limit() {
    let folder = tempfile::tempdir().unwrap();
    let html = "<link rel='stylesheet' href='a.css'><link rel='stylesheet' href='b.css'>";
    let source = Scripted::default()
        .page("https://x.test/", html)
        .page("https://x.test/a.css", "")
        .page("https://x.test/b.css", "");

    let project = Project {
        max_pages: 1,
        ..offline_project(folder.path())
    };
    let crawl = run(&source, &project).await;

    assert_eq!(crawl.pages_read, 1);
    assert_eq!(crawl.saved_pages.len(), 3, "{:?}", crawl.saved_pages);
}

#[tokio::test]
async fn a_saved_page_keeps_what_it_is_built_from_wherever_that_is() {
    let folder = tempfile::tempdir().unwrap();
    let html = "<link rel='stylesheet' href='https://cdn.test/s.css'>\
                <img src='https://cdn.test/a.png'>\
                <script src='https://cdn.test/a.js'></script>";
    let source = Scripted::default()
        .page("https://x.test/", html)
        .page("https://cdn.test/s.css", "");

    let crawl = run(&source, &offline_project(folder.path())).await;

    let urls: Vec<&str> = crawl.found.iter().map(|file| file.url.as_str()).collect();
    assert!(urls.contains(&"https://cdn.test/a.png"), "{urls:?}");
    assert!(urls.contains(&"https://cdn.test/a.js"), "{urls:?}");
    assert_eq!(crawl.saved_pages.len(), 2, "the stylesheet is saved too");
}

#[tokio::test]
async fn saving_a_page_does_not_wander_onto_other_sites() {
    let folder = tempfile::tempdir().unwrap();
    let html = "<a href='https://other.test/page.html'>a</a>\
                <a href='https://other.test/big.zip'>b</a>";
    let source = Scripted::default().page("https://x.test/", html);

    let crawl = run(&source, &offline_project(folder.path())).await;

    assert!(crawl.found.is_empty(), "{:?}", crawl.found);
    assert!(!source.asked().iter().any(|url| url.contains("other.test")));
}

#[tokio::test]
async fn a_chain_of_stylesheets_cannot_go_on_for_ever() {
    let folder = tempfile::tempdir().unwrap();
    let mut source =
        Scripted::default().page("https://x.test/", "<link rel='stylesheet' href='s0.css'>");
    for number in 0..3000 {
        let next = number + 1;
        source = source.page(
            &format!("https://x.test/s{number}.css"),
            &format!("@import \"s{next}.css\";"),
        );
    }

    let project = Project {
        delay_ms: 0,
        ..offline_project(folder.path())
    };
    let mut crawl = Crawl::start(&project);
    let stop = CancellationToken::new();
    let stopper = stop.clone();
    // A real run would take minutes at the minimum delay, so this only checks the count
    // doesn't run past the page budget of a real project's reach.
    explore(&project, &mut crawl, &source, &stop, |progress| {
        if progress.saved_pages.len() >= 5 {
            stopper.cancel();
        }
    })
    .await;

    assert!(crawl.saved_pages.len() >= 5);
}

#[tokio::test]
async fn two_addresses_for_one_page_are_only_rewritten_once() {
    let folder = tempfile::tempdir().unwrap();
    let html = "<a href='index.html'>home</a><img src='https://cdn.test/a.png'>\
                <a href='sub/page.html'>below</a>";
    let source = Scripted::default()
        .page("https://x.test/", html)
        .page("https://x.test/index.html", html)
        .page(
            "https://x.test/sub/page.html",
            "<img src='https://cdn.test/a.png'>",
        );

    run(&source, &offline_project(folder.path())).await;

    let home = fs::read_to_string(folder.path().join("x.test").join("index.html")).unwrap();
    assert!(home.contains(r#"src="../cdn.test/a.png""#), "{home}");
    assert!(!home.contains("x.test/cdn.test"), "{home}");

    let below = folder.path().join("x.test").join("sub").join("page.html");
    let below = fs::read_to_string(below).unwrap();
    assert!(below.contains(r#"src="../../cdn.test/a.png""#), "{below}");
}
