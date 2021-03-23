use crate::error::{BitError, BitResult};
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

impl BitRepo {
    pub fn user_signature(&self) -> BitResult<BitSignature> {
        self.with_local_config(|config| {
            let name = config.name()?;
            let email = config.email()?;
            if let (Some(name), Some(email)) = (name, email) {
                Ok(BitSignature {
                    name: name.to_owned(),
                    email: email.to_owned(),
                    time: BitTime::now(),
                })
            } else {
                // this is too dumb to tell if only one of the entries is missing but whatever
                Err(anyhow!("{}", MISSING_IDENTITY_MSG))
            }
        })
    }
}

#[derive(PartialEq, Clone, Debug, Hash, Ord, PartialOrd, Eq, Copy)]
pub struct BitEpochTime(i64);

#[derive(PartialEq, Clone, Debug, Hash, Ord, PartialOrd, Eq, Copy)]
/// timezone offset in minutes
pub struct BitTimeZoneOffset(i32);

#[derive(PartialEq, Clone, Debug, PartialOrd, Eq, Ord, Hash)]
pub struct BitTime {
    time: BitEpochTime,
    offset: BitTimeZoneOffset,
}

impl BitTime {
    pub fn now() -> Self {
        let now = chrono::offset::Local::now();
        let offset = BitTimeZoneOffset(now.offset().local_minus_utc() / 60);
        let time = BitEpochTime(now.timestamp());
        Self { time, offset }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct BitSignature {
    name: String,
    email: String,
    time: BitTime,
}

impl FromStr for BitTimeZoneOffset {
    type Err = BitError;

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
    type Err = BitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse().unwrap()))
    }
}

impl FromStr for BitTime {
    type Err = BitError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut splits = s.split_ascii_whitespace();
        let time = splits.next().unwrap().parse()?;
        let offset = splits.next().unwrap().parse()?;
        Ok(Self { time, offset })
    }
}

impl FromStr for BitSignature {
    type Err = BitError;

    // Andy Yu <andyyu2004@gmail.com> 1616061862 +1300
    fn from_str(s: &str) -> BitResult<Self> {
        // assumes no < or > in name
        let email_start_idx = s.find("<").unwrap();
        let email_end_idx = s.find(">").unwrap();

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
mod tests {
    use super::*;
    use crate::test_utils::generate_sane_string;
    use quickcheck::{quickcheck, Arbitrary, Gen};
    use rand::Rng;

    impl Arbitrary for BitSignature {
        fn arbitrary(_g: &mut Gen) -> Self {
            Self {
                name: generate_sane_string(5..100),
                email: generate_sane_string(10..200),
                // don't care too much about this being random
                time: BitTime { time: BitEpochTime(12345678), offset: BitTimeZoneOffset(200) },
            }
        }
    }

    #[test]
    fn parse_timezone_offset() {
        let offset = BitTimeZoneOffset::from_str("+0200").unwrap();
        assert_eq!(offset.0, 120);
        let offset = BitTimeZoneOffset::from_str("+1300").unwrap();
        assert_eq!(offset.0, 780);
        let offset = BitTimeZoneOffset::from_str("-0830").unwrap();
        assert_eq!(offset.0, -510);
    }

    impl Arbitrary for BitTimeZoneOffset {
        fn arbitrary(_g: &mut quickcheck::Gen) -> Self {
            // how to bound quickchecks genrange?
            Self(rand::thread_rng().gen_range(-1000..1000))
        }
    }

    #[quickcheck(sizee)]
    fn serialize_then_parse_timezone(offset: BitTimeZoneOffset) {
        let parsed: BitTimeZoneOffset = offset.to_string().parse().unwrap();
        assert_eq!(offset, parsed)
    }

    #[test]
    fn parse_bit_signature() {
        let sig = "Andy Yu <andyyu2004@gmail.com> 1616061862 +1300";
        let sig = sig.parse::<BitSignature>().unwrap();
        assert_eq!(sig.name, "Andy Yu");
        assert_eq!(sig.email, "andyyu2004@gmail.com");
        assert_eq!(
            sig.time,
            BitTime { time: BitEpochTime(1616061862), offset: BitTimeZoneOffset(780) }
        );
    }

    #[quickcheck]
    fn serialize_then_parse_bit_signature(sig: BitSignature) {
        assert_eq!(sig, sig.to_string().parse().unwrap())
    }

    #[test]
    fn serialize_timezone_offset() {
        let offset = BitTimeZoneOffset(780);
        assert_eq!(format!("{}", offset), "+1300");
        let offset = BitTimeZoneOffset(-200);
        assert_eq!(format!("{}", offset), "-0320");
    }
}
