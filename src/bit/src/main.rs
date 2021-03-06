mod cli;

pub fn main() -> libbit::error::BitResult<()> {
    if let Err(err) = cli::run() {
        eprintln!("{:?}", err)
    }

    Ok(())
}

