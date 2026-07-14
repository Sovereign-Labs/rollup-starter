use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::IpcError;
use crate::types::OracleFrame;

pub const MAX_FRAME_LEN: u32 = 256 * 1024;

pub async fn write_frame<W>(writer: &mut W, frame: &OracleFrame) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
{
    let body = borsh::to_vec(frame).map_err(IpcError::Codec)?;
    let len = u32::try_from(body.len()).map_err(|_| IpcError::FrameTooLarge(body.len()))?;
    if len > MAX_FRAME_LEN {
        return Err(IpcError::FrameTooLarge(body.len()));
    }
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&body).await?;
    Ok(())
}

pub async fn read_frame<R>(reader: &mut R) -> Result<OracleFrame, IpcError>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(IpcError::Closed),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME_LEN {
        return Err(IpcError::FrameTooLarge(len as usize));
    }
    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body).await?;
    borsh::from_slice(&body).map_err(IpcError::Codec)
}

pub async fn read_frame_with_timeout<R>(
    reader: &mut R,
    timeout: Duration,
) -> Result<OracleFrame, IpcError>
where
    R: AsyncRead + Unpin,
{
    tokio::time::timeout(timeout, read_frame(reader))
        .await
        .map_err(|_| IpcError::ReadTimeout)?
}

pub async fn write_frame_with_timeout<W>(
    writer: &mut W,
    frame: &OracleFrame,
    timeout: Duration,
) -> Result<(), IpcError>
where
    W: AsyncWrite + Unpin,
{
    tokio::time::timeout(timeout, write_frame(writer, frame))
        .await
        .map_err(|_| IpcError::WriteTimeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;

    fn sample_update() -> OracleFrame {
        OracleFrame::PriceUpdate {
            feed_id: B256::repeat_byte(0xcd),
            payload: vec![0x12, 0x34, 0x56, 0x78],
            source_time_ms: 0x1234_5670,
        }
    }

    #[tokio::test]
    async fn roundtrips_a_frame_through_a_pipe() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let frame = sample_update();
        write_frame(&mut a, &frame).await.unwrap();
        let got = read_frame(&mut b).await.unwrap();
        assert_eq!(got, frame);
    }

    #[tokio::test]
    async fn roundtrips_multiple_frames_in_order() {
        let (mut a, mut b) = tokio::io::duplex(4096);
        let hello = OracleFrame::Hello {
            protocol_version: crate::PROTOCOL_VERSION,
            provider_id: B256::repeat_byte(0x01),
            feeds: vec![B256::repeat_byte(0x02), B256::repeat_byte(0x03)],
        };
        write_frame(&mut a, &hello).await.unwrap();
        write_frame(&mut a, &sample_update()).await.unwrap();
        assert_eq!(read_frame(&mut b).await.unwrap(), hello);
        assert_eq!(read_frame(&mut b).await.unwrap(), sample_update());
    }

    #[tokio::test]
    async fn clean_eof_reports_closed() {
        let (a, mut b) = tokio::io::duplex(64);
        drop(a);
        assert!(matches!(read_frame(&mut b).await, Err(IpcError::Closed)));
    }

    #[tokio::test]
    async fn oversized_length_is_rejected() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&(MAX_FRAME_LEN + 1).to_le_bytes())
            .await
            .unwrap();
        assert!(matches!(
            read_frame(&mut b).await,
            Err(IpcError::FrameTooLarge(_))
        ));
    }

    #[tokio::test]
    async fn read_with_timeout_elapses_on_idle_stream() {
        let (_a, mut b) = tokio::io::duplex(64);
        let result = read_frame_with_timeout(&mut b, Duration::from_millis(20)).await;
        assert!(matches!(result, Err(IpcError::ReadTimeout)));
    }

    #[tokio::test]
    async fn read_with_timeout_returns_frame_when_in_time() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write_frame(&mut a, &sample_update()).await.unwrap();
        let got = read_frame_with_timeout(&mut b, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(got, sample_update());
    }

    #[tokio::test]
    async fn write_with_timeout_elapses_when_peer_never_reads() {
        let (_a, mut b) = tokio::io::duplex(16);
        let result =
            write_frame_with_timeout(&mut b, &sample_update(), Duration::from_millis(20)).await;
        assert!(matches!(result, Err(IpcError::WriteTimeout)));
    }

    #[tokio::test]
    async fn invalid_frame_body_is_codec_error() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&1u32.to_le_bytes()).await.unwrap();
        a.write_all(&[0xff]).await.unwrap();
        assert!(matches!(read_frame(&mut b).await, Err(IpcError::Codec(_))));
    }
}
