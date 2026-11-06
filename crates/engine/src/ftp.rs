//! A small FTP client: just what downloading needs (RFC 959, RFC 3659).
//!
//! Each connection of a download has its own session, because one FTP control
//! connection can only run one transfer at a time.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use bytes::Bytes;
use percent_encoding::percent_decode_str;
use reqwest::Url;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::auth::Login;
use crate::name;
use crate::remote::{Problem, Remote};

const DEFAULT_PORT: u16 = 21;
const READ_CHUNK_BYTES: usize = 64 << 10;
const ANONYMOUS_USER: &str = "anonymous";
const ANONYMOUS_PASSWORD: &str = "anonymous@";

/// Where a file is, and how to reach it.
#[derive(Debug, Clone)]
pub struct FtpTarget {
    pub url: Url,
    pub login: Option<Login>,
    pub passive: bool,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
}

impl FtpTarget {
    /// The path on the server. Line breaks are refused, because they would let a URL
    /// send the server extra commands.
    fn path(&self) -> Result<String, Problem> {
        let path = percent_decode_str(self.url.path())
            .decode_utf8()
            .map_err(|_| Problem::Fatal("the FTP path isn't valid text".to_owned()))?;

        if path.contains(['\r', '\n']) {
            return Err(Problem::Fatal(
                "the FTP path contains a line break".to_owned(),
            ));
        }
        Ok(path.into_owned())
    }
}

/// Decides what an FTP reply means for a download.
fn problem_for(code: u16, text: &str) -> Problem {
    let reason = format!("the FTP server answered {code} {text}");
    match code {
        // Service not available, data connection problems and busy files are temporary.
        421 | 425 | 426 | 450 | 451 | 452 => Problem::retry(reason),
        530 => Problem::LoginRequired { challenge: None },
        _ => Problem::Fatal(reason),
    }
}

/// Reads the address from a `227 Entering Passive Mode (h1,h2,h3,h4,p1,p2)` reply.
fn parse_passive_port(reply: &str) -> Option<u16> {
    let start = reply.find('(')?;
    let end = reply[start..].find(')')? + start;
    let numbers: Vec<u16> = reply[start + 1..end]
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect::<Option<_>>()?;

    match numbers[..] {
        [_, _, _, _, high, low] if high < 256 && low < 256 => Some(high * 256 + low),
        _ => None,
    }
}

/// How the data connection will be made once `RETR` is sent.
enum DataChannel {
    Passive(TcpStream),
    Active(TcpListener),
}

/// A logged-in control connection.
pub struct FtpSession {
    control: BufReader<TcpStream>,
    target: FtpTarget,
    server_ip: IpAddr,
    local_ip: IpAddr,
}

impl FtpSession {
    pub async fn open(target: &FtpTarget) -> Result<Self, Problem> {
        let host = target
            .url
            .host_str()
            .ok_or_else(|| Problem::Fatal("the FTP address has no host".to_owned()))?;
        let port = target.url.port().unwrap_or(DEFAULT_PORT);

        let stream = timeout(target.connect_timeout, TcpStream::connect((host, port)))
            .await
            .map_err(|_| Problem::retry("the FTP server didn't answer in time"))?
            .map_err(|error| Problem::retry(format!("could not reach the FTP server: {error}")))?;

        let server_ip = stream
            .peer_addr()
            .map_err(|error| Problem::retry(error.to_string()))?
            .ip();
        let local_ip = stream
            .local_addr()
            .map_err(|error| Problem::retry(error.to_string()))?
            .ip();

        let mut session = Self {
            control: BufReader::new(stream),
            target: target.clone(),
            server_ip,
            local_ip,
        };

        let (code, text) = session.reply().await?;
        if code != 220 {
            return Err(problem_for(code, &text));
        }
        session.log_in().await?;
        session.expect("TYPE I", 200).await?;
        Ok(session)
    }

