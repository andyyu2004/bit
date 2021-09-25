use super::*;
use crate::path;
use crate::upload_pack::UploadPack;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread::JoinHandle;
use tokio::io::{self, AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

pub struct FileTransport<'rcx> {
    repo: BitRepo<'rcx>,
    handle: JoinHandle<BitResult<()>>,
    client: DuplexStream,
}

impl<'rcx> FileTransport<'rcx> {
    pub async fn new(repo: BitRepo<'rcx>, remote: Remote) -> BitResult<FileTransport<'rcx>> {
        let (client, server) = tokio::io::duplex(64);
        let (server_read, server_write) = tokio::io::split(server);
        // doing a preemptive `find` on the current thread just to check the repo exists
        let path = path::normalize(&repo.to_absolute_path(remote.url.path));
        BitRepo::find(&path, |_| Ok(()))?;
        let handle = std::thread::spawn(move || {
            BitRepo::find(path, |repo| UploadPack::new(repo, server_read, server_write).run())
        });
        Ok(Self { repo, handle, client })
    }
}

#[async_trait]
impl Transport for FileTransport<'_> {
}

impl AsyncRead for FileTransport<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.client).poll_read(cx, buf)
    }
}

impl AsyncWrite for FileTransport<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.client).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.client).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.client).poll_shutdown(cx)
    }
}
