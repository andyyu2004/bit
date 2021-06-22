use super::*;
use crate::signature::{BitEpochTime, BitSignature, BitTime, BitTimeZoneOffset};
use lazy_static::lazy_static;
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
        Oid::from_str("0004a3cf85dbcbfbef916599145a0c370bb78cf5").unwrap(),
    )?;
    assert_eq!(index, 0);
    Ok(())
}
#[test]
fn test_pack_idx_find_oid_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let index = PackIndexReader::new(&mut cursor)?.find_oid_index(
        // this hash is the last oid in sorted list
        Oid::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    assert_eq!(index, PACK_LEN - 1);
    Ok(())
}

fn pack() -> BitResult<Pack> {
    Pack::new(BitPath::intern("tests/files/pack.pack"), BitPath::intern("tests/files/pack.idx"))
}

fn rustc_pack() -> BitResult<Pack> {
    Pack::new(
        BitPath::intern("tests/files/pack-60ac90f4de41b44ce379159aef33377876973d6a.pack"),
        BitPath::intern("tests/files/pack-60ac90f4de41b44ce379159aef33377876973d6a.idx"),
    )
}

lazy_static! {
    // these commits are from the packfile in the `l-lang` repository
    // oid of the HEAD commit at the time (undeltified)
    static ref HEAD_OID: Oid = "1806658f16f76480a3f40461db577a02d1e01591".parse().unwrap();
    // oid of the tree of the HEAD commit at the time (3 levels deltified)
    static ref TREE_OID: Oid = "2a09245f13365a5d812a9d463595d815062b7d42".parse().unwrap();
    // oid of the tree of the `src` folder at some point in time
    static ref SRC_TREE_OID: Oid = "223ee1fdad64a152c8e88a5472233dbc2e0119aa".parse().unwrap();
}

#[test]
fn test_check_oid_exists_in_pack() -> BitResult<()> {
    assert!(pack()?.obj_exists(*HEAD_OID)?);
    Ok(())
}

#[test]
fn test_find_offset_in_pack() -> BitResult<()> {
    let (_crc, offset) = pack()?.idx_reader().find_oid_crc_offset(*HEAD_OID)?;
    assert_eq!(offset, 2247656);
    Ok(())
}

#[test]
fn test_read_type_and_size_from_offset_in_pack() -> BitResult<()> {
    let mut pack = pack()?;
    let (_crc, offset) = pack.idx_reader().find_oid_crc_offset(*HEAD_OID)?;
    let header = pack.pack_reader().read_header_from_offset(offset)?;
    assert_eq!(header.obj_type, BitObjType::Commit);
    assert_eq!(header.size, 215);
    Ok(())
}

#[test]
fn test_read_pack_undeltified_oid() -> BitResult<()> {
    let mut pack = pack()?;
    let obj = pack.read_obj(*HEAD_OID)?;
    let commit = MutableCommit {
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
        message: CommitMessage { subject: "broken".to_owned(), message: "".into() },
        parent: Some("4719f26c289d6bc2dbb3e68ac437537828cd8a11".into()),
        gpgsig: None,
    };
    assert_eq!(&commit, &*obj.into_commit());
    Ok(())
}

#[test]
fn test_read_pack_deltified_oid() -> BitResult<()> {
    let mut pack = pack()?;
    let obj = pack.read_obj(*TREE_OID)?;
    let tree = MutableTree::new(
        vec![
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern(".cargo"),
                oid: "1e5a588e1aa62fffff318db5fb046c5cdfdd91d3".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern(".gitignore"),
                oid: "26f14f842aaa4a0d97bb8819be8fb71c0190427e".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern(".gitlab-ci.yml"),
                oid: "a9c351760ad80fc76d477a49f2d3950f6e0a80c9".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern(".vscode"),
                oid: "cecb72a56ee2fb43a2e13bc05924f7cbc30859be".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("Cargo.lock"),
                oid: "436259997a05110a3fd6eb5bf8054621948d6916".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("Cargo.toml"),
                oid: "c1c7c27c2c8dddc367baaa9af95c81c4942bbb3c".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("README.md"),
                oid: "9771b9be0344bc413747a9d1e124d95298c6a116".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("examples"),
                oid: "111475360a14e05ff2476471a174e3b94b6bfbc9".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100755),
                path: BitPath::intern("llvm.sh"),
                oid: "58eef27644b5c32d53b886f20e87e5aa230b6df6".into(),
            },
            TreeEntry {
                mode: FileMode::new(0o100644),
                path: BitPath::intern("rustfmt.toml"),
                oid: "e6d13c9311011cbb74eed646e1f9c45af4d9b59d".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("src"),
                oid: "b9e61d6ae21c00ac6b3cd276371df6dc97abccfe".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("tests"),
                oid: "8abe5eabdddd1aa98cbbd834cb8583f4959a6843".into(),
            },
        ]
        .into_iter()
        .collect(),
    );

    assert_eq!(&tree, &*obj.into_tree()?);
    Ok(())
}

