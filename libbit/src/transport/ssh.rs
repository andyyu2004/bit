use super::*;
use crate::repo::BitRepo;
use git_url_parse::GitUrl;
use openssh::{RemoteChild, Session};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::process::Stdio;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncBufRead, AsyncRead, AsyncWrite, BufReader, ReadBuf};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};

pin_project! {
    pub struct SshTransport<'s> {
        repo: BitRepo,
        child: RemoteChild<'s>,
        stdin: ChildStdin,
        #[pin]
        stdout: BufReader<ChildStdout>,
        stderr: BufReader<ChildStderr>,
    }
}

impl<'s> SshTransport<'s> {
    pub async fn new(
        repo: BitRepo,
        session: &'s Session,
        url: &GitUrl,
    ) -> BitResult<SshTransport<'s>> {
        let mut child = session
            .command("git-upload-pack")
            .arg(&url.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin().take().unwrap();
        let stdout = BufReader::new(child.stdout().take().unwrap());
        let stderr = BufReader::new(child.stderr().take().unwrap());
        Ok(Self { repo, child, stdin, stdout, stderr })
    }
}

#[async_trait]
impl ProtocolTransport for SshTransport<'_> {
}

impl AsyncBufRead for SshTransport<'_> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        self.project().stdout.poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.project().stdout.consume(amt)
    }
}

impl AsyncRead for SshTransport<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl AsyncWrite for SshTransport<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}
