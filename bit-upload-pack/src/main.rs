use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::upload_pack::UploadPack;
use std::path::PathBuf;

#[derive(Clap, Debug)]
struct Opts {
    path: PathBuf,
}

fn main() -> BitResult<()> {
    let opts = Opts::parse();
    BitRepo::find(&opts.path, |repo| {
        UploadPack::new(repo, tokio::io::stdin(), tokio::io::stdout()).run()
    })
}
