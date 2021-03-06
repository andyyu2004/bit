mod cli;

pub fn main() {
    if let Err(err) = cli::run() {
        eprintln!("{:?}", err)
    }
}

