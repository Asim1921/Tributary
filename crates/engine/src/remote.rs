//! What a server says about a file, and how to classify its failures.

use std::error::Error as _;
use std::time::Duration;

use reqwest::header::{
    AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
    HeaderMap, HeaderName, HeaderValue, LAST_MODIFIED, RANGE, RETRY_AFTER, WWW_AUTHENTICATE,
};
use reqwest::{Client, RequestBuilder, StatusCode, Url};

use crate::auth::{Authenticator, Challenge, parse_challenge};
use crate::name;
use crate::store::Download;

/// What a download reports when a server wants a login it doesn't have.
pub const LOGIN_NEEDED: &str = "this site needs a login";

/// The longest `Retry-After` obeyed, so a bad value can't stall a download for days.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(300);

/// Why a request didn't produce the expected data.
#[derive(Debug)]
pub enum Problem {
    /// The user paused, or another connection hit a fatal problem.
    Stopped,
    /// The file on the server isn't the one being downloaded any more.
    Changed,
    /// Worth trying again, after `wait` if the server gave one.
    Retry {
        reason: String,
        wait: Option<Duration>,
    },
    /// Trying again won't help.
    Fatal(String),
    /// The server wants a login. `challenge` says how, for servers that say.
    LoginRequired { challenge: Option<Challenge> },
}

impl Problem {
    pub fn retry(reason: impl Into<String>) -> Self {
        Self::Retry {
            reason: reason.into(),
            wait: None,
        }
    }

    pub fn from_request(error: reqwest::Error) -> Self {
        Self::retry(describe(error))
    }

    /// Explains the problem in words.
    pub fn into_reason(self) -> String {
        match self {
            Self::Stopped => "stopped".to_owned(),
            Self::Changed => "the file changed on the server".to_owned(),
            Self::Retry { reason, .. } | Self::Fatal(reason) => reason,
            Self::LoginRequired { .. } => LOGIN_NEEDED.to_owned(),
        }
    }

    /// Classifies a response that wasn't the expected success.
    pub fn from_status(status: StatusCode, headers: &HeaderMap) -> Self {
        if status == StatusCode::UNAUTHORIZED {
            let values = headers
                .get_all(WWW_AUTHENTICATE)
                .iter()
                .filter_map(|value| value.to_str().ok());
            return Self::LoginRequired {
                challenge: parse_challenge(values),
            };
        }

        let reason = format!("the server answered {status}");
        let temporary = status.is_server_error()
            || status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS;

        if temporary {
            Self::Retry {
                reason,
                wait: retry_after(headers),
            }
        } else {
            Self::Fatal(reason)
        }
    }
}

/// Describes a request failure and its causes, leaving out the URL because it may
/// hold a signed token.
fn describe(error: reqwest::Error) -> String {
    let error = error.without_url();
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds = text(headers, RETRY_AFTER)?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

pub fn text(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(|value| value.trim().to_owned())
}

/// A parsed `Content-Range`. `start` is unknown for `bytes */N`, `total` for `bytes a-b/*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: Option<u64>,
    pub total: Option<u64>,
}

impl ContentRange {
    pub fn parse(value: &str) -> Option<Self> {
        let (range, total) = value.strip_prefix("bytes ")?.trim().split_once('/')?;
        let start = match range {
            "*" => None,
            _ => Some(range.split_once('-')?.0.parse().ok()?),
        };
        let total = match total {
            "*" => None,
            _ => Some(total.parse().ok()?),
        };
        Some(Self { start, total })
    }

    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        Self::parse(&text(headers, CONTENT_RANGE)?)
    }
}

/// What a server reports about a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// Where the file really is, after redirects.
    pub url: String,
    pub size: Option<u64>,
    pub ranged: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub suggested_name: Option<String>,
    pub content_type: Option<String>,
}

impl Remote {
    /// Whether the address leads to a web page rather than a file.
    pub fn is_web_page(&self) -> bool {
        self.content_type.as_deref().is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value.starts_with("text/html") || value.starts_with("application/xhtml")
        })
    }

    /// The validator for `If-Range`. RFC 9110 doesn't allow a weak ETag there.
    pub fn if_range(&self) -> Option<&str> {
        self.etag
            .as_deref()
            .filter(|etag| !etag.starts_with("W/"))
            .or(self.last_modified.as_deref())
    }

    /// Whether this looks like the file a saved download was fetching.
    ///
    /// A server with no validators can't prove the file is unchanged, so matching
    /// size alone counts. Restarting every such download would make resume useless
    /// on those servers, and a full restart is what a mismatch costs.
    pub fn same_file(&self, saved: &Download) -> bool {
        if self.size != saved.size {
            return false;
        }
        match (&self.etag, &saved.etag) {
            (Some(current), Some(previous)) => current == previous,
            _ => match (&self.last_modified, &saved.last_modified) {
                (Some(current), Some(previous)) => current == previous,
                _ => true,
            },
        }
    }
}

/// Adds a login to a request, if there is one.
pub fn authorize(
    request: RequestBuilder,
    auth: Option<&Authenticator>,
    url: &str,
) -> RequestBuilder {
    let Some(auth) = auth else {
        return request;
    };
    let Ok(parsed) = Url::parse(url) else {
        return request;
    };

    let uri = match parsed.query() {
        Some(query) => format!("{}?{query}", parsed.path()),
        None => parsed.path().to_owned(),
    };
    let Ok(mut value) = HeaderValue::from_str(&auth.header(&uri)) else {
        return request;
    };
    value.set_sensitive(true);
    request.header(AUTHORIZATION, value)
}

