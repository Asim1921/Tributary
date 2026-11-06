//! Test scenarios, chosen entirely by the request URL so any client can run them.
//!
//! The file size comes from the name, for example `/file/100M.bin`. Sizes accept
//! `K`, `M` and `G` (binary) here and in `rate`, `slow_from` and `drop_after`.
//!
//! | Option                    | Effect                                                       |
//! |---------------------------|--------------------------------------------------------------|
//! | `seed=N`                  | Selects the file content (default 1)                         |
//! | `run=X`                   | Separates repeat runs; fault counters are per exact URL      |
//! | `ranges=0`                | Ignores `Range` and stops advertising `Accept-Ranges`        |
//! | `length=0`                | Sends a chunked body with no `Content-Length`; implies no ranges |
//! | `validators=0`            | Sends no `ETag` or `Last-Modified`                           |
//! | `rate=SIZE`               | Limits each connection to SIZE bytes per second              |
//! | `slow_from=SIZE`          | Applies `rate` only to requests starting at or after SIZE    |
//! | `drop_after=SIZE`         | Cuts the connection after SIZE body bytes                    |
//! | `drop_times=N`            | Number of responses to cut (default 1)                       |
//! | `fail=STATUS`             | Answers early requests with STATUS                           |
//! | `fail_times=N`            | Number of requests to fail (default 1)                       |
//! | `retry_after=SECONDS`     | Adds `Retry-After` to those failures                         |
//! | `limit=N`                 | Answers 503 to requests beyond N at once for this URL        |
//! | `redirects=N`             | Redirects N times before serving                             |
//! | `change_after=N`          | Replaces the file with new content after N requests          |
//! | `require_cookie=NAME=VAL` | Answers 403 unless that cookie is sent                       |
//! | `filename=NAME`           | Sends `Content-Disposition: attachment`                      |
//!
//! Unknown options are rejected, so a typo can't silently run the wrong test.

use std::str::FromStr;

/// One file and the behaviour to apply while serving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub size: u64,
    pub seed: u64,
    pub ranges: bool,
    pub length: bool,
    pub validators: bool,
    pub rate: Option<u64>,
    pub slow_from: Option<u64>,
    pub drop_after: Option<u64>,
    pub drop_times: u32,
    pub fail: Option<u16>,
    pub fail_times: u32,
    pub retry_after: Option<u32>,
    pub limit: Option<u32>,
    pub redirects: u32,
    pub change_after: Option<u32>,
    pub require_cookie: Option<String>,
    pub filename: Option<String>,
}

impl Scenario {
    /// Parses a file name such as `100M.bin` and a query string.
    pub fn parse(name: &str, query: &str) -> Result<Self, String> {
        let mut scenario = Self {
            size: parse_leading_size(name)?,
            seed: 1,
            ranges: true,
            length: true,
            validators: true,
            rate: None,
            slow_from: None,
            drop_after: None,
            drop_times: 1,
            fail: None,
            fail_times: 1,
            retry_after: None,
            limit: None,
            redirects: 0,
            change_after: None,
            require_cookie: None,
            filename: None,
        };

        for (key, value) in pairs(query) {
            match key {
                "seed" => scenario.seed = number(key, value)?,
                "run" => {}
                "ranges" => scenario.ranges = flag(key, value)?,
                "length" => scenario.length = flag(key, value)?,
                "validators" => scenario.validators = flag(key, value)?,
                "rate" => scenario.rate = Some(positive_size(key, value)?),
                "slow_from" => scenario.slow_from = Some(size(key, value)?),
                "drop_after" => scenario.drop_after = Some(size(key, value)?),
                "drop_times" => scenario.drop_times = number(key, value)?,
                "fail" => scenario.fail = Some(status(key, value)?),
                "fail_times" => scenario.fail_times = number(key, value)?,
                "retry_after" => scenario.retry_after = Some(number(key, value)?),
                "limit" => scenario.limit = Some(number(key, value)?),
                "redirects" => scenario.redirects = number(key, value)?,
                "change_after" => scenario.change_after = Some(number(key, value)?),
                "require_cookie" => scenario.require_cookie = Some(decode(key, value)?),
                "filename" => scenario.filename = Some(header_safe(key, decode(key, value)?)?),
                _ => return Err(format!("unknown option `{key}`")),
            }
        }

        if !scenario.length {
            scenario.ranges = false;
        }
        Ok(scenario)
    }

    /// Returns 0 for the original file, or 1 once it has been replaced.
    pub fn version_at(&self, visit: u32) -> u64 {
        u64::from(self.change_after.is_some_and(|limit| visit > limit))
    }

    /// The per-connection rate for a response starting at `start`, if limited.
    pub fn rate_from(&self, start: u64) -> Option<u64> {
        self.rate
            .filter(|_| self.slow_from.is_none_or(|from| start >= from))
    }
}

