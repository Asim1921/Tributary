//! A player embedded from another site, as video hosting services offer: a page at one
//! origin holds an iframe from another, and the player answers only when it's asked from
//! the page it's embedded in. The server is reached as both `127.0.0.1` and `localhost`, so
//! the two origins differ without a second server.
//!
//! | Address | Answers |
//! |---------|---------|
//! | `/embed-host?player=ORIGIN&kind=K` | A page holding the player from `ORIGIN` |
//! | `/embedded-player?host=ORIGIN&kind=K` | The player, or 404 unless the `Referer` is `ORIGIN` |
//!
//! `kind=script` feeds the player from a blob by script, so nothing the browser fetches
//! looks like video. `kind=file` plays a file that answers 403 unless it's asked for as the
//! player asked for it. `kind=guarded` loads a stream list that answers only the player
//! page itself, as a site that turns away anything but a browser does. Any `case=` is
//! passed on to the player's address.

use crate::scenario;

/// What a request for one of these pages gets.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Page(String),
    Status(u16),
}

/// Only this test server's own addresses can be named as either side.
fn local_origin(value: &str) -> Option<&str> {
    let rest = value
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| value.strip_prefix("http://localhost:"))?;
    let valid =
        !rest.is_empty() && rest.len() <= 5 && rest.bytes().all(|byte| byte.is_ascii_digit());
    valid.then_some(value)
}

fn value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, raw) = pair.split_once('=')?;
        (name == key)
            .then(|| scenario::decode(key, raw).ok())
            .flatten()
    })
}

fn kind(query: &str) -> Option<&'static str> {
    match value(query, "kind").as_deref() {
        Some("file") => Some("file"),
        Some("guarded") => Some("guarded"),
        Some("script") | None => Some("script"),
        Some(_) => None,
    }
}

