use crate::obj::Oid;
use crate::signature::BitSignature;

#[derive(Debug, Clone, PartialEq)]
pub struct ReflogEntry {
    old_oid: Oid,
    new_oid: Oid,
    committer: BitSignature,
    msg: String,
}

#[derive(Debug)]
pub struct Reflog {
    entries: Vec<ReflogEntry>,
}
