use crate::cli::*;
use crate::error::BitResult;
use crate::obj::{self, BitObj, BitObjKind};
use crate::repo::BitRepo;
use std::fs::File;
use std::io::Read;

pub fn bit_init(opts: BitInit) -> BitResult<()> {
    let _repo = BitRepo::init(&opts.path)?;
    Ok(())
}

pub fn bit_hash_object(opts: BitHashObject) -> BitResult<()> {
    let mut buf = vec![];
    File::open(&opts.path)?.read_to_end(&mut buf)?;
    let object = BitObjKind::new(opts.objtype, &buf);

    if opts.write {
        BitRepo::new()?.write_obj(&object)?;
    } else {
        eprintln!("{}", obj::hash_obj(&object)?);
    }
    Ok(())
}

pub fn bit_cat_file(opts: BitCatFile) -> BitResult<()> {
    let repo = BitRepo::new()?;
    let id = repo.find_obj(&opts.name)?;
    let obj = repo.read_obj_from_hash(&id)?;
    // just for now
    match obj {
        BitObjKind::Blob(blob) => println!("{}", std::str::from_utf8(&blob.bytes).unwrap()),
        BitObjKind::Commit(_) => todo!(),
    }
    Ok(())
}
