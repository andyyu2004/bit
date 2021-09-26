use crate::error::BitResult;
use crate::obj::Oid;
use async_trait::async_trait;
use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[async_trait]
pub trait BitProtocolRead: AsyncRead + Unpin + Send {
    async fn recv_packet(&mut self) -> BitResult<Vec<u8>> {
        let mut buf = [0; 4];
        assert_eq!(self.read_exact(&mut buf).await?, 4);
        let n = usize::from_str_radix(std::str::from_utf8(&buf)?, 16)?;
        if n == 0 {
            // recv flush packet
            return Ok(vec![]);
        }
        let mut contents = vec![0; n - 4];
        assert_eq!(self.read_exact(&mut contents).await?, n - 4);
        Ok(contents)
    }

    /// Receive a message which is a collection of packets deliminated by a flush packet.
    async fn recv_message(&mut self) -> BitResult<Vec<Vec<u8>>> {
        let mut packets = vec![];
        loop {
            let packet = self.recv_packet().await?;
            if packet.is_empty() || packet == b"done" {
                break Ok(packets);
            }
            packets.push(packet);
        }
    }
}

impl<R: AsyncRead + Unpin + Send> BitProtocolRead for R {
}

#[async_trait]
pub trait BitProtocolWrite: AsyncWrite + Unpin + Send {
    async fn write_packet(&mut self, bytes: &[u8]) -> io::Result<()> {
        assert!(bytes.len() < u16::MAX as usize);
        let length = format!("{:04x}", 4 + bytes.len());
        debug_assert_eq!(length.len(), 4);
        self.write_all(&length.as_bytes()).await?;
        self.write_all(bytes).await?;
        Ok(())
    }

    #[inline]
    async fn write_flush_packet(&mut self) -> io::Result<()> {
        self.write_all(b"0000").await?;
        self.flush().await
    }

    async fn want(&mut self, oid: Oid) -> io::Result<()> {
        self.write_packet(format!("want {}\n", oid).as_bytes()).await
    }

    async fn have(&mut self, oid: Oid) -> io::Result<()> {
        self.write_packet(format!("have {}\n", oid).as_bytes()).await
    }
}

impl<W: AsyncWrite + Unpin + Send> BitProtocolWrite for W {
}
