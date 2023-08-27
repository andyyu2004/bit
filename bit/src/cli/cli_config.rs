use clap::Parser;
use lazy_static::lazy_static;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use regex::Regex;

lazy_static! {
    static ref REGEX: Regex = Regex::new("^[A-Za-z]+\\.[A-Za-z]+$").unwrap();
}

fn validate_name(name: &str) -> Result<String, String> {
    if REGEX.is_match(name) {
        Ok(name.to_owned())
    } else {
        Err("invalid value for name".to_owned())
    }
}

#[derive(Parser, Debug)]
pub struct BitConfigCliOpts {
    #[arg(long = "global", conflicts_with = "local")]
    global: bool,
    #[arg(long = "local")]
    local: bool,
    #[arg(value_parser = validate_name)]
    name: String,
    value: Option<String>,
}

impl BitConfigCliOpts {
    pub fn execute(&self, repo: BitRepo) -> BitResult<()> {
        // if its not global we assume its local even if self.local is not explicitly set
        let (section, key) = self.name.split_once('.').unwrap();
        repo.with_raw_local_config(|config| {
            match &self.value {
                Some(value) => config.set(section, key, value),
                // git just prints nothing if `section.value` does not exist
                None => {
                    if let Some(value) = config.get::<String>(section, key)? {
                        println!("{value}")
                    };
                    Ok(())
                }
            }
        })
    }
}
