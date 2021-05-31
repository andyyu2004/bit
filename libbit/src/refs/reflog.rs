use crate::error::BitGenericError;
use crate::obj::Oid;
use crate::signature::BitSignature;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct BitReflogEntry {
    pub old_oid: Oid,
    pub new_oid: Oid,
    pub committer: BitSignature,
    pub msg: String,
}

#[derive(Debug)]
pub struct BitReflog {
    entries: Vec<BitReflogEntry>,
}

impl BitReflog {
    pub fn append(&mut self, new_oid: Oid, committer: BitSignature, msg: String) {
        let old_oid = match self.entries.last() {
            Some(entry) => entry.new_oid,
            None => Oid::UNKNOWN,
        };
        self.entries.push(BitReflogEntry { old_oid, new_oid, committer, msg })
    }
}

impl FromStr for BitReflogEntry {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (old_oid, s) = s.split_once(' ').unwrap();
        let (new_oid, s) = s.split_once(' ').unwrap();
        let (committer, msg) = s.split_once('\t').unwrap();
        Ok(Self {
            old_oid: old_oid.parse()?,
            new_oid: new_oid.parse()?,
            committer: committer.parse()?,
            msg: msg.to_owned(),
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