/// Returns the same query with `redirects` reduced to `remaining`.
pub fn with_redirects(query: &str, remaining: u32) -> String {
    pairs(query)
        .map(|(key, value)| match key {
            "redirects" => format!("redirects={remaining}"),
            _ => format!("{key}={value}"),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn pairs(query: &str) -> impl Iterator<Item = (&str, &str)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
}

fn parse_leading_size(name: &str) -> Result<u64, String> {
    let stem = name.split('.').next().unwrap_or(name);
    parse_size(stem)
        .ok_or_else(|| format!("file name `{name}` must start with a size, such as 100M.bin"))
}

fn parse_size(text: &str) -> Option<u64> {
    let (digits, shift) = match text.as_bytes().last()? {
        b'K' | b'k' => (&text[..text.len() - 1], 10),
        b'M' | b'm' => (&text[..text.len() - 1], 20),
        b'G' | b'g' => (&text[..text.len() - 1], 30),
        _ => (text, 0),
    };
    digits.parse::<u64>().ok()?.checked_mul(1 << shift)
}

fn size(key: &str, value: &str) -> Result<u64, String> {
    parse_size(value).ok_or_else(|| format!("`{key}` must be a size, got `{value}`"))
}

fn positive_size(key: &str, value: &str) -> Result<u64, String> {
    match size(key, value)? {
        0 => Err(format!("`{key}` must be greater than zero")),
        bytes => Ok(bytes),
    }
}

fn number<T: FromStr>(key: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("`{key}` must be a number, got `{value}`"))
}

fn flag(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("`{key}` must be 0 or 1, got `{value}`")),
    }
}

fn status(key: &str, value: &str) -> Result<u16, String> {
    match number(key, value)? {
        code @ 400..=599 => Ok(code),
        code => Err(format!("`{key}` must be a 4xx or 5xx status, got {code}")),
    }
}

/// Decodes `%XX` escapes.
fn decode(key: &str, value: &str) -> Result<String, String> {
    let invalid = || format!("`{key}` has an invalid escape");
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3).ok_or_else(invalid)?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|_| invalid())?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| invalid())
}

/// Blocks values that would break out of the header they are sent in.
fn header_safe(key: &str, value: String) -> Result<String, String> {
    if value.chars().any(|c| c.is_control() || c == '"') {
        return Err(format!(
            "`{key}` contains a character that isn't allowed in a header"
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sizes_from_file_names() {
        assert_eq!(Scenario::parse("100.bin", "").unwrap().size, 100);
        assert_eq!(Scenario::parse("2K.zip", "").unwrap().size, 2048);
        assert_eq!(Scenario::parse("3M.tar.gz", "").unwrap().size, 3 << 20);
        assert_eq!(Scenario::parse("1G", "").unwrap().size, 1 << 30);
        assert!(Scenario::parse("large.bin", "").is_err());
    }

    #[test]
    fn rejects_unknown_options_and_bad_values() {
        assert!(
            Scenario::parse("1K", "drop_aftr=5")
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(Scenario::parse("1K", "ranges=no").is_err());
        assert!(Scenario::parse("1K", "fail=200").is_err());
        assert!(Scenario::parse("1K", "rate=0").is_err());
    }

    #[test]
    fn no_length_implies_no_ranges() {
        assert!(!Scenario::parse("1K", "length=0").unwrap().ranges);
    }

    #[test]
    fn decodes_escaped_values_and_blocks_header_injection() {
        let scenario =
            Scenario::parse("1K", "require_cookie=a%3Db&filename=my%20file.zip").unwrap();
        assert_eq!(scenario.require_cookie.as_deref(), Some("a=b"));
        assert_eq!(scenario.filename.as_deref(), Some("my file.zip"));
        assert!(Scenario::parse("1K", "filename=a%0D%0AX:%20y").is_err());
        assert!(Scenario::parse("1K", "filename=%ZZ").is_err());
    }

    #[test]
    fn the_file_is_replaced_only_after_the_limit() {
        let scenario = Scenario::parse("1K", "change_after=2").unwrap();
        assert_eq!(scenario.version_at(2), 0);
        assert_eq!(scenario.version_at(3), 1);
    }

    #[test]
    fn rate_can_apply_to_part_of_the_file() {
        let scenario = Scenario::parse("1M", "rate=1K&slow_from=512K").unwrap();
        assert_eq!(scenario.rate_from(0), None);
        assert_eq!(scenario.rate_from(512 << 10), Some(1024));
        assert_eq!(
            Scenario::parse("1M", "rate=1K").unwrap().rate_from(0),
            Some(1024)
        );
    }

    #[test]
    fn redirect_targets_keep_other_options() {
        assert_eq!(
            with_redirects("seed=2&redirects=3&run=a", 2),
            "seed=2&redirects=2&run=a"
        );
    }
}
