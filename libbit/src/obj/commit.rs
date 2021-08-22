use super::{BitObjCached, ImmutableBitObject, Tree, Treeish, WritableObject};
use crate::error::{BitGenericError, BitResult};
use crate::graph::{Dag, DagNode};
use crate::obj::{BitObjType, BitObject, Oid};
use crate::odb::BitObjDbBackend;
use crate::peel::Peel;
use crate::repo::{BitRepo, Repo};
use crate::serialize::{DeserializeSized, Serialize};
use crate::signature::BitSignature;
use smallvec::SmallVec;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::ops::Deref;
use std::process::Command;
use std::str::FromStr;

#[derive(PartialEq, Clone, Debug)]
pub struct Commit<'rcx> {
    owner: BitRepo<'rcx>,
    cached: BitObjCached,
    inner: MutableCommit,
}

impl<'rcx> Commit<'rcx> {
    /// Get a reference to the commit's tree.
    pub fn tree_oid(&self) -> Oid {
        self.tree
    }
}

impl Deref for Commit<'_> {
    type Target = MutableCommit;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub type CommitParents = SmallVec<[Oid; 2]>;

#[derive(PartialEq, Clone, Debug)]
pub struct MutableCommit {
    pub tree: Oid,
    pub author: BitSignature,
    pub committer: BitSignature,
    pub message: CommitMessage,
    pub parents: CommitParents,
    pub gpgsig: Option<String>,
    pub mergetag: Option<MergeTag>,
}

#[derive(PartialEq, Clone, Debug)]
pub struct CommitMessage {
    pub subject: String,
    pub message: String,
}

impl FromStr for CommitMessage {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (subject, message) = if let Some((subject, message)) = s.split_once("\n\n") {
            (subject, message)
        } else {
            (s, "")
        };

        Ok(Self { subject: subject.to_owned(), message: message.to_owned() })
    }
}

impl Display for CommitMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.subject)?;
        if !self.message.is_empty() {
            write!(f, "\n\n{}", self.message)?;
        }
        Ok(())
    }
}

impl<'rcx> Treeish<'rcx> for Commit<'rcx> {
    fn treeish(self, repo: BitRepo<'rcx>) -> BitResult<Tree<'rcx>> {
        self.tree.treeish(repo)
    }

    fn treeish_oid(&self, _repo: BitRepo<'rcx>) -> BitResult<Oid> {
        Ok(self.tree_oid())
    }
}

impl MutableCommit {
    pub fn new(
        tree: Oid,
        parents: CommitParents,
        message: CommitMessage,
        author: BitSignature,
        committer: BitSignature,
    ) -> Self {
        Self::new_with_gpg(tree, parents, message, author, committer, None)
    }

    pub fn new_with_gpg(
        tree: Oid,
        parents: CommitParents,
        message: CommitMessage,
        author: BitSignature,
        committer: BitSignature,
        gpgsig: Option<String>,
    ) -> Self {
        Self { tree, author, committer, message, parents, gpgsig, mergetag: None }
    }

    pub fn sole_parent(&self) -> Oid {
        assert_eq!(
            self.parents.len(),
            1,
            "expected exactly one commit parent, found `{}`",
            self.parents.len()
        );
        self.parents[0]
    }
}

impl<'rcx> BitRepo<'rcx> {
    /// create and write commit to odb
    pub fn mk_commit(
        self,
        tree: Oid,
        message: CommitMessage,
        parents: CommitParents,
    ) -> BitResult<Oid> {
        ensure!(self.read_obj_header(tree)?.obj_type == BitObjType::Tree);
        let author = self.user_signature()?;
        let committer = author.clone();

        for &parent in &parents {
            let parent = self.read_obj(parent)?;
            ensure!(parent.is_commit());
            // we use timestamps to order commits
            // we can't enforce a strict ordering as it is valid for them to have the exact same time
            ensure!(
                parent.into_commit().committer.time <= committer.time,
                "Attempted to create a commit that is older than it's parent. Please check the system clock."
            );
        }

        let commit = MutableCommit::new(tree, parents, message, author, committer);
        self.odb()?.write(&commit)
    }

    pub fn read_commit_msg(self) -> BitResult<CommitMessage> {
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
        CommitMessage::from_str(&msg)
    }
}

impl Display for Commit<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut buf = vec![];
        self.serialize(&mut buf).unwrap();
        write!(f, "{}", std::str::from_utf8(&buf).unwrap())
    }
}

impl WritableObject for MutableCommit {
    fn obj_ty(&self) -> BitObjType {
        BitObjType::Commit
    }
}

impl Serialize for MutableCommit {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        // adds the required spaces for multiline strings
        macro_rules! w {
            ($s:expr) => {
                writeln!(writer, "{}", $s.replace("\n", "\n "))
            };
        }

