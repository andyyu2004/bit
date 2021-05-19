use crate::error::{BitError, BitErrorExt, BitResult};
use crate::obj::PartialOid;
use crate::repo::BitRepo;

#[test]
fn test_loose_ambiguous_prefix_loose_odb() -> BitResult<()> {
    BitRepo::find("tests/repos/ambiguous-prefix", |repo| {
        let partial = PartialOid::from("2341");
        let err = repo.read_obj(partial).unwrap_err();
        assert_eq!(
            err.into_bit_error()?,
            BitError::AmbiguousPrefix(
                partial,
                vec![
                    "2341a1ca41f3a7cb692c82e6a0b66e131c74fe14".into(),
                    "2341b13fb53d240de3722dd6c0e93b0d2edabada".into()
                ]
            )
        );
        Ok(())
    })
}
