use clap::Parser;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::upload_pack::UploadPack;
use std::path::PathBuf;
use tokio::io::BufReader;

#[derive(Parser, Debug)]
struct Opts {
    path: PathBuf,
}

fn main() -> BitResult<()> {
    let opts = Opts::parse();
    BitRepo::find(&opts.path, |repo| {
        UploadPack::new(repo, BufReader::new(tokio::io::stdin()), tokio::io::stdout()).run()
    })
}
