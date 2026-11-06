//! A small FTP server for tests, with the same generated files as the HTTP server.
//!
//! The size comes from the last part of the path, so `/pub/8M.bin` is 8 MiB. Anything
//! under `/missing` doesn't exist.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::{JoinHandle, JoinSet};

use crate::content;
use crate::scenario::Scenario;

const CHUNK_BYTES: usize = 64 << 10;

#[derive(Debug, Clone)]
pub struct FtpConfig {
    /// A user name and password to require. `None` accepts anyone.
    pub login: Option<(String, String)>,
    pub allow_resume: bool,
    /// Limits each transfer to this many bytes per second.
    pub rate: Option<u64>,
    /// Cuts a transfer after this many bytes.
    pub drop_after: Option<u64>,
    /// How many transfers to cut.
    pub drop_times: u32,
}

impl Default for FtpConfig {
    fn default() -> Self {
        Self {
            login: None,
            allow_resume: true,
            rate: None,
            drop_after: None,
            drop_times: 1,
        }
    }
}

/// One `RETR` and how it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retrieval {
    pub path: String,
    pub offset: u64,
    pub sent: u64,
    pub active: bool,
}

#[derive(Debug, Default)]
struct Log {
    commands: Vec<String>,
    retrievals: Vec<Retrieval>,
    sessions: u32,
    drops: u32,
}

struct Shared {
    config: FtpConfig,
    log: Mutex<Log>,
}

impl Shared {
    fn log(&self) -> MutexGuard<'_, Log> {
        self.log.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A running server. Dropping it stops the server and closes its connections.
pub struct FtpServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    accept_loop: JoinHandle<()>,
}

impl FtpServer {
    pub async fn start(config: FtpConfig) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            config,
            log: Mutex::default(),
        });

        let accept_loop = tokio::spawn(accept(listener, Arc::clone(&shared)));
        Ok(Self {
            addr,
            shared,
            accept_loop,
        })
    }

    /// The address of a path on this server.
    pub fn url(&self, path: &str) -> String {
        format!("ftp://{}{path}", self.addr)
    }

    /// The same address, with a login in it.
    pub fn url_with_login(&self, user: &str, password: &str, path: &str) -> String {
        format!("ftp://{user}:{password}@{}{path}", self.addr)
    }

    pub fn retrievals(&self) -> Vec<Retrieval> {
        self.shared.log().retrievals.clone()
    }

    pub fn commands(&self) -> Vec<String> {
        self.shared.log().commands.clone()
    }

    pub fn sessions(&self) -> u32 {
        self.shared.log().sessions
    }
}

impl Drop for FtpServer {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    // Held here so that aborting this task also aborts every open connection.
    let mut sessions = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    sessions.spawn(session(stream, Arc::clone(&shared)));
                }
            }
            Some(_) = sessions.join_next() => {}
        }
    }
}

/// Where the next transfer's data goes.
enum Data {
    None,
    Passive(TcpListener),
    Active(SocketAddr),
}

struct Session {
    shared: Arc<Shared>,
    user: Option<String>,
    logged_in: bool,
    offset: u64,
    data: Data,
}

async fn session(stream: TcpStream, shared: Arc<Shared>) {
    shared.log().sessions += 1;
    let mut control = BufReader::new(stream);
    if control
        .get_mut()
        .write_all(b"220 test server ready\r\n")
        .await
        .is_err()
    {
        return;
    }

    let mut session = Session {
        shared,
        user: None,
        logged_in: false,
        offset: 0,
        data: Data::None,
    };

    let mut line = String::new();
    loop {
        line.clear();
        match control.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }

        let command = line.trim_end().to_owned();
        session.shared.log().commands.push(command.clone());

        let (verb, argument) = command.split_once(' ').unwrap_or((&command, ""));
        let verb = verb.to_ascii_uppercase();

        if verb == "RETR" && session.logged_in {
            if retrieve(&mut session, control.get_mut(), argument)
                .await
                .is_err()
            {
                return;
            }
            continue;
        }

        let reply = session.answer(&verb, argument).await;
        if control.get_mut().write_all(reply.as_bytes()).await.is_err() || verb == "QUIT" {
            return;
        }
    }
}

