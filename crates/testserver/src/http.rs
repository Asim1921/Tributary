//! The small part of HTTP/1.1 this server needs, keeping values exactly as sent.

/// A parsed request line and headers, in original case and order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    pub method: String,
    pub target: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
}

impl RequestHead {
    /// Parses a request head, without its trailing blank line.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.split("\r\n");
        let mut request_line = lines.next()?.split(' ');
        let method = request_line.next()?.to_owned();
        let target = request_line.next()?.to_owned();
        let version = request_line.next()?.to_owned();
        if request_line.next().is_some() || !version.starts_with("HTTP/") {
            return None;
        }

        let headers = lines
            .map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.to_owned(), value.trim().to_owned()))
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Self {
            method,
            target,
            version,
            headers,
        })
    }

    /// Returns the first value of a header, matching its name in any case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Whether the client expects the connection to stay open after this response.
    pub fn keep_alive(&self) -> bool {
        match self.header("connection") {
            Some(value) if value.eq_ignore_ascii_case("close") => false,
            Some(value) if value.eq_ignore_ascii_case("keep-alive") => true,
            _ => self.version == "HTTP/1.1",
        }
    }
}

/// What a `Range` header asks for, given the size of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeRequest {
    /// Serve the whole file. Invalid and multi-part ranges land here, as RFC 9110 allows.
    Full,
    /// Serve `start..=end`.
    Partial { start: u64, end: u64 },
    /// Valid syntax, but no requested byte exists.
    Unsatisfiable,
}

/// Interprets a `Range` header value for a file of `size` bytes.
pub fn parse_range(value: &str, size: u64) -> RangeRequest {
    let Some(spec) = value.strip_prefix("bytes=") else {
        return RangeRequest::Full;
    };
    let Some((first, last)) = spec.trim().split_once('-') else {
        return RangeRequest::Full;
    };
    if spec.contains(',') {
        return RangeRequest::Full;
    }

    if first.is_empty() {
        return match last.parse::<u64>() {
            Ok(0) => RangeRequest::Unsatisfiable,
            Ok(_) if size == 0 => RangeRequest::Unsatisfiable,
            Ok(length) => RangeRequest::Partial {
                start: size.saturating_sub(length),
                end: size - 1,
            },
            Err(_) => RangeRequest::Full,
        };
    }

    let Ok(start) = first.parse::<u64>() else {
        return RangeRequest::Full;
    };
    let end = if last.is_empty() {
        None
    } else {
        match last.parse::<u64>() {
            Ok(end) if end >= start => Some(end),
            _ => return RangeRequest::Full,
        }
    };

    if start >= size {
        return RangeRequest::Unsatisfiable;
    }
    RangeRequest::Partial {
        start,
        end: end.unwrap_or(size - 1).min(size - 1),
    }
}

/// Returns the standard reason phrase for the statuses this server sends.
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        206 => "Partial Content",
        302 => "Found",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        416 => "Range Not Satisfiable",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_header_case_and_order() {
        let head =
            RequestHead::parse("GET /a HTTP/1.1\r\nHost: x\r\nUser-Agent: y\r\naccept: z").unwrap();
        assert_eq!(head.headers[1], ("User-Agent".to_owned(), "y".to_owned()));
        assert_eq!(head.header("ACCEPT"), Some("z"));
    }

    #[test]
    fn rejects_malformed_request_heads() {
        assert_eq!(RequestHead::parse("GET /a"), None);
        assert_eq!(RequestHead::parse("GET /a FTP/1.0"), None);
        assert_eq!(RequestHead::parse("GET /a HTTP/1.1\r\nno colon here"), None);
    }

    #[test]
    fn keep_alive_follows_the_http_version_by_default() {
        let parse = |text| RequestHead::parse(text).unwrap().keep_alive();
        assert!(parse("GET / HTTP/1.1"));
        assert!(!parse("GET / HTTP/1.0"));
        assert!(!parse("GET / HTTP/1.1\r\nConnection: close"));
        assert!(parse("GET / HTTP/1.0\r\nConnection: Keep-Alive"));
    }

    #[test]
    fn interprets_range_forms() {
        use RangeRequest::*;
        let cases = [
            ("bytes=0-9", Partial { start: 0, end: 9 }),
            ("bytes=90-", Partial { start: 90, end: 99 }),
            ("bytes=90-500", Partial { start: 90, end: 99 }),
            ("bytes=-10", Partial { start: 90, end: 99 }),
            ("bytes=-500", Partial { start: 0, end: 99 }),
            ("bytes=100-", Unsatisfiable),
            ("bytes=-0", Unsatisfiable),
            ("bytes=9-1", Full),
            ("bytes=0-1,5-6", Full),
            ("bytes=abc", Full),
            ("items=0-9", Full),
        ];
        for (value, expected) in cases {
            assert_eq!(parse_range(value, 100), expected, "{value}");
        }
    }

    #[test]
    fn an_empty_file_has_no_satisfiable_range() {
        assert_eq!(parse_range("bytes=0-", 0), RangeRequest::Unsatisfiable);
        assert_eq!(parse_range("bytes=-5", 0), RangeRequest::Unsatisfiable);
    }
}