#[test]
fn test_read_pack_deltified_oid2() -> BitResult<()> {
    let mut pack = pack()?;
    let obj = pack.read_obj(*SRC_TREE_OID)?;
    let tree = MutableTree::new(
        vec![
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("arena"),
                oid: "375844f48ff48ab9bc6bab5b441f29acbff5b80a".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("ast"),
                oid: "7192fb06e4e2db31258a8ec461acce577d460356".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("astlowering"),
                oid: "24bf5671df647540e490e6049bf8d3a65ce3ae0f".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("cli"),
                oid: "b4c9aa7462315271d0bb157e464faf6bb8361228".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("codegen"),
                oid: "e15c7d98d4114b767beff2819ce4e56ad6f876c8".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("ds"),
                oid: "e6b79e424081bad73e8b297a1a066aab237e5716".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("error"),
                oid: "cf3d15fcc1bb27d7254b4cf54b58b5f943d9009d".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("gc"),
                oid: "c0d2f358f61cd73328fc7ac15715a1a409eba620".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("index"),
                oid: "bbfdaf6b02326087049e3cd018f13876bcf1ea83".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("infer"),
                oid: "b903e5200f895007aae3e4413061a160ca722066".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("ir"),
                oid: "68fea9956a2f1e305c55fa0860d54bc5cf95d34f".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("l"),
                oid: "d261d4556349799b9d55ee357983ba1f5a91fafd".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("lcore"),
                oid: "301436c693236bda4565c11c0ac91806861bbae6".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("ldriver"),
                oid: "1eb21ddfead6f028cd282db2ef8e7aefb85594d0".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("lex"),
                oid: "27ff5d729b99f6dd6f39920afb6ef3ecf79dc859".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("lutil"),
                oid: "e9c1db4e930d81d613d664c1bea3db82f4577bdb".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("macros"),
                oid: "82b1be89f9f296eaf3b7e553efc3efd9d8a87115".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("meta"),
                oid: "11830c74dc18117d5da55ba51071bd47ca71ab0e".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("mir"),
                oid: "ddb92fba7f54afe3d63dbb4228a1aad637436871".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("mirgen"),
                oid: "01ba8f48e66141d019bb789569a771c2d2aad221".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("parse"),
                oid: "31ca59b332d9597028802c9abc447da529afbfc4".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("queries"),
                oid: "3dd2c938bb887c53afad510b80ce871c76b3adbd".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("resolve"),
                oid: "e5a99cffdfccfc91fb5b3c3b8cec1291983434e6".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("session"),
                oid: "24ddc0d72640a9ae7d842c996f0bdab37a9b5870".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("span"),
                oid: "3d78ddf36de06157490cb57ad723f5c1982d4b73".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("tir"),
                oid: "ff4e4dedbe5c871db188fb01579e73858bd39c3f".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("traits"),
                oid: "2f6f26cf4496b25ef0405e8b8c52b1daac13694e".into(),
            },
            TreeEntry {
                mode: FileMode::TREE,
                path: BitPath::intern("typeck"),
                oid: "e9f679681a631d595fb63614b2bcbec292e428d3".into(),
            },
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(&tree, &*obj.into_tree()?);
    Ok(())
}

#[test]
fn test_pack_idx_find_oid_offset_end() -> BitResult<()> {
    let mut cursor = Cursor::new(include_bytes!("../../tests/files/pack.idx"));
    let (_crc, pack_idx) = PackIndexReader::new(&mut cursor)?.find_oid_crc_offset(
        // this hash is the last oid in sorted list
        Oid::from_str("fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47").unwrap(),
    )?;
    // `git verify-pack src/libbit/tests/files/pack.pack -v | rg fffc6e8cf5f6798732a6031ebf24d2f6aaa60e47`
    assert_eq!(pack_idx, 2151306);
    Ok(())
}

// this is actually a delta (ofs) of a tree, check that the type is expanded to the actual type
// and the size remains the type of the expanded tree
#[test]
fn test_packed_header_is_expanded() -> BitResult<()> {
    let mut pack = pack()?;
    let header = pack.read_obj_header("2a09245f13365a5d812a9d463595d815062b7d42".into())?;
    assert_eq!(header.obj_type, BitObjType::Tree);
    assert_eq!(header.size, 138);
    Ok(())
}

#[test]
fn test_read_obj_from_large_pack() -> BitResult<()> {
    // in particular this tests an edge case where the default size is 0x10000 which is not hit very often
    // TODO add some actual metadata to assert against
    // the test currently is just asking it to not error
    let mut pack = rustc_pack()?;
    for oid in pack.prefix_matches("3febc".into())? {
        pack.read_obj(oid)?;
    }
    Ok(())
}