/// Asks for one byte, which reveals the size, range support and validators together.
/// `HEAD` is avoided because some servers answer it differently from `GET`.
pub async fn probe(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    auth: Option<&Authenticator>,
) -> Result<Remote, Problem> {
    let request = client
        .get(url)
        .headers(headers.clone())
        .header(RANGE, "bytes=0-0");
    let response = authorize(request, auth, url)
        .send()
        .await
        .map_err(Problem::from_request)?;

    let status = response.status();
    let response_headers = response.headers();
    let range = ContentRange::from_headers(response_headers);

    let (size, ranged) = match status {
        StatusCode::PARTIAL_CONTENT => {
            let total = range.and_then(|range| range.total);
            (total, total.is_some())
        }
        StatusCode::OK => {
            let length = text(response_headers, CONTENT_LENGTH).and_then(|text| text.parse().ok());
            (length, false)
        }
        // Asking for the first byte of an empty file lands here.
        StatusCode::RANGE_NOT_SATISFIABLE => (range.and_then(|range| range.total), false),
        _ => return Err(Problem::from_status(status, response_headers)),
    };

    let suggested_name = text(response_headers, CONTENT_DISPOSITION)
        .and_then(|value| name::from_content_disposition(&value))
        .or_else(|| name::from_url(response.url()));

    let remote = Remote {
        url: response.url().to_string(),
        size,
        ranged,
        etag: text(response_headers, ETAG),
        last_modified: text(response_headers, LAST_MODIFIED),
        suggested_name,
        content_type: text(response_headers, CONTENT_TYPE),
    };

    // Reading the one byte lets the connection be used again for the download itself.
    if status == StatusCode::PARTIAL_CONTENT {
        let _ = response.bytes().await;
    }
    Ok(remote)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reqwest::header::HeaderValue;

    use super::*;
    use crate::store::{DownloadId, State};

    fn remote() -> Remote {
        Remote {
            url: "http://x.test/a".to_owned(),
            size: Some(100),
            ranged: true,
            etag: Some("\"a\"".to_owned()),
            last_modified: Some("yesterday".to_owned()),
            suggested_name: None,
            content_type: None,
        }
    }

    #[test]
    fn recognises_web_pages() {
        let with_type = |value: &str| Remote {
            content_type: Some(value.to_owned()),
            ..remote()
        };

        assert!(with_type("text/html; charset=utf-8").is_web_page());
        assert!(with_type("TEXT/HTML").is_web_page());
        assert!(with_type("application/xhtml+xml").is_web_page());
        assert!(!with_type("video/mp4").is_web_page());
        assert!(!remote().is_web_page());
    }

    fn saved(size: u64, etag: Option<&str>, last_modified: Option<&str>) -> Download {
        Download {
            id: DownloadId(1),
            url: String::new(),
            dest_dir: PathBuf::new(),
            file_name: None,
            headers: Vec::new(),
            size: Some(size),
            etag: etag.map(str::to_owned),
            last_modified: last_modified.map(str::to_owned),
            ranged: true,
            state: State::Paused,
            error: None,
            final_path: None,
            downloaded: 1,
            description: None,
            added_at: 0,
            completed_at: None,
            part_dir: None,
            queue: None,
            category: None,
            replace_on_finish: false,
        }
    }

    #[test]
    fn parses_content_range_forms() {
        let parse = ContentRange::parse;
        assert_eq!(
            parse("bytes 5-9/100"),
            Some(ContentRange {
                start: Some(5),
                total: Some(100)
            })
        );
        assert_eq!(
            parse("bytes 5-9/*"),
            Some(ContentRange {
                start: Some(5),
                total: None
            })
        );
        assert_eq!(
            parse("bytes */100"),
            Some(ContentRange {
                start: None,
                total: Some(100)
            })
        );
        assert_eq!(parse("items 5-9/100"), None);
        assert_eq!(parse("bytes x-y/z"), None);
    }

    #[test]
    fn weak_etags_are_not_used_for_if_range() {
        let mut weak = remote();
        weak.etag = Some("W/\"a\"".to_owned());
        assert_eq!(weak.if_range(), Some("yesterday"));

        assert_eq!(remote().if_range(), Some("\"a\""));

        weak.last_modified = None;
        assert_eq!(weak.if_range(), None);
    }

    #[test]
    fn compares_files_by_size_then_best_validator() {
        let current = remote();
        assert!(current.same_file(&saved(100, Some("\"a\""), Some("other"))));
        assert!(!current.same_file(&saved(100, Some("\"b\""), Some("yesterday"))));
        assert!(!current.same_file(&saved(99, Some("\"a\""), None)));
        assert!(current.same_file(&saved(100, None, Some("yesterday"))));
        assert!(!current.same_file(&saved(100, None, Some("other"))));
        assert!(current.same_file(&saved(100, None, None)));
    }

    #[test]
    fn classifies_statuses() {
        let headers = HeaderMap::new();
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(matches!(
                Problem::from_status(status, &headers),
                Problem::Retry { .. }
            ));
        }
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::FORBIDDEN,
            StatusCode::GONE,
        ] {
            assert!(matches!(
                Problem::from_status(status, &headers),
                Problem::Fatal(_)
            ));
        }
    }

    #[test]
    fn obeys_retry_after_up_to_a_limit() {
        let wait_for = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
            match Problem::from_status(StatusCode::SERVICE_UNAVAILABLE, &headers) {
                Problem::Retry { wait, .. } => wait,
                other => panic!("expected a retry, got {other:?}"),
            }
        };

        assert_eq!(wait_for("7"), Some(Duration::from_secs(7)));
        assert_eq!(wait_for("999999"), Some(MAX_RETRY_AFTER));
        assert_eq!(wait_for("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }
}
