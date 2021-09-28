use crate::error::BitResult;
use crate::io::AsyncHashWriter;
use crate::pack::PACK_SIGNATURE;
use sha1::Sha1;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) struct PackIndexer<R, W> {
    writer: AsyncHashWriter<Sha1, W>,
    reader: R,
}

impl<R, W> PackIndexer<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    pub fn new(reader: R, writer: W) -> Self {
        let writer = AsyncHashWriter::new_sha1(writer);
        Self { reader, writer }
    }

    pub async fn read_pack(&mut self) -> BitResult<()> {
        let pack_size = self.parse_packfile_header().await?;
        todo!()
    }

    async fn parse_packfile_header(&mut self) -> BitResult<u32> {
        let sig = self.reader.read_u32().await?.to_be_bytes();
        ensure_eq!(&sig, PACK_SIGNATURE, "invalid packfile signature");
        let version = self.reader.read_u32().await?;
        ensure_eq!(version, 2, "invalid packfile version `{}`", version);
        Ok(self.reader.read_u32().await?)
    }

    pub async fn commit(&mut self) -> BitResult<()> {
        // rename the tmp file etc
        todo!()
    }
}
