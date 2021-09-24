use anyhow::Result;
use clap::Clap;
use libbit::refs::SymbolicRef;
use libbit::repo::BitRepo;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt, Stdin, Stdout};

#[derive(Clap, Debug)]
struct Opts {
    path: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    BitRepo::find(opts.path.clone(), |repo| {
        UploadPack { repo, opts, stdin: tokio::io::stdin(), stdout: tokio::io::stdout() }.run()
    })
}

struct UploadPack<'rcx> {
    repo: BitRepo<'rcx>,
    opts: Opts,
    stdin: Stdin,
    stdout: Stdout,
}

const CAPABILITIES: &[&str] = &[
    "multi_ack",
    "thin-pack",
    "side-band",
    "side-band-64k",
    "ofs-delta",
    "shallow",
    "deepen-since",
    "deepen-not",
    "deepen-relative",
    "no-progress",
    "include-tag",
    "multi_ack_detailed",
    "symref=HEAD:refs/heads/remote",
    "object-format=sha1",
    "agent=bit",
];

impl<'rcx> UploadPack<'rcx> {
    #[tokio::main]
    async fn run(&mut self) -> Result<()> {
        self.write_ref_discovery().await?;
        let bytes = self.recv().await?;
        if bytes.is_empty() {
            return Ok(());
        }
        Ok(())
    }

    // Reference Discovery
    // -------------------
    //
    // When the client initially connects the server will immediately respond
    // with a version number (if "version=1" is sent as an Extra Parameter),
    // and a listing of each reference it has (all branches and tags) along
    // with the object name that each reference currently points to.
    //
    //    $ echo -e -n "0045git-upload-pack /schacon/gitbook.git\0host=example.com\0\0version=1\0" |
    //       nc -v example.com 9418
    //    000eversion 1
    //    00887217a7c7e582c46cec22a130adf4b9d7d950fba0 HEAD\0multi_ack thin-pack
    // 		side-band side-band-64k ofs-delta shallow no-progress include-tag
    //    00441d3fcd5ced445d1abc402225c0b8a1299641f497 refs/heads/integration
    //    003f7217a7c7e582c46cec22a130adf4b9d7d950fba0 refs/heads/master
    //    003cb88d2441cac0977faf98efc80305012112238d9d refs/tags/v0.9
    //    003c525128480b96c89e6418b1e40909bf6c5b2d580f refs/tags/v1.0
    //    003fe92df48743b7bc7d26bcaabfddde0a1e20cae47c refs/tags/v1.0^{}
    //    0000
    //
    // The returned response is a pkt-line stream describing each ref and
    // its current value.  The stream MUST be sorted by name according to
    // the C locale ordering.
    //
    // If HEAD is a valid ref, HEAD MUST appear as the first advertised
    // ref.  If HEAD is not a valid ref, HEAD MUST NOT appear in the
    // advertisement list at all, but other refs may still appear.
    //
    // The stream MUST include capability declarations behind a NUL on the
    // first ref. The peeled value of a ref (that is "ref^{}") MUST be
    // immediately after the ref itself, if presented. A conforming server
    // MUST peel the ref if it's an annotated tag.
    async fn write_ref_discovery(&mut self) -> Result<()> {
        let repo = self.repo;
        let mut refs = repo.ls_refs()?.into_iter().collect::<Vec<_>>();
        // The order isn't really significant but keeping it close to git
        // The ord impl for refs is tailored for other purposes (i.e. remotes before heads in bit log)
        refs.sort_by_key(|r| r.path());
        for (i, r) in refs.into_iter().enumerate() {
            let oid = repo.fully_resolve_ref(r)?;
            if i == 0 {
                assert_eq!(r, SymbolicRef::HEAD);
                self.write(format!("{} {}\0{}\n", oid, r.path(), CAPABILITIES.join(" "))).await?;
                continue;
            }
            self.write(format!("{} {}\n", oid, r.path()).as_bytes()).await?;
        }
        self.write_flush().await
    }

    #[inline]
    async fn write_flush(&mut self) -> Result<()> {
        self.stdout.write_all(b"0000").await?;
        Ok(self.stdout.flush().await?)
    }

    #[inline]
    async fn write(&mut self, bytes: impl AsRef<[u8]>) -> Result<()> {
        self.write_internal(bytes.as_ref()).await
    }

    async fn write_internal(&mut self, bytes: &[u8]) -> Result<()> {
        assert!(bytes.len() < u16::MAX as usize);
        let length = format!("{:04x}", 4 + bytes.len());
        debug_assert_eq!(length.len(), 4);
        self.stdout.write_all(&length.as_bytes()).await?;
        self.stdout.write_all(bytes).await?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Vec<u8>> {
        let mut buf = [0; 4];
        assert_eq!(self.stdin.read_exact(&mut buf).await?, buf.len());
        let n = usize::from_str_radix(std::str::from_utf8(&buf)?, 16)?;
        if n == 0 {
            // recv flush packet
            return Ok(vec![]);
        }
        let mut contents = Vec::with_capacity(n);
        assert_eq!(self.stdin.read_exact(&mut contents).await?, contents.len());
        Ok(contents)
    }
}

// https://github.com/git/git/blob/master/Documentation/technical/protocol-common.txt
struct PacketLine {
    length: u16,
}
