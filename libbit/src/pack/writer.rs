use crate::error::BitResult;
use crate::repo::BitRepo;
use pin_project_lite::pin_project;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::File;
use tokio::io::{AsyncWrite, BufWriter};

pin_project! {
    /// This struct literally just writes the data given to it to a file in .git/objects/pack.
    /// No validation or anything is performed here.
    pub(crate) struct PackWriter {
        pub path: PathBuf,
        #[pin]
        file: BufWriter< File>,
    }
}

impl PackWriter {
    pub async fn new(repo: BitRepo<'_>) -> BitResult<Self> {
        let mut path =
            tempfile::NamedTempFile::new_in(repo.pack_objects_dir())?.into_temp_path().keep()?;
        path.set_extension("pack");
        let file = BufWriter::new(File::create(&path).await?);
        Ok(Self { file, path })
    }
}

impl AsyncWrite for PackWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        self.project().file.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        self.project().file.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().file.poll_shutdown(cx)
    }
}