impl Session {
    async fn answer(&mut self, verb: &str, argument: &str) -> String {
        match verb {
            "USER" => {
                self.user = Some(argument.to_owned());
                "331 password please\r\n".to_owned()
            }
            "PASS" => {
                let accepted = self
                    .shared
                    .config
                    .login
                    .as_ref()
                    .is_none_or(|(user, password)| {
                        self.user.as_deref() == Some(user) && argument == password
                    });
                self.logged_in = accepted;
                if accepted {
                    "230 logged in\r\n".to_owned()
                } else {
                    "530 login incorrect\r\n".to_owned()
                }
            }
            "QUIT" => "221 goodbye\r\n".to_owned(),
            _ if !self.logged_in => "530 please log in\r\n".to_owned(),
            "TYPE" => "200 ok\r\n".to_owned(),
            "SIZE" => match size_of(argument) {
                Some(size) => format!("213 {size}\r\n"),
                None => "550 no such file\r\n".to_owned(),
            },
            "MDTM" => match size_of(argument) {
                Some(_) => "213 20261021072800\r\n".to_owned(),
                None => "550 no such file\r\n".to_owned(),
            },
            "REST" if self.shared.config.allow_resume => {
                self.offset = argument.parse().unwrap_or(0);
                "350 ready to resume\r\n".to_owned()
            }
            "PASV" => match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await {
                Ok(listener) => {
                    let port = listener.local_addr().map_or(0, |addr| addr.port());
                    self.data = Data::Passive(listener);
                    format!(
                        "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                        port / 256,
                        port % 256
                    )
                }
                Err(_) => "425 can't open a port\r\n".to_owned(),
            },
            "PORT" => match parse_port(argument) {
                Some(address) => {
                    self.data = Data::Active(address);
                    "200 ok\r\n".to_owned()
                }
                None => "501 bad address\r\n".to_owned(),
            },
            _ => "502 not implemented\r\n".to_owned(),
        }
    }

    fn claim_drop(&self) -> Option<u64> {
        let limit = self.shared.config.drop_after?;
        let mut log = self.shared.log();
        if log.drops < self.shared.config.drop_times {
            log.drops += 1;
            Some(limit)
        } else {
            None
        }
    }
}

fn size_of(path: &str) -> Option<u64> {
    if path.starts_with("/missing") {
        return None;
    }
    let name = path.rsplit('/').next()?;
    Scenario::parse(name, "").ok().map(|scenario| scenario.size)
}

fn parse_port(argument: &str) -> Option<SocketAddr> {
    let numbers: Vec<u16> = argument
        .split(',')
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;

    match numbers[..] {
        [a, b, c, d, high, low] => {
            let ip = Ipv4Addr::new(a as u8, b as u8, c as u8, d as u8);
            Some(SocketAddr::from((ip, high * 256 + low)))
        }
        _ => None,
    }
}

async fn retrieve(session: &mut Session, control: &mut TcpStream, path: &str) -> io::Result<()> {
    let Some(size) = size_of(path) else {
        return control.write_all(b"550 no such file\r\n").await;
    };

    let offset = std::mem::take(&mut session.offset);
    let channel = std::mem::replace(&mut session.data, Data::None);
    control.write_all(b"150 sending\r\n").await?;

    let (mut data, active) = match channel {
        Data::Passive(listener) => (listener.accept().await?.0, false),
        Data::Active(address) => (TcpStream::connect(address).await?, true),
        Data::None => return control.write_all(b"425 no data connection\r\n").await,
    };

    let record = {
        let mut log = session.shared.log();
        log.retrievals.push(Retrieval {
            path: path.to_owned(),
            offset,
            sent: 0,
            active,
        });
        log.retrievals.len() - 1
    };

    let limit = session.claim_drop();
    let rate = session.shared.config.rate;
    let remaining = size.saturating_sub(offset);
    let stop_at = limit.map_or(remaining, |limit| limit.min(remaining));

    let mut sent = 0;
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let began = Instant::now();
    let mut aborted = false;

    while sent < stop_at {
        let count = (stop_at - sent).min(CHUNK_BYTES as u64) as usize;
        content::fill(1, offset + sent, &mut buffer[..count]);
        if data.write_all(&buffer[..count]).await.is_err() {
            aborted = true;
            break;
        }
        sent += count as u64;
        session.shared.log().retrievals[record].sent = sent;

        if let Some(rate) = rate {
            let due = Duration::from_secs_f64(sent as f64 / rate as f64);
            if let Some(wait) = due.checked_sub(began.elapsed()) {
                tokio::time::sleep(wait).await;
            }
        }
    }
    drop(data);

    if aborted || sent < remaining {
        control.write_all(b"426 transfer aborted\r\n").await
    } else {
        control.write_all(b"226 complete\r\n").await
    }
}
