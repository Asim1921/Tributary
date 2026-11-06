//! Passwords, kept in Windows Credential Manager rather than in any file of ours.

use std::io;
use std::iter;
use std::mem;
use std::ptr;
use std::slice;

use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

const PREFIX: &str = "Tributary/";

fn target(key: &str) -> Vec<u16> {
    format!("{PREFIX}{key}")
        .encode_utf16()
        .chain(iter::once(0))
        .collect()
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(iter::once(0)).collect()
}

/// Saves a user name and password under `key`, replacing any earlier one.
pub fn store(key: &str, username: &str, password: &str) -> io::Result<()> {
    let mut target = target(key);
    let mut username = wide(username);
    let mut blob: Vec<u8> = password
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();

    // SAFETY: an all-zero CREDENTIALW is valid, and it means no optional fields.
    let mut credential: CREDENTIALW = unsafe { mem::zeroed() };
    credential.Type = CRED_TYPE_GENERIC;
    credential.TargetName = target.as_mut_ptr();
    credential.UserName = username.as_mut_ptr();
    credential.CredentialBlob = blob.as_mut_ptr();
    credential.CredentialBlobSize = blob.len() as u32;
    credential.Persist = CRED_PERSIST_LOCAL_MACHINE;

    // SAFETY: every pointer in `credential` stays valid for the whole call.
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Reads the user name and password saved under `key`.
pub fn read(key: &str) -> Option<(String, String)> {
    let target = target(key);
    let mut credential: *mut CREDENTIALW = ptr::null_mut();

    // SAFETY: the name is null-terminated, and `credential` receives memory that's
    // released below.
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        return None;
    }

    // SAFETY: on success, `credential` points to a valid entry until `CredFree`. The
    // blob holds `CredentialBlobSize` bytes, and the user name is null-terminated.
    let found = unsafe {
        let entry = &*credential;
        // Windows gives a null pointer for an empty password, which a slice can't hold.
        let bytes: &[u8] = if entry.CredentialBlob.is_null() || entry.CredentialBlobSize == 0 {
            &[]
        } else {
            slice::from_raw_parts(entry.CredentialBlob, entry.CredentialBlobSize as usize)
        };
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();

        let mut length = 0;
        if !entry.UserName.is_null() {
            while *entry.UserName.add(length) != 0 {
                length += 1;
            }
        }
        let username = if length == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(slice::from_raw_parts(entry.UserName, length))
        };
        (username, String::from_utf16_lossy(&units))
    };

    // SAFETY: `credential` came from `CredReadW` and isn't used after this.
    unsafe { CredFree(credential.cast()) };
    Some(found)
}

/// Removes what's saved under `key`. Having nothing there is fine.
pub fn delete(key: &str) {
    let target = target(key);
    // SAFETY: the name is null-terminated.
    unsafe {
        CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per test run, and removed even if the test fails.
    struct TestKey(String);

    impl TestKey {
        fn new(label: &str) -> Self {
            Self(format!("test/{label}/{}", std::process::id()))
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            delete(&self.0);
        }
    }

    #[test]
    fn a_saved_login_can_be_read_back() {
        let key = TestKey::new("roundtrip");

        store(&key.0, "alice", "pässwörd with spaces ✓").unwrap();

        assert_eq!(
            read(&key.0),
            Some(("alice".to_owned(), "pässwörd with spaces ✓".to_owned()))
        );
    }

    #[test]
    fn saving_again_replaces_the_old_login() {
        let key = TestKey::new("replace");

        store(&key.0, "first", "one").unwrap();
        store(&key.0, "second", "two").unwrap();

        assert_eq!(read(&key.0), Some(("second".to_owned(), "two".to_owned())));
    }

    #[test]
    fn deleting_removes_it() {
        let key = TestKey::new("delete");
        store(&key.0, "alice", "secret").unwrap();

        delete(&key.0);

        assert_eq!(read(&key.0), None);
    }

    #[test]
    fn nothing_saved_reads_as_nothing() {
        assert_eq!(read("test/never-saved"), None);
    }

    #[test]
    fn an_empty_password_works() {
        let key = TestKey::new("empty");

        store(&key.0, "alice", "").unwrap();

        assert_eq!(read(&key.0), Some(("alice".to_owned(), String::new())));
    }
}
