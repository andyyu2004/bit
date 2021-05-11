use lazy_static::lazy_static;

use crate::signature::{BitEpochTime, BitSignature, BitTime, BitTimeZoneOffset};

use super::*;
use std::io::Cursor;
use std::str::FromStr;

// got this number by inspecting last entry of the fanout table
const PACK_LEN: u64 = 11076;

#[test]
fn test_deserialize_pack_idx_is_ok() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/pack.idx") as &[u8];
    let _pack_idx = PackIndex::deserialize_unbuffered(bytes)?;
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_start() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let index = PackIndexReader::new(&mut cursor)?.find_oid_index(
        // this hash is the first oid in sorted list
        BitHash::from_str("0004a3cf85dbcbfbef916599145a0c370bb78cf5").unwrap(),
    )?;
    assert_eq!(index, 0);
    Ok(())
}
#[test]
fn test_pack_idx_find_oid_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let index = PackIndexReader::new(&mut cursor)?.find_oid_index(
        // this hash is the last oid in sorted list
        BitHash::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    assert_eq!(index, PACK_LEN - 1);
    Ok(())
}

fn pack() -> Pack {
    Pack {
        pack: BitPath::intern("tests/files/pack.pack"),
        idx: BitPath::intern("tests/files/pack.idx"),
    }
}

lazy_static! {
    // oid of the HEAD commit at the time (undeltified)
    static ref HEAD_OID: BitHash = "1806658f16f76480a3f40461db577a02d1e01591".parse().unwrap();
    // oid of the tree of the HEAD commit at the time (3 levels deltified)
    static ref TREE_OID: BitHash = "2a09245f13365a5d812a9d463595d815062b7d42".parse().unwrap();
}

#[test]
fn test_check_oid_exists_in_pack() -> BitResult<()> {
    assert!(pack().obj_exists(*HEAD_OID)?);
    Ok(())
}

#[test]
fn test_find_offset_in_pack() -> BitResult<()> {
    let (_crc, offset) = pack().index_reader()?.find_oid_crc_offset(*HEAD_OID)?;
    assert_eq!(offset, 2247656);
    Ok(())
}

#[test]
fn test_read_type_and_size_from_offset_in_pack() -> BitResult<()> {
    let pack = pack();
    let (_crc, offset) = pack.index_reader()?.find_oid_crc_offset(*HEAD_OID)?;
    let (obj_ty, size) = pack.pack_reader()?.read_header_from_offset(offset)?;
    assert_eq!(obj_ty, BitObjType::Commit);
    assert_eq!(size, 215);
    Ok(())
}

#[test]
fn test_read_pack_undeltified_oid() -> BitResult<()> {
    let pack = pack();
    let obj = pack.read_obj(*HEAD_OID)?;
    let commit = Commit {
        tree: "2a09245f13365a5d812a9d463595d815062b7d42".into(),
        author: BitSignature {
            name: "Andy Yu".to_owned(),
            email: "andyyu2004@gmail.com".to_owned(),
            time: BitTime {
                time: BitEpochTime::new(1619232531),
                offset: BitTimeZoneOffset::new(720),
            },
        },
        committer: BitSignature {
            name: "Andy Yu".to_owned(),
            email: "andyyu2004@gmail.com".to_owned(),
            time: BitTime {
                time: BitEpochTime::new(1619232531),
                offset: BitTimeZoneOffset::new(720),
            },
        },
        message: "broken".to_owned(),
        parent: Some("4719f26c289d6bc2dbb3e68ac437537828cd8a11".into()),
        gpgsig: None,
    };
    assert_eq!(commit, obj.into_commit());
    Ok(())
}

#[test]
fn test_read_pack_deltified_oid() -> BitResult<()> {
    let pack = pack();
    let obj = pack.read_obj(*TREE_OID)?;
    let tree = Tree {
        entries: vec![
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern(".gitignore"),
                hash: "26f14f842aaa4a0d97bb8819be8fb71c0190427e".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern(".gitlab-ci.yml"),
                hash: "a9c351760ad80fc76d477a49f2d3950f6e0a80c9".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o40000),
                path: BitPath::intern(".vscode"),
                hash: "bd956bb951e5cdd695ea59b629f2e504d58df12d".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("Cargo.lock"),
                hash: "436259997a05110a3fd6eb5bf8054621948d6916".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("Cargo.toml"),
                hash: "9c71a109c23bba0678a98ea37e07af7ee25ca322".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("README.md"),
                hash: "9771b9be0344bc413747a9d1e124d95298c6a116".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o40000),
                path: BitPath::intern("examples"),
                hash: "090f714f0b99d6c422b80613ca45e8fb36908deb".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100755),
                path: BitPath::intern("llvm.sh"),
                hash: "58eef27644b5c32d53b886f20e87e5aa230b6df6".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("rustfmt.toml"),
                hash: "e6d13c9311011cbb74eed646e1f9c45af4d9b59d".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o40000),
                path: BitPath::intern("src"),
                hash: "f81995cdfd381ad571814e6c94809ff2251259e0".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o40000),
                path: BitPath::intern("tests"),
                hash: "8abe5eabdddd1aa98cbbd834cb8583f4959a6843".into(),
            },
        ]
        .into_iter()
        .collect(),
    };
    assert_eq!(obj.into_tree(), tree);
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_offset_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let (_crc, pack_idx) = PackIndexReader::new(&mut cursor)?.find_oid_crc_offset(
        // this hash is the last oid in sorted list
        BitHash::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    // `git verify-pack src/libbit/tests/files/pack.pack -v | rg fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47`
    assert_eq!(pack_idx, 2151306);
    Ok(())
}