        w!(format!("tree {:}", self.tree))?;
        for parent in &self.parents {
            w!(format!("parent {}", parent))?;
        }
        w!(format!("author {}", self.author))?;
        w!(format!("committer {}", self.committer))?;

        if let Some(gpgsig) = &self.gpgsig {
            w!(format!("gpgsig {}", gpgsig))?;
        }

        if let Some(mergetag) = &self.mergetag {
            w!(format!("mergetab {}", mergetag))?;
        }

        writeln!(writer)?;
        write!(writer, "{}", self.message)?;
        Ok(())
    }
}

impl DeserializeSized for MutableCommit {
    fn deserialize_sized(r: impl BufRead, size: u64) -> BitResult<Self> {
        // WARNING the following line fails for non utf8 encodings with error `stream did not contain valid UTF-8 `
        // not intending to support non-utf8 anytime soon
        let lines = r
            .take(size)
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|_| panic!("unsupported non-utf8 encoding detected in commit"));
        let mut builder = CommitBuilder::default();
        let mut iter = lines.iter().peekable();
        while let Some(line) = iter.next() {
            if line.is_empty() {
                break;
            }

            let (field, value) =
                line.split_once(' ').unwrap_or_else(|| panic!("Failed to parse line `{}`", line));
            let mut value = value.to_owned();

            // if the line starts with space it is a continuation of the previous key
            while let Some(line) = iter.peek() {
                match line.strip_prefix(' ') {
                    Some(stripped) => {
                        value.push('\n');
                        value.push_str(stripped);
                        iter.next();
                    }
                    None => break,
                }
            }

            match field {
                "parent" => builder.parents.push(value.parse()?),
                "tree" => builder.tree = Some(value.parse()?),
                "author" => builder.author = Some(value.parse()?),
                "committer" => builder.committer = Some(value.parse()?),
                "gpgsig" => builder.gpgsig = Some(value.parse()?),
                "mergetag" => builder.mergetag = Some(value.parse()?),
                _ => eprintln!(
                    "ignoring unknown field `{}` when parsing commit (commit has parents = `{:?}`: field has value `{}`)",
                    field, builder.parents, value
                ),
            }
        }

        // TODO could definitely do this more efficiently but its not urgent
        // as we have a vector we could just slice it and join without doing any copying
        // we would just have to keep track of where to slice it from
        let message = iter.cloned().collect::<Vec<_>>().join("\n");
        builder.message = Some(message.parse()?);
        builder.build()
    }
}

// don't really know what this is, just handling it to avoid errors above
#[derive(Clone, Debug, PartialEq)]
pub struct MergeTag {
    todo_parse: String,
}

impl FromStr for MergeTag {
    type Err = BitGenericError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self { todo_parse: s.to_owned() })
    }
}

impl Display for MergeTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.todo_parse)
    }
}

#[derive(Default)]
struct CommitBuilder {
    pub tree: Option<Oid>,
    pub author: Option<BitSignature>,
    pub committer: Option<BitSignature>,
    pub message: Option<CommitMessage>,
    pub parents: CommitParents,
    pub gpgsig: Option<String>,
    pub mergetag: Option<MergeTag>,
}

impl CommitBuilder {
    fn build(mut self) -> BitResult<MutableCommit> {
        Ok(MutableCommit {
            tree: self.tree.ok_or_else(|| anyhow!("commit missing tree"))?,
            author: self.author.ok_or_else(|| anyhow!("commit missing author"))?,
            committer: self.committer.ok_or_else(|| anyhow!("commit missing committer"))?,
            message: self.message.ok_or_else(|| anyhow!("commit missing message"))?,
            parents: self.parents,
            gpgsig: self.gpgsig.take(),
            mergetag: self.mergetag.take(),
        })
    }
}

impl<'rcx> BitObject<'rcx> for Commit<'rcx> {
    fn obj_cached(&self) -> &BitObjCached {
        &self.cached
    }

    fn owner(&self) -> BitRepo<'rcx> {
        self.owner
    }
}

impl<'rcx> ImmutableBitObject<'rcx> for Commit<'rcx> {
    type Mutable = MutableCommit;

    fn from_mutable(owner: BitRepo<'rcx>, cached: BitObjCached, inner: Self::Mutable) -> Self {
        Self { owner, cached, inner }
    }
}

// trait needs some changes for commit to have a reasonable implementation
impl<'rcx> Dag for Commit<'rcx> {
    type Node = Oid;
    type NodeData = Self;
    type Nodes = SmallVec<[Oid; 2]>;

    fn node_data(&self, oid: Oid) -> BitResult<Self::NodeData> {
        oid.peel(self.owner)
    }

    fn nodes(&self) -> BitResult<Self::Nodes> {
        todo!()
    }
}

impl<'rcx> DagNode<Commit<'rcx>> for Commit<'rcx> {
    fn adjacent(&self) -> <Commit<'rcx> as Dag>::Nodes {
        self.parents.clone()
    }
}