    async fn log_in(&mut self) -> Result<(), Problem> {
        let login = self.target.login.clone();
        let (user, password) = login
            .as_ref()
            .map_or((ANONYMOUS_USER, ANONYMOUS_PASSWORD), |login| {
                (login.username.as_str(), login.password.as_str())
            });

        let (code, text) = self.command(&format!("USER {user}")).await?;
        match code {
            230 => Ok(()),
            331 => {
                let (code, text) = self.command(&format!("PASS {password}")).await?;
                if code == 230 {
                    Ok(())
                } else {
                    Err(problem_for(code, &text))
                }
            }
            _ => Err(problem_for(code, &text)),
        }
    }

    async fn reply(&mut self) -> Result<(u16, String), Problem> {
        let mut line = String::new();
        self.read_line(&mut line).await?;

        let code: u16 = line
            .get(..3)
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| Problem::retry("the FTP server sent an unreadable reply"))?;

        // `123-` starts a reply of several lines, which `123 ` ends.
        if line.as_bytes().get(3) == Some(&b'-') {
            let last = format!("{code} ");
            loop {
                line.clear();
                self.read_line(&mut line).await?;
                if line.starts_with(&last) {
                    break;
                }
            }
        }
        Ok((code, line.get(4..).unwrap_or_default().trim().to_owned()))
    }

    async fn read_line(&mut self, line: &mut String) -> Result<(), Problem> {
        let read = timeout(self.target.read_timeout, self.control.read_line(line))
            .await
            .map_err(|_| Problem::retry("the FTP server stopped answering"))?
            .map_err(|error| Problem::retry(format!("lost the FTP connection: {error}")))?;

        if read == 0 {
            return Err(Problem::retry("the FTP server closed the connection"));
        }
        Ok(())
    }

    async fn command(&mut self, line: &str) -> Result<(u16, String), Problem> {
        self.control
            .get_mut()
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .map_err(|error| Problem::retry(format!("lost the FTP connection: {error}")))?;
        self.reply().await
    }

    async fn expect(&mut self, line: &str, wanted: u16) -> Result<String, Problem> {
        let (code, text) = self.command(line).await?;
        if code == wanted {
            Ok(text)
        } else {
            Err(problem_for(code, &text))
        }
    }

    async fn open_data_channel(&mut self) -> Result<DataChannel, Problem> {
        if self.target.passive {
            let reply = self.expect("PASV", 227).await?;
            let port = parse_passive_port(&reply)
                .ok_or_else(|| Problem::retry("the FTP server sent an unreadable address"))?;

            // The address in the reply is ignored on purpose. It's often wrong behind a
            // router, and following it would let a server send us anywhere.
            let address = SocketAddr::new(self.server_ip, port);
            let stream = timeout(self.target.connect_timeout, TcpStream::connect(address))
                .await
                .map_err(|_| Problem::retry("the FTP data connection timed out"))?
                .map_err(|error| Problem::retry(format!("FTP data connection failed: {error}")))?;
            return Ok(DataChannel::Passive(stream));
        }

        let listener = TcpListener::bind((self.local_ip, 0))
            .await
            .map_err(|error| Problem::retry(format!("could not open an FTP port: {error}")))?;
        let port = listener
            .local_addr()
            .map_err(|error| Problem::retry(error.to_string()))?
            .port();

        let IpAddr::V4(ip) = self.local_ip else {
            return Err(Problem::Fatal(
                "active FTP needs an IPv4 connection".to_owned(),
            ));
        };
        let [a, b, c, d] = ip.octets();
        let (high, low) = (port / 256, port % 256);
        self.expect(&format!("PORT {a},{b},{c},{d},{high},{low}"), 200)
            .await?;
        Ok(DataChannel::Active(listener))
    }

    /// Starts sending the file from `offset`, and returns the stream it arrives on.
    async fn retrieve(&mut self, offset: u64) -> Result<TcpStream, Problem> {
        let path = self.target.path()?;
        let channel = self.open_data_channel().await?;

        if offset > 0 {
            self.expect(&format!("REST {offset}"), 350).await?;
        }

        let (code, text) = self.command(&format!("RETR {path}")).await?;
        if code != 150 && code != 125 {
            return Err(problem_for(code, &text));
        }

        match channel {
            DataChannel::Passive(stream) => Ok(stream),
            DataChannel::Active(listener) => {
                let (stream, _) = timeout(self.target.connect_timeout, listener.accept())
                    .await
                    .map_err(|_| Problem::retry("the FTP server never connected back"))?
                    .map_err(|error| Problem::retry(error.to_string()))?;
                Ok(stream)
            }
        }
    }
}

