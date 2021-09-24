use super::*;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};

pub struct FileTransport {
    handle: Child,
    stdin: ChildStdin,
}

impl FileTransport {
    pub fn new(remote: &Remote) -> BitResult<Self> {
        let mut handle =
            Command::new("git-upload-pack").arg(&remote.url.path).stdin(Stdio::piped()).spawn()?;
        let stdin = handle.stdin.take().unwrap();
        Ok(Self { handle, stdin })
    }
}

#[async_trait]
impl Transport for FileTransport {
    async fn send(&mut self, bytes: &[u8]) -> BitResult<()> {
        Ok(self.stdin.write_all(bytes).await?)
    }
}
