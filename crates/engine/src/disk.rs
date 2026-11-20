//! The `.part` file a download writes into, and moving it to its final name.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::iter;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::FileExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING, MoveFileExW,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;

const MAX_NAME_ATTEMPTS: u32 = 10_000;

/// A file that several connections write into at the same time, at different offsets.
#[derive(Debug)]
pub struct PartFile {
    file: File,
}

impl PartFile {
    /// Creates an empty file, replacing any earlier attempt, sized to `size` if known.
    pub fn create(path: &Path, size: Option<u64>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        mark_sparse(&file);
        if let Some(size) = size {
            file.set_len(size)?;
        }
        Ok(Self { file })
    }

    /// Opens an existing file to continue it.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Self { file })
    }

    /// Writes all of `data` at `offset`, leaving other offsets untouched.
    pub fn write_at(&self, mut data: &[u8], mut offset: u64) -> io::Result<()> {
        while !data.is_empty() {
            let count = self.file.seek_write(data, offset)?;
            if count == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            data = &data[count..];
            offset += count as u64;
        }
        Ok(())
    }

    /// Returns once everything written so far is on the disk.
    pub fn sync(&self) -> io::Result<()> {
        self.file.sync_data()
    }

    pub fn set_len(&self, length: u64) -> io::Result<()> {
        self.file.set_len(length)
    }
}

/// Marks a file as sparse.
///
/// Otherwise NTFS fills everything before a far offset with zeros during the first
/// write there, which stalls a large download. Volumes without sparse support, such
/// as exFAT, return an error here and simply work the slower way.
fn mark_sparse(file: &File) {
    let mut returned = 0u32;
    // SAFETY: the handle is open for this whole call, this control code takes no
    // input or output buffers, and `returned` outlives the call.
    unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_SPARSE,
            ptr::null(),
            0,
            ptr::null_mut(),
            0,
            &mut returned,
            ptr::null_mut(),
        );
    }
}

/// Moves a finished `.part` file to `name` in `folder`, returning where it ended up.
/// An existing file is never replaced: the name gets a number instead.
pub fn finish(part: &Path, folder: &Path, name: &str) -> io::Result<PathBuf> {
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };

    for attempt in 0..MAX_NAME_ATTEMPTS {
        let candidate = match attempt {
            0 => folder.join(name),
            number => folder.join(format!("{stem} ({number}){extension}")),
        };
        match move_without_replacing(part, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "too many files with this name already exist",
    ))
}

/// Tags a finished file as downloaded from the internet, as browsers do, so Windows
/// SmartScreen and Office's Protected View check it before it's opened. Drives that
/// can't hold the extra stream, such as FAT32 and exFAT, are skipped without complaint.
pub fn mark_from_internet(path: &Path, host_url: &str, referrer: Option<&str>) {
    let _ = std::fs::write(zone_stream(path), zone_identifier(host_url, referrer));
}

/// Gives `to` the same internet tag as `from`, if `from` has one. For a file that's
/// rewritten into a new one, such as a video joined with its sound.
pub fn copy_mark(from: &Path, to: &Path) {
    if let Ok(mark) = std::fs::read(zone_stream(from)) {
        let _ = std::fs::write(zone_stream(to), mark);
    }
}

/// The hidden stream Windows keeps a file's origin in.
fn zone_stream(path: &Path) -> PathBuf {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    PathBuf::from(stream)
}

/// What Windows reads: the internet zone, then where the file came from.
fn zone_identifier(host_url: &str, referrer: Option<&str>) -> String {
    let mut text = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if let Some(referrer) = referrer.and_then(shareable_address) {
        text.push_str(&format!("ReferrerUrl={referrer}\r\n"));
    }
    if let Some(host) = shareable_address(host_url) {
        text.push_str(&format!("HostUrl={host}\r\n"));
    }
    text
}