/// A file arriving over FTP.
pub struct FtpBody {
    data: TcpStream,
    session: FtpSession,
    buffer: Vec<u8>,
    finished: bool,
}

impl FtpBody {
    pub async fn open(target: &FtpTarget, offset: u64) -> Result<Self, Problem> {
        let mut session = FtpSession::open(target).await?;
        let data = session.retrieve(offset).await?;
        Ok(Self {
            data,
            session,
            buffer: vec![0; READ_CHUNK_BYTES],
            finished: false,
        })
    }

    pub async fn next(&mut self) -> Result<Option<Bytes>, Problem> {
        if self.finished {
            return Ok(None);
        }

        let read = timeout(
            self.session.target.read_timeout,
            self.data.read(&mut self.buffer),
        )
        .await
        .map_err(|_| Problem::retry("the FTP transfer stalled"))?
        .map_err(|error| Problem::retry(format!("the FTP transfer broke: {error}")))?;

        if read > 0 {
            return Ok(Some(Bytes::copy_from_slice(&self.buffer[..read])));
        }

        // The data connection closing says nothing by itself. Only the reply on the
        // control connection shows whether the whole file was sent.
        self.finished = true;
        let (code, text) = self.session.reply().await?;
        if (200..300).contains(&code) {
            Ok(None)
        } else {
            Err(problem_for(code, &text))
        }
    }
}

/// Asks an FTP server about a file.
pub async fn probe(target: &FtpTarget) -> Result<Remote, Problem> {
    let mut session = FtpSession::open(target).await?;
    let path = target.path()?;

    let (code, text) = session.command(&format!("SIZE {path}")).await?;
    let size = match code {
        213 => text.trim().parse().ok(),
        550 => return Err(problem_for(code, &text)),
        _ => None,
    };

    let (code, text) = session.command(&format!("MDTM {path}")).await?;
    let last_modified = (code == 213).then(|| text.trim().to_owned());

    let (code, _) = session.command("REST 0").await?;
    let ranged = code == 350 && size.is_some();

    let _ = session.command("QUIT").await;

    Ok(Remote {
        url: target.url.to_string(),
        size,
        ranged,
        etag: None,
        last_modified,
        suggested_name: name::from_url(&target.url),
        content_type: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(url: &str) -> FtpTarget {
        FtpTarget {
            url: Url::parse(url).unwrap(),
            login: None,
            passive: true,
            connect_timeout: Duration::from_secs(1),
            read_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn reads_a_passive_port() {
        let reply = "Entering Passive Mode (127,0,0,1,195,80).";
        assert_eq!(parse_passive_port(reply), Some(195 * 256 + 80));
    }

    #[test]
    fn refuses_a_broken_passive_reply() {
        assert_eq!(parse_passive_port("no address here"), None);
        assert_eq!(parse_passive_port("(1,2,3)"), None);
        assert_eq!(parse_passive_port("(1,2,3,4,999,1)"), None);
        assert_eq!(parse_passive_port("(a,b,c,d,e,f)"), None);
    }

    #[test]
    fn paths_are_decoded() {
        let path = target("ftp://x.test/a%20b/c.zip").path().unwrap();
        assert_eq!(path, "/a b/c.zip");
    }

    #[test]
    fn a_url_cannot_smuggle_in_extra_commands() {
        let result = target("ftp://x.test/a%0D%0ADELE%20b").path();
        assert!(matches!(result, Err(Problem::Fatal(_))));
    }

    #[test]
    fn temporary_and_permanent_replies_are_told_apart() {
        assert!(matches!(problem_for(421, ""), Problem::Retry { .. }));
        assert!(matches!(problem_for(450, ""), Problem::Retry { .. }));
        assert!(matches!(problem_for(550, ""), Problem::Fatal(_)));
        assert!(matches!(
            problem_for(530, ""),
            Problem::LoginRequired { .. }
        ));
    }
}
