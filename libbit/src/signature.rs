use crate::error::{BitGenericError, BitResult};
use crate::repo::BitRepo;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

const MISSING_IDENTITY_MSG: &str = r#"Author identity unknown

*** Please tell me who you are.

Run

  bit config --global user.email "you@example.com"
  bit config --global user.name "Your Name"

to set your account's default identity.
Omit --global to set the identity only in this repository."#;

impl<'r> BitRepo<'r> {
    pub fn user_signature(&self) -> BitResult<BitSignature> {
        let name = self.config().name()?;
        let email = self.config().email()?;
        if let (Some(name), Some(email)) = (name, email) {
            Ok(BitSignature { name, email, time: BitTime::now() })
        } else {
            // this is too dumb to tell if only one of the entries is missing but whatever
            Err(anyhow!("{}", MISSING_IDENTITY_MSG))
        }
    }
}

#[derive(PartialEq, Clone, Debug, Hash, Ord, PartialOrd, Eq, Copy)]
pub struct BitEpochTime(i64);

impl BitEpochTime {
    pub fn new(i: i64) -> Self {
        Self(i)
    }
}

#[derive(PartialEq, Clone, Debug, Hash, Ord, PartialOrd, Eq, Copy)]
/// timezone offset in minutes
pub struct BitTimeZoneOffset(i32);

impl BitTimeZoneOffset {
    pub fn new(offset: i32) -> Self {
        Self(offset)
    }
}

#[derive(PartialEq, Clone, Debug, PartialOrd, Eq, Ord, Hash)]
pub struct BitTime {
    pub(crate) time: BitEpochTime,
    pub(crate) offset: BitTimeZoneOffset,
}

impl BitTime {
    pub fn now() -> Self {
        // for testing we always have some fixed time so each run is deterministic
        // (commit oid depends on time which makes comparing oids impossible)
        if cfg!(test) {
            Self { time: BitEpochTime(0), offset: BitTimeZoneOffset(0) }
        } else {
            let now = chrono::offset::Local::now();
            let offset = BitTimeZoneOffset(now.offset().local_minus_utc() / 60);
            let time = BitEpochTime(now.timestamp());
            Self { time, offset }
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct BitSignature {
    pub name: String,
    pub email: String,
    pub time: BitTime,
}

impl FromStr for BitTimeZoneOffset {
    type Err = BitGenericError;

    // format: (+|-)0200
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let sign = match &s[0..1] {
            "+" => 1,
            "-" => -1,
            _ => panic!("invalid timezone format {}", s),
        };
        let hours: i32 = s[1..3].parse().unwrap();
        let minutes: i32 = s[3..5].parse().unwrap();
        let offset = sign * (minutes + hours * 60);
        Ok(Self(offset))
    }
}

impl FromStr for BitEpochTime {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse().unwrap()))
    }
}

impl FromStr for BitTime {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut splits = s.split_ascii_whitespace();
        let time = splits.next().unwrap().parse()?;
        let offset = splits.next().unwrap().parse()?;
        Ok(Self { time, offset })
    }
}

impl FromStr for BitSignature {
    type Err = BitGenericError;

    // Andy Yu <andyyu2004@gmail.com> 1616061862 +1300
    fn from_str(s: &str) -> BitResult<Self> {
        // assumes no < or > in name
        let email_start_idx = s.find('<').unwrap();
        let email_end_idx = s.find('>').unwrap();

        let name = s[..email_start_idx - 1].to_owned();
        let email = s[email_start_idx + 1..email_end_idx].to_owned();
        let time = s[email_end_idx + 1..].parse()?;
        Ok(Self { name, email, time })
    }
}

impl Display for BitEpochTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for BitTimeZoneOffset {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let sign = if self.0 >= 0 { '+' } else { '-' };
        let offset = self.0.abs();
        let hours = offset / 60;
        let minutes = offset % 60;
        write!(f, "{}{:02}{:02}", sign, hours, minutes)?;
        Ok(())
    }
}

impl Display for BitTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.time, self.offset)
    }
}

impl Display for BitSignature {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} <{}> {}", self.name, self.email, self.time)
    }
}

#[cfg(test)]
mod tests;
