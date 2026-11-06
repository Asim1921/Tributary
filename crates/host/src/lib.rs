//! The message format shared by the browser host and the app.
//!
//! Browsers frame native messages as a 4-byte length followed by JSON. The pipe between
//! the host and the app uses the same framing, so a message passes through unchanged.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest message accepted. A page with thousands of links stays well under this.
pub const MAX_MESSAGE_BYTES: usize = 8 << 20;

/// Lets a test use its own pipe, beside a real copy of the app.
const PIPE_VARIABLE: &str = "TRIBUTARY_PIPE";

/// The pipe the app listens on. Each Windows user gets their own.
pub fn pipe_name() -> String {
    if let Ok(name) = std::env::var(PIPE_VARIABLE) {
        return name;
    }

    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    format!(r"\\.\pipe\tributary-capture-{user}")
}

/// Reads one message. `None` means the other side closed cleanly.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }

    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the message is too large",
        ));
    }

    let mut message = vec![0u8; length];
    reader.read_exact(&mut message).await?;
    Ok(Some(message))
}

pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, message: &[u8]) -> io::Result<()> {
    let length = u32::try_from(message.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "the message is too large"))?;
    writer.write_all(&length.to_le_bytes()).await?;
    writer.write_all(message).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    async fn framed(message: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, message).await.unwrap();
        bytes
    }

    #[tokio::test]
    async fn a_message_survives_the_round_trip() {
        let bytes = framed(b"{\"type\":\"ping\"}").await;
        let mut reader = Cursor::new(bytes);

        let message = read_frame(&mut reader).await.unwrap();

        assert_eq!(message.as_deref(), Some(b"{\"type\":\"ping\"}".as_slice()));
    }

    #[tokio::test]
    async fn the_length_comes_first_in_little_endian() {
        assert_eq!(framed(b"abc").await[..4], [3, 0, 0, 0]);
    }

    #[tokio::test]
    async fn several_messages_stay_separate() {
        let mut bytes = framed(b"one").await;
        bytes.extend(framed(b"two").await);
        let mut reader = Cursor::new(bytes);

        assert_eq!(read_frame(&mut reader).await.unwrap().unwrap(), b"one");
        assert_eq!(read_frame(&mut reader).await.unwrap().unwrap(), b"two");
        assert_eq!(read_frame(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_other_side_closing_is_not_an_error() {
        let mut reader = Cursor::new(Vec::new());
        assert_eq!(read_frame(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_oversized_message_is_refused_before_allocating() {
        let bytes = u32::MAX.to_le_bytes().to_vec();
        let mut reader = Cursor::new(bytes);

        let error = read_frame(&mut reader).await.unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn a_message_cut_short_is_an_error() {
        let mut bytes = 10u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"abc");
        let mut reader = Cursor::new(bytes);

        assert!(read_frame(&mut reader).await.is_err());
    }

    #[test]
    fn each_user_gets_their_own_pipe() {
        assert!(pipe_name().starts_with(r"\\.\pipe\tributary-capture-"));
    }
}
