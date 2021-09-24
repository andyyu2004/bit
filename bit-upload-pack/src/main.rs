use anyhow::Result;
use clap::Clap;
use libbit::repo::BitRepo;
use libbit_upload_pack::UploadPack;
use std::path::PathBuf;

#[derive(Clap, Debug)]
struct Opts {
    path: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    BitRepo::find(&opts.path, |repo| {
        UploadPack::new(repo, tokio::io::stdin(), tokio::io::stdout()).run()
    })
}
