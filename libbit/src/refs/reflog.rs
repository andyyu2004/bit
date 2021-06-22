use crate::error::BitGenericError;
use crate::error::BitResult;
use crate::obj::Oid;
use crate::serialize::{Deserialize, Serialize};
use crate::signature::BitSignature;
use std::io::{BufRead, Write};
use std::ops::Deref;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct BitReflogEntry {
    pub old_oid: Oid,
    pub new_oid: Oid,
    pub committer: BitSignature,
    pub message: String,
}

impl Serialize for BitReflogEntry {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        Ok(writeln!(
            writer,
            "{} {} {}\t{}",
            self.old_oid, self.new_oid, self.committer, self.message
        )?)
    }
}

#[derive(Debug, Default)]
pub struct BitReflog {
    entries: Vec<BitReflogEntry>,
}

impl BitReflog {
    pub fn append(&mut self, new_oid: Oid, committer: BitSignature, msg: String) {
        let old_oid = match self.entries.last() {
            Some(entry) => entry.new_oid,
            None => Oid::UNKNOWN,
        };
        self.entries.push(BitReflogEntry { old_oid, new_oid, committer, message: msg })
    }
}

impl Deref for BitReflog {
    type Target = Vec<BitReflogEntry>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl FromStr for BitReflogEntry {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (old_oid, s) = s.split_once(' ').unwrap();
        let (new_oid, s) = s.split_once(' ').unwrap();
        let (committer, message) = s.split_once('\t').unwrap();

        Ok(Self {
            old_oid: old_oid.parse()?,
            new_oid: new_oid.parse()?,
            committer: committer.parse()?,
            message: message.to_owned(),
        })
    }
}

impl FromStr for BitReflog {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let entries = s.lines().map(BitReflogEntry::from_str).collect::<Result<Vec<_>, _>>()?;
        Ok(Self { entries })
    }
}

impl Serialize for BitReflog {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        for entry in &self.entries {
            entry.serialize(writer)?;
        }
        Ok(())
    }
}

impl Deserialize for BitReflog {
    fn deserialize(mut reader: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        let mut s = String::new();
        reader.read_to_string(&mut s)?;
        Self::from_str(&s)
    }
}
