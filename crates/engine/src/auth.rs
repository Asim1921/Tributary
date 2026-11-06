//! Logging in to servers with Basic (RFC 7617) and Digest (RFC 7616) authentication.

use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest as _, Md5};
use sha2::Sha256;

/// A user name and password, held in memory only.
#[derive(Clone, PartialEq, Eq)]
pub struct Login {
    pub username: String,
    pub password: String,
}

impl fmt::Debug for Login {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Login({}, hidden)", self.username)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Md5,
    Sha256,
}

impl Algorithm {
    fn hash(self, text: &str) -> String {
        let bytes: Vec<u8> = match self {
            Self::Md5 => Md5::digest(text).to_vec(),
            Self::Sha256 => Sha256::digest(text).to_vec(),
        };
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha256 => "SHA-256",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    realm: String,
    nonce: String,
    opaque: Option<String>,
    /// Whether the server offers `qop=auth`. Older servers don't.
    qop_auth: bool,
    algorithm: Algorithm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Challenge {
    Basic,
    Digest(DigestChallenge),
}

/// Reads the `WWW-Authenticate` values a server sent, preferring Digest.
pub fn parse_challenge<'a>(values: impl Iterator<Item = &'a str>) -> Option<Challenge> {
    let mut basic = false;

    for value in values {
        let value = value.trim();
        let (scheme, rest) = value.split_once(' ').unwrap_or((value, ""));

        if scheme.eq_ignore_ascii_case("basic") {
            basic = true;
        } else if scheme.eq_ignore_ascii_case("digest")
            && let Some(digest) = parse_digest(rest)
        {
            return Some(Challenge::Digest(digest));
        }
    }
    basic.then_some(Challenge::Basic)
}

fn parse_digest(text: &str) -> Option<DigestChallenge> {
    let parameters = parameters(text);
    let find = |wanted: &str| {
        parameters
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value.as_str())
    };

    let algorithm = match find("algorithm")
        .unwrap_or("MD5")
        .to_ascii_uppercase()
        .as_str()
    {
        "MD5" => Algorithm::Md5,
        "SHA-256" => Algorithm::Sha256,
        _ => return None,
    };

    Some(DigestChallenge {
        realm: find("realm")?.to_owned(),
        nonce: find("nonce")?.to_owned(),
        opaque: find("opaque").map(str::to_owned),
        qop_auth: find("qop").is_some_and(|qop| {
            qop.split(',')
                .any(|item| item.trim().eq_ignore_ascii_case("auth"))
        }),
        algorithm,
    })
}

/// Splits `a=b, c="d,e"` into parameters, respecting quotes.
fn parameters(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    let mut finish = |current: &mut String| {
        if let Some((name, value)) = current.split_once('=') {
            found.push((name.trim().to_owned(), value.trim().to_owned()));
        }
        current.clear();
    };

    for c in text.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if in_quotes && c == '\\' {
            escaped = true;
        } else if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ',' && !in_quotes {
            finish(&mut current);
        } else {
            current.push(c);
        }
    }
    finish(&mut current);
    found
}

fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The response value for one request.
fn digest_response(
    login: &Login,
    digest: &DigestChallenge,
    uri: &str,
    count: u32,
    client_nonce: &str,
) -> String {
    let hash = |text: &str| digest.algorithm.hash(text);
    let secret = hash(&format!(
        "{}:{}:{}",
        login.username, digest.realm, login.password
    ));
    let request = hash(&format!("GET:{uri}"));

    if digest.qop_auth {
        hash(&format!(
            "{secret}:{}:{count:08x}:{client_nonce}:auth:{request}",
            digest.nonce
        ))
    } else {
        hash(&format!("{secret}:{}:{request}", digest.nonce))
    }
}

#[derive(Debug)]
struct State {
    challenge: Challenge,
    count: u32,
}

/// Produces the `Authorization` header for each request, which Digest requires to be
/// different every time.
#[derive(Debug)]
pub struct Authenticator {
    login: Login,
    state: Mutex<State>,
}

