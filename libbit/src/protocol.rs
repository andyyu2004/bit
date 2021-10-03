use crate::error::BitResult;
use crate::obj::Oid;
use crate::pack::{PackIndexer, PackWriter, PACK_EXT, PACK_IDX_EXT};
use crate::refs::SymbolicRef;
use crate::repo::BitRepo;
use async_trait::async_trait;
use parse_display::{Display, FromStr};
use std::collections::HashSet;
use tokio::io::{self, AsyncBufRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub type Capabilities = HashSet<Capability>;

const SIDEBAND_DATA: u8 = 1;
const SIDEBAND_PROGRESS: u8 = 2;
const SIDEBAND_ERROR: u8 = 3;

#[derive(Debug, Display, FromStr, Hash, PartialEq, Eq)]
#[display(style = "kebab-case")]
pub enum Capability {
    #[display(style = "snake_case")]
    MultiAck,
    #[display(style = "snake_case")]
    MultiAckDetailed,
    #[display("agent={0}")]
    Agent(String),
    #[display("symref={0}:{1}")]
    Symref(SymbolicRef, SymbolicRef),
    #[display("side-band-64k")]
    SideBand64k,
    #[display("object-format={0}")]
    ObjectFormat(String),
    SideBand,
    ThinPack,
    OfsDelta,
    Shallow,
    DeepenSince,
    DeepenNot,
    DeepenRelative,
    NoProgress,
    IncludeTag,
    AllowTipSha1InWant,
    AllowReachableSha1InWant,
    Filter,
}

// 0103f1b89a201e9329e6df48f8d6cf320781570c936a HEADmulti_ack thin-pack side-band side-band-64k ofs-delta shallow deepen-since deepen-not deepen-relative no-progress include-tag multi_ack_detailed symref=HEAD:refs/heads/main object-format=sha
#[async_trait]
pub trait BitProtocolRead: AsyncBufRead + Unpin + Send {
    async fn recv_packet(&mut self) -> BitResult<Vec<u8>> {
        let mut length = [0; 4];
        assert_eq!(self.read_exact(&mut length).await?, 4);
        self.read_contents(length).await
    }

    async fn read_contents(&mut self, length: [u8; 4]) -> BitResult<Vec<u8>> {
        let n = usize::from_str_radix(std::str::from_utf8(&length)?, 16)?;
        self.read_contents_with_parsed_len(n).await
    }

    async fn read_contents_with_parsed_len(&mut self, n: usize) -> BitResult<Vec<u8>> {
        if n == 0 {
            // recv flush packet
            return Ok(vec![]);
        }
        let mut contents = vec![0; n - 4];
        assert_eq!(self.read_exact(&mut contents).await?, n - 4);
        Ok(contents)
    }

    /// Assumes `side-band-64k` capability
    async fn recv_pack(&mut self, repo: BitRepo<'_>) -> BitResult<()> {
        let mut writer = PackWriter::new(repo).await?;
        let mut first_data_packet = true;
        loop {
            let mut length = [0; 4];
            assert_eq!(self.read_exact(&mut length).await?, 4);
            let n = usize::from_str_radix(std::str::from_utf8(&length[..4])?, 16)?;
            if n == 0 {
                break;
            }
            let mut sideband = 0;
            assert_eq!(self.read_exact(std::slice::from_mut(&mut sideband)).await?, 1);
            let data = self.read_contents_with_parsed_len(n - 1).await?;
            match sideband {
                SIDEBAND_DATA => {
                    if first_data_packet {
                        assert_eq!(b"PACK", &data[0..4]);
                        first_data_packet = false;
                    }
                    writer.write_all(&data).await?
                }
                SIDEBAND_PROGRESS => eprint!("{}", std::str::from_utf8(&data)?),
                SIDEBAND_ERROR => todo!(),
                _ => bail!("invalid sideband byte `{:x}`", sideband),
            }
        }
        writer.flush().await?;

        let pack_index = PackIndexer::write_pack_index(&writer.path, Default::default())?;
        std::fs::rename(
            &writer.path,
            writer.path.with_file_name(format!("pack-{}.{}", pack_index.pack_hash, PACK_EXT)),
        )?;
        std::fs::rename(
            &writer.path.with_extension(PACK_IDX_EXT),
            writer.path.with_file_name(format!("pack-{}.{}", pack_index.pack_hash, PACK_IDX_EXT)),
        )?;
        repo.refresh_odb()?;
        Ok(())
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

impl<R: AsyncBufRead + Unpin + Send> BitProtocolRead for R {
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

    async fn done(&mut self) -> io::Result<()> {
        self.write_packet(b"done").await
    }

    async fn have(&mut self, oid: Oid) -> io::Result<()> {
        self.write_packet(format!("have {}\n", oid).as_bytes()).await
    }
}

impl<W: AsyncWrite + Unpin + Send> BitProtocolWrite for W {
}
