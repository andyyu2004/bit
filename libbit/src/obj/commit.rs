use crate::error::BitResult;
use crate::obj::{BitObj, BitObjType, Oid};
use crate::repo::BitRepo;
use crate::serialize::{DeserializeSized, Serialize};
use crate::signature::BitSignature;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::process::Command;

#[derive(PartialEq, Clone, Debug)]
pub struct Commit {
    pub(crate) tree: Oid,
    pub(crate) author: BitSignature,
    pub(crate) committer: BitSignature,
    pub(crate) message: String,
    pub(crate) parent: Option<Oid>,
    pub(crate) gpgsig: Option<String>,
}

impl Commit {
    /// Get a reference to the commit's tree.
    pub fn tree(&self) -> Oid {
        self.tree
    }
}

impl BitRepo {
    pub fn mk_commit(&self, tree: Oid, message: String, parent: Option<Oid>) -> BitResult<Commit> {
        // TODO validate hashes of parent and tree
        let author = self.user_signature()?;
        let committer = author.clone();
        Ok(Commit { tree, parent, message, author, committer, gpgsig: None })
    }

    pub fn read_commit_msg(&self) -> BitResult<String> {
        let editor = std::env::var("EDITOR").expect("$EDITOR variable is not set");
        let template = r#"
# Please; enter the commit message for your changes. Lines starting
# with '#' will be ignored, and an empty message aborts the commit."#;
        let editmsg_filepath = self.bitdir.join("COMMIT_EDITMSG");
        let mut editmsg_file = File::create(&editmsg_filepath)?;
        write!(editmsg_file, "{}", template)?;
        Command::new(editor).arg(&editmsg_filepath).status()?;
        let mut msg = String::new();
        for line in BufReader::new(File::open(&editmsg_filepath)?).lines() {
            let line = line?;
            if line.starts_with('#') {
                continue;
            }
            msg.push_str(&line);
        }
        std::fs::remove_file(editmsg_filepath)?;
        if msg.is_empty() {
            bail!("aborting commit due to empty commit message");
        }
        Ok(msg)
    }
}

impl Display for Commit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut buf = vec![];
        self.serialize(&mut buf).unwrap();
        write!(f, "{}", std::str::from_utf8(&buf).unwrap())
    }
}

impl Serialize for Commit {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        // adds the required spaces for multiline strings
        macro_rules! w {
            ($s:expr) => {
                writeln!(writer, "{}", $s.replace("\n", "\n "))
            };
        }

        w!(format!("tree {:}", self.tree))?;
        if let Some(parent) = &self.parent {
            w!(format!("parent {}", parent))?;
        }
        w!(format!("author {}", self.author))?;
        w!(format!("committer {}", self.committer))?;
        if let Some(gpgsig) = &self.gpgsig {
            w!(format!("gpgsig {}", gpgsig))?;
        }

        writeln!(writer)?;
        write!(writer, "{}", self.message)?;
        Ok(())
    }
}

impl DeserializeSized for Commit {
    fn deserialize_sized(r: &mut impl BufRead, size: u64) -> BitResult<Self> {
        let mut lines = r.take(size).lines();
        let mut attrs = HashMap::new();

        let mut key: Option<String> = None;
        let mut value: Option<String> = None;

        while let Some(line) = lines.next() {
            let line = line?;

            // line is a continuation of the previous line
            if let Some(v) = &mut value {
                if line.starts_with(' ') {
                    v.push('\n');
                    v.push_str(&line[1..]);
                    continue;
                } else {
                    attrs.insert(key.take().unwrap(), value.take().unwrap());
                }
            }

            // everything after the current (blank) line is part of the message
            if line.is_empty() {
                break;
            }

            let (k, v) =
                line.split_once(' ').unwrap_or_else(|| panic!("Failed to parse line `{}`", line));
            key = Some(k.to_owned());
            value = Some(v.to_owned());
        }

        let message = lines.collect::<Result<Vec<_>, _>>()?.join("\n");

        let tree = attrs["tree"].parse().unwrap();
        let parent = attrs.get("parent").map(|parent| parent.parse().unwrap());
        let author = attrs["author"].parse().unwrap();
        let committer = attrs["committer"].parse().unwrap();
        let gpgsig = attrs.get("gpgsig").map(|sig| sig.to_owned());
        Ok(Self { tree, parent, author, committer, message, gpgsig })
    }
}

impl BitObj for Commit {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Commit
    }
}

#[cfg(test)]
mod tests;