impl Authenticator {
    pub fn new(login: Login, challenge: Challenge) -> Self {
        Self {
            login,
            state: Mutex::new(State {
                challenge,
                count: 0,
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes a fresh challenge, such as when a server's nonce has expired. The same
    /// challenge again changes nothing, because restarting the count would repeat
    /// values the server has already seen.
    pub fn renew(&self, challenge: Challenge) {
        let mut state = self.state();
        if state.challenge != challenge {
            state.challenge = challenge;
            state.count = 0;
        }
    }

    /// The header for a GET of `uri`, which is the path and query of the URL.
    pub fn header(&self, uri: &str) -> String {
        let mut state = self.state();
        state.count += 1;
        let count = state.count;

        let digest = match &state.challenge {
            Challenge::Basic => {
                let pair = format!("{}:{}", self.login.username, self.login.password);
                return format!("Basic {}", STANDARD.encode(pair));
            }
            Challenge::Digest(digest) => digest,
        };

        let client_nonce = client_nonce(count);
        let response = digest_response(&self.login, digest, uri, count, &client_nonce);

        let mut header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", algorithm={}, response=\"{response}\"",
            quoted(&self.login.username),
            quoted(&digest.realm),
            quoted(&digest.nonce),
            quoted(uri),
            digest.algorithm.name(),
        );
        if digest.qop_auth {
            header.push_str(&format!(
                ", qop=auth, nc={count:08x}, cnonce=\"{client_nonce}\""
            ));
        }
        if let Some(opaque) = &digest.opaque {
            header.push_str(&format!(", opaque=\"{}\"", quoted(opaque)));
        }
        header
    }
}

fn client_nonce(count: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{nanos:x}{count:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login(username: &str, password: &str) -> Login {
        Login {
            username: username.to_owned(),
            password: password.to_owned(),
        }
    }

    fn challenge(text: &str) -> Option<Challenge> {
        parse_challenge(std::iter::once(text))
    }

    #[test]
    fn matches_the_worked_example_in_rfc_2617() {
        let digest = DigestChallenge {
            realm: "testrealm@host.com".to_owned(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".to_owned(),
            opaque: None,
            qop_auth: true,
            algorithm: Algorithm::Md5,
        };

        let response = digest_response(
            &login("Mufasa", "Circle Of Life"),
            &digest,
            "/dir/index.html",
            1,
            "0a4f113b",
        );

        assert_eq!(response, "6629fae49393a05397450978507c4ef1");
    }

    #[test]
    fn matches_the_sha_256_example_in_rfc_7616() {
        let digest = DigestChallenge {
            realm: "http-auth@example.org".to_owned(),
            nonce: "7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v".to_owned(),
            opaque: None,
            qop_auth: true,
            algorithm: Algorithm::Sha256,
        };

        let response = digest_response(
            &login("Mufasa", "Circle of Life"),
            &digest,
            "/dir/index.html",
            1,
            "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ",
        );

        assert_eq!(
            response,
            "753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1"
        );
    }

    #[test]
    fn basic_login_is_encoded_as_the_standard_says() {
        let auth = Authenticator::new(login("Aladdin", "open sesame"), Challenge::Basic);
        assert_eq!(auth.header("/"), "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
    }

    #[test]
    fn reads_challenges_of_both_kinds() {
        assert_eq!(challenge("Basic realm=\"x\""), Some(Challenge::Basic));
        assert!(matches!(
            challenge("Digest realm=\"a, b\", nonce=\"n\", qop=\"auth,auth-int\""),
            Some(Challenge::Digest(_))
        ));
    }

    #[test]
    fn prefers_digest_when_a_server_offers_both() {
        let values = ["Basic realm=\"x\"", "Digest realm=\"x\", nonce=\"n\""];
        assert!(matches!(
            parse_challenge(values.into_iter()),
            Some(Challenge::Digest(_))
        ));
    }

    #[test]
    fn refuses_what_it_cannot_answer() {
        assert_eq!(challenge("NTLM"), None);
        assert_eq!(challenge("Digest realm=\"x\""), None, "no nonce");
        assert_eq!(
            challenge("Digest realm=\"x\", nonce=\"n\", algorithm=SHA-512-256"),
            None
        );
    }

    #[test]
    fn each_digest_request_gets_a_new_count_and_response() {
        let found = challenge("Digest realm=\"r\", nonce=\"n\", qop=\"auth\"").unwrap();
        let auth = Authenticator::new(login("a", "b"), found);

        let first = auth.header("/x");
        let second = auth.header("/x");

        assert!(first.contains("nc=00000001"));
        assert!(second.contains("nc=00000002"));
        assert_ne!(first, second);
    }

    #[test]
    fn a_new_nonce_restarts_the_count() {
        let first = challenge("Digest realm=\"r\", nonce=\"n1\", qop=\"auth\"").unwrap();
        let second = challenge("Digest realm=\"r\", nonce=\"n2\", qop=\"auth\"").unwrap();
        let auth = Authenticator::new(login("a", "b"), first);
        auth.header("/x");

        auth.renew(second);

        assert!(auth.header("/x").contains("nc=00000001"));
    }

    #[test]
    fn the_same_nonce_again_keeps_counting() {
        let found = challenge("Digest realm=\"r\", nonce=\"n\", qop=\"auth\"").unwrap();
        let auth = Authenticator::new(login("a", "b"), found.clone());
        auth.header("/x");

        auth.renew(found);

        assert!(
            auth.header("/x").contains("nc=00000002"),
            "a replay would be refused"
        );
    }

    #[test]
    fn older_servers_without_qop_still_work() {
        let found = challenge("Digest realm=\"r\", nonce=\"n\"").unwrap();
        let header = Authenticator::new(login("a", "b"), found).header("/x");

        assert!(!header.contains("qop="));
        assert!(!header.contains("nc="));
    }

    #[test]
    fn passes_the_opaque_value_back() {
        let found = challenge("Digest realm=\"r\", nonce=\"n\", opaque=\"abc\"").unwrap();
        let header = Authenticator::new(login("a", "b"), found).header("/x");

        assert!(header.contains("opaque=\"abc\""));
    }

    #[test]
    fn quotes_in_a_user_name_cannot_break_the_header() {
        let found = challenge("Digest realm=\"r\", nonce=\"n\"").unwrap();
        let header = Authenticator::new(login("a\"b", "c"), found).header("/x");

        assert!(header.contains(r#"username="a\"b""#));
    }

    #[test]
    fn passwords_stay_out_of_debug_output() {
        let text = format!("{:?}", login("user", "secret"));

        assert!(!text.contains("secret"));
        assert!(text.contains("user"));
    }
}
