//! Shares a small made-up torrent from this computer only, for the app's end-to-end tests:
//! no DHT, no trackers, listening on 127.0.0.1 at the port given.
//!
//!   seed <folder> <port>
//!
//! Writes `<folder>/album.torrent` for two files, `a.bin` and `b.bin`, prints
//! `sha256 a.bin <hex>` and then `seeding`, and shares until it's stopped.
use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session, SessionOptions,
    create_torrent,
};
use sha2::{Digest, Sha256};

#[tokio::main]
async fn main() {
    let mut arguments = std::env::args().skip(1);
    let folder = PathBuf::from(arguments.next().expect("a folder"));
    let port: u16 = arguments
        .next()
        .expect("a port")
        .parse()
        .expect("a port number");

    let album = folder.join("seeded").join("album");
    fs::create_dir_all(&album).unwrap();
    let a: Vec<u8> = (0..1_500_000u32)
        .map(|index| (index * 13 % 251) as u8)
        .collect();
    fs::write(album.join("a.bin"), &a).unwrap();
    fs::write(album.join("b.bin"), vec![9u8; 700_000]).unwrap();

    let made = create_torrent(
        &album,
        CreateTorrentOptions::default(),
        &BlockingSpawner::new(2),
    )
    .await
    .unwrap();
    let bytes = made.as_bytes().unwrap();
    fs::write(folder.join("album.torrent"), &bytes).unwrap();
    let sha: String = Sha256::digest(&a)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("sha256 a.bin {sha}");

    let session = Session::new_with_opts(
        album.clone(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                enable_upnp_port_forwarding: false,
                ipv4_only: true,
                ..ListenerOptions::default()
            }),
            ..SessionOptions::default()
        },
    )
    .await
    .unwrap();
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                overwrite: true,
                output_folder: Some(album.to_string_lossy().into_owned()),
                ..AddTorrentOptions::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    handle.wait_until_completed().await.unwrap();
    println!("seeding");
    std::future::pending::<()>().await;
}
