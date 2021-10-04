use super::*;
use crate::path;
use git_url_parse::GitUrl;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncBufRead, AsyncRead, AsyncWrite, BufReader, ReadBuf};
use tokio::process::{ChildStdin, ChildStdout, Command};

pin_project! {
    pub struct FileTransport {
        #[pin]
        stdin: ChildStdin,
        #[pin]
        stdout: BufReader<ChildStdout>,
    }
}

impl FileTransport {
    pub async fn new(repo: BitRepo<'_>, url: &GitUrl) -> BitResult<Self> {
        let path = path::normalize(&repo.to_absolute_path(&url.path));
        let mut child = Command::new("git-upload-pack")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());

        Ok(Self { stdin, stdout })
    }
}

#[async_trait]
impl ProtocolTransport for FileTransport {
}

impl AsyncBufRead for FileTransport {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        self.project().stdout.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().stdout.consume(amt)
    }
}

impl AsyncRead for FileTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().stdout.poll_read(cx, buf)
    }
}

impl AsyncWrite for FileTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().stdin.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().stdin.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().stdin.poll_shutdown(cx)
    }
}