/// Only letters, digits and dashes pass on, as they need no escaping anywhere.
fn case(query: &str) -> Option<String> {
    value(query, "case").filter(|case| {
        !case.is_empty()
            && case.len() <= 32
            && case
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The page holding the player. `own` is this page's origin, from the request's `Host`.
pub fn host_page(own: &str, query: &str) -> Reply {
    let (Some(own), Some(player), Some(kind)) = (
        local_origin(own),
        value(query, "player")
            .as_deref()
            .and_then(local_origin)
            .map(str::to_owned),
        kind(query),
    ) else {
        return Reply::Status(400);
    };
    let case = case(query).map_or_else(String::new, |case| format!("&amp;case={case}"));
    Reply::Page(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Embedding test page</title>
<style>
  body {{ font: 16px/1.6 system-ui, sans-serif; max-width: 46rem; margin: 2rem auto; padding: 0 1rem; }}
  iframe {{ display: block; width: 100%; aspect-ratio: 16 / 9; border: 0; background: #222; }}
</style>
</head>
<body>
<h1>Embedding test page</h1>
<p>A player from another site, embedded here. It only answers when asked from this page.</p>
<iframe src="{player}/embedded-player?host={host}&amp;kind={kind}{case}" allow="autoplay"></iframe>
</body>
</html>
"#,
        host = encode(own),
    ))
}

/// The embedded player. It refuses to be shown anywhere but the page it was made for.
pub fn player_page(own: &str, query: &str, referer: Option<&str>) -> Reply {
    let (Some(own), Some(host), Some(kind)) = (
        local_origin(own),
        value(query, "host")
            .as_deref()
            .and_then(local_origin)
            .map(str::to_owned),
        kind(query),
    ) else {
        return Reply::Status(400);
    };
    if !referer.is_some_and(|referer| referer.starts_with(&host)) {
        return Reply::Status(404);
    }

    let player = match kind {
        // Asked for by the player, as the player's site, which is all this file accepts.
        "file" => format!(
            r#"<video id="player" controls muted src="/file/2M.mp4?seed=902&amp;require_referer={}"></video>"#,
            encode(&format!("{own}/"))
        ),
        // Only the player page itself is let in, which nothing outside a browser names.
        "guarded" => format!(
            r#"<video id="player" controls muted src="/file/1K.m3u8?seed=903&amp;require_referer={}"></video>"#,
            encode(&format!("{own}/embedded-player"))
        ),
        _ => r#"<video id="player" controls></video>
<script>
  fetch('/file/1M.bin?seed=78')
    .then((reply) => reply.blob())
    .then((blob) => {
      document.getElementById('player').src = URL.createObjectURL(blob);
    });
</script>"#
            .to_owned(),
    };
    Reply::Page(format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Embedded player</title>
<style>
  html, body {{ margin: 0; height: 100%; background: #111; }}
  video {{ display: block; width: 100%; height: 100%; }}
</style>
</head>
<body>
{player}
</body>
</html>
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "http://127.0.0.1:8792";
    const PLAYER: &str = "http://localhost:8792";

    fn page(reply: Reply) -> String {
        match reply {
            Reply::Page(html) => html,
            Reply::Status(status) => panic!("answered {status}"),
        }
    }

    #[test]
    fn the_host_page_embeds_the_player_from_the_other_origin() {
        let html = page(host_page(
            HOST,
            "player=http%3A%2F%2Flocalhost%3A8792&kind=file&case=stuck",
        ));
        assert!(html.contains(r#"src="http://localhost:8792/embedded-player?host=http%3A%2F%2F127.0.0.1%3A8792&amp;kind=file&amp;case=stuck""#));
    }

    #[test]
    fn the_player_answers_only_when_asked_from_its_page() {
        let query = "host=http%3A%2F%2F127.0.0.1%3A8792&kind=script";

        assert!(matches!(
            player_page(PLAYER, query, Some("http://127.0.0.1:8792/")),
            Reply::Page(_)
        ));
        assert!(matches!(
            player_page(PLAYER, query, Some("http://127.0.0.1:8792/embed-host?x")),
            Reply::Page(_)
        ));
        assert_eq!(player_page(PLAYER, query, None), Reply::Status(404));
        assert_eq!(
            player_page(PLAYER, query, Some("http://localhost:8792/")),
            Reply::Status(404)
        );
    }

    #[test]
    fn the_players_file_accepts_only_the_player_as_referer() {
        let html = page(player_page(
            PLAYER,
            "host=http%3A%2F%2F127.0.0.1%3A8792&kind=file",
            Some(HOST),
        ));
        assert!(html.contains("require_referer=http%3A%2F%2Flocalhost%3A8792%2F"));
    }

    #[test]
    fn a_guarded_players_stream_lets_in_only_the_player_page() {
        let html = page(player_page(
            PLAYER,
            "host=http%3A%2F%2F127.0.0.1%3A8792&kind=guarded",
            Some(HOST),
        ));
        assert!(html.contains("/file/1K.m3u8?seed=903"));
        assert!(html.contains("require_referer=http%3A%2F%2Flocalhost%3A8792%2Fembedded-player"));
    }

    #[test]
    fn only_this_servers_own_addresses_are_accepted() {
        for bad in [
            "player=https%3A%2F%2Fevil.test",
            "player=javascript%3Aalert(1)",
            "player=http%3A%2F%2Flocalhost%3Aabc",
            "kind=file",
        ] {
            assert_eq!(host_page(HOST, bad), Reply::Status(400), "{bad}");
        }
        assert_eq!(
            host_page("http://evil.test", "player=http%3A%2F%2Flocalhost%3A1"),
            Reply::Status(400)
        );
        assert_eq!(
            host_page(HOST, "player=http%3A%2F%2Flocalhost%3A1&kind=other"),
            Reply::Status(400)
        );
    }

    #[test]
    fn a_case_that_needs_escaping_is_left_out() {
        let html = page(host_page(
            HOST,
            "player=http%3A%2F%2Flocalhost%3A1&case=%22%3E%3Cscript%3E",
        ));
        assert!(!html.contains("case="));
        assert!(!html.contains("<script>"));
    }
}
