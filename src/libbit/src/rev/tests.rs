use crate::refs::SymbolicRef;

use super::*;

macro_rules! lex {
    ($rev:expr) => {
        RevspecLexer::new($rev).collect::<Vec<_>>().expect("failed to lex revspec")
    };
}

macro_rules! parse {
    ($rev:expr) => {
        Revspec::from_str($rev).expect("failed to parse revspec")
    };
}

macro_rules! symbolic_ref {
    ($sym:expr) => {
        BitRef::Symbolic(SymbolicRef::from_str($sym).unwrap())
    };
}

#[test]
fn test_lex_simple_revspec() {
    let tokens = lex!("HEAD^");
    assert_eq!(tokens, vec![Token::Ref(symbolic_ref!("HEAD")), Token::Caret]);
}

#[test]
fn test_parse_revspec_parent() {
    let rev = parse!("HEAD^");
    assert_eq!(rev, Revspec::Parent(Box::new(Revspec::Ref(symbolic_ref!("HEAD")))))
}

#[test]
fn test_lex_revspec_with_symref_ancestor() {
    let tokens = lex!("HEAD~5");
    assert_eq!(tokens, vec![Token::Ref(symbolic_ref!("HEAD")), Token::Tilde, Token::Num(5)]);
}

#[test]
fn test_parse_revspec_with_symref_ancestor() {
    let rev = parse!("HEAD~5");
    assert_eq!(rev, Revspec::Ancestor(Box::new(Revspec::Ref(symbolic_ref!("HEAD"))), 5));
}

#[test]
fn test_parse_revspec_with_symref() {
    let rev = parse!("e3eaee01f47f98216f4160658179420ff5e30f50");
    assert_eq!(rev, Revspec::Ref(BitRef::Direct("e3eaee01f47f98216f4160658179420ff5e30f50".into())))
}