/// A web address with any login taken out of it, or nothing for anything else.
fn shareable_address(address: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(address).ok()?;
    if !matches!(url.scheme(), "http" | "https" | "ftp") {
        return None;
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Some(url.to_string())
}

/// Moves a finished `.part` file over an earlier version of the same download. Only a
/// sync queue does this, and only for a file it downloaded itself.
pub fn replace(part: &Path, target: &Path) -> io::Result<()> {
    move_file(
        part,
        target,
        MOVEFILE_COPY_ALLOWED | MOVEFILE_REPLACE_EXISTING,
    )
}

/// Unlike `std::fs::rename`, this fails rather than replacing an existing file, and
/// the check and the move are a single step. Moving to another drive copies the
/// file, so that can take a while.
fn move_without_replacing(from: &Path, to: &Path) -> io::Result<()> {
    move_file(from, to, MOVEFILE_COPY_ALLOWED)
}

fn move_file(from: &Path, to: &Path, flags: u32) -> io::Result<()> {
    let from = wide(from.as_os_str());
    let to = wide(to.as_os_str());

    // SAFETY: both paths are null-terminated and outlive the call.
    let moved = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) };
    if moved == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::Instant;

    use super::*;

    #[test]
    fn writes_land_at_their_offsets() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.part");

        let file = PartFile::create(&path, Some(10)).unwrap();
        file.write_at(b"end", 7).unwrap();
        file.write_at(b"start", 0).unwrap();
        drop(file);

        assert_eq!(fs::read(&path).unwrap(), b"start\0\0end");
    }

    #[test]
    fn reopening_keeps_existing_data() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.part");

        PartFile::create(&path, Some(4))
            .unwrap()
            .write_at(b"ab", 0)
            .unwrap();
        PartFile::open(&path).unwrap().write_at(b"cd", 2).unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"abcd");
    }

    #[test]
    fn creating_replaces_an_earlier_attempt() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a.part");
        fs::write(&path, b"old contents").unwrap();

        drop(PartFile::create(&path, None).unwrap());

        assert_eq!(fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn a_write_near_the_end_of_a_huge_file_is_immediate() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("huge.part");
        let size = 64u64 << 30;

        let began = Instant::now();
        let file = PartFile::create(&path, Some(size)).unwrap();
        file.write_at(b"tail", size - 4).unwrap();
        file.sync().unwrap();

        let elapsed = began.elapsed().as_secs_f64();
        assert!(elapsed < 5.0, "took {elapsed:.1}s: the file wasn't sparse");
    }

    #[test]
    fn a_marked_file_says_it_came_from_the_internet() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("setup.exe");
        fs::write(&path, b"program").unwrap();

        mark_from_internet(
            &path,
            "https://user:secret@example.com/files/setup.exe?v=2",
            Some("https://example.com/download"),
        );

        let mut stream = path.as_os_str().to_owned();
        stream.push(":Zone.Identifier");
        assert_eq!(
            fs::read_to_string(stream).unwrap(),
            "[ZoneTransfer]\r\nZoneId=3\r\n\
             ReferrerUrl=https://example.com/download\r\n\
             HostUrl=https://example.com/files/setup.exe?v=2\r\n"
        );
        // The file itself is untouched.
        assert_eq!(fs::read(&path).unwrap(), b"program");
    }

    #[test]
    fn only_web_addresses_are_named_in_the_mark() {
        assert_eq!(
            zone_identifier("ftp://me:pw@files.example/a.zip", Some("not an address")),
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=ftp://files.example/a.zip\r\n"
        );
        assert_eq!(
            zone_identifier("file:///C:/a.zip", Some("javascript:alert(1)")),
            "[ZoneTransfer]\r\nZoneId=3\r\n"
        );
    }

    #[test]
    fn a_rewritten_file_keeps_its_mark() {
        let folder = tempfile::tempdir().unwrap();
        let (old, new, plain) = (
            folder.path().join("a.mp4"),
            folder.path().join("a.joining.mp4"),
            folder.path().join("b.mp4"),
        );
        for path in [&old, &new, &plain] {
            fs::write(path, b"video").unwrap();
        }
        mark_from_internet(&old, "https://a.example/v.mp4", None);

        copy_mark(&old, &new);
        copy_mark(&plain, &new.with_extension("other"));

        let read = |path: &Path| {
            let mut stream = path.as_os_str().to_owned();
            stream.push(":Zone.Identifier");
            fs::read_to_string(stream).ok()
        };
        assert_eq!(read(&new), read(&old));
        assert!(read(&new).unwrap().contains("ZoneId=3"));
        // A file that was never marked doesn't hand on a mark.
        assert_eq!(read(&new.with_extension("other")), None);
    }

    #[test]
    fn marking_a_missing_file_does_no_harm() {
        let folder = tempfile::tempdir().unwrap();
        mark_from_internet(
            &folder.path().join("gone").join("a.zip"),
            "https://a.example/",
            None,
        );
    }

    #[test]
    fn finishing_moves_the_file_to_its_name() {
        let folder = tempfile::tempdir().unwrap();
        let part = folder.path().join("a.zip.1.part");
        fs::write(&part, b"data").unwrap();

        let target = finish(&part, folder.path(), "a.zip").unwrap();

        assert_eq!(target, folder.path().join("a.zip"));
        assert_eq!(fs::read(&target).unwrap(), b"data");
        assert!(!part.exists());
    }

    #[test]
    fn finishing_never_replaces_an_existing_file() {
        let folder = tempfile::tempdir().unwrap();
        let part = folder.path().join("x.part");
        fs::write(&part, b"new").unwrap();
        fs::write(folder.path().join("a.tar.gz"), b"keep").unwrap();
        fs::write(folder.path().join("a.tar (1).gz"), b"keep too").unwrap();

        let target = finish(&part, folder.path(), "a.tar.gz").unwrap();

        assert_eq!(target, folder.path().join("a.tar (2).gz"));
        assert_eq!(fs::read(folder.path().join("a.tar.gz")).unwrap(), b"keep");
    }

    #[test]
    fn numbering_works_without_an_extension() {
        let folder = tempfile::tempdir().unwrap();
        let part = folder.path().join("x.part");
        fs::write(&part, b"new").unwrap();
        fs::write(folder.path().join("notes"), b"keep").unwrap();

        let target = finish(&part, folder.path(), "notes").unwrap();

        assert_eq!(target, folder.path().join("notes (1)"));
    }
}
