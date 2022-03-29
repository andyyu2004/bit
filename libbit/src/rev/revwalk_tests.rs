use super::*;
use crate::obj::BitObject;
use fallible_iterator::FallibleIterator;

#[test]
fn test_revwalk_on_branch() -> BitResult<()> {
    BitRepo::find(repos_dir!("revwalk-test"), |repo| {
        let revwalk = RevWalk::walk_revspec(&repo, &rev!("master"))?;
        let oids = revwalk.map(|commit| Ok(commit.oid())).collect::<Vec<_>>()?;

        // output from `git rev-list master`
        let expected = [
            "9aed6cad276983296289c808f85cdcecdcbc6aff".into(),
            "2c5a4ac9722e245f12b83642fe24848252b9b1ce".into(),
            "dfaea58c308d8ede90abbd439c6f84ea3b95402c".into(),
            "46bcfda1e8b47e02e3168605c170aaf338472326".into(),
            "f78dd0ade418038677cda9ada00989a21af1e242".into(),
            "7b158cc2692f71b0d39f5abcb3ede6197aa55708".into(),
            "8a148895abe4507c87c4e2756b9c7743dbc3deb7".into(),
            "f439ec863a80027439ecdbe78c1a517cb5b3caca".into(),
            "988788c14ac1f5324cdd60335e7e01cfa628be1d".into(),
            "f8103f9989467247cd1097e0aaa538b545a99996".into(),
            "c1bc532c9e6d1b74888bd893a316dfcbda218beb".into(),
        ];

        assert_eq!(&oids[..], expected);
        Ok(())
    })
}

#[test]
fn test_revwalk_on_multiple_branches() -> BitResult<()> {
    BitRepo::find(repos_dir!("revwalk-test"), |repo| {
        let revwalk = RevWalk::walk_revspecs(&repo, &[&rev!("master"), &rev!("some-branch")])?;
        let oids = revwalk.map(|commit| Ok(commit.oid())).collect::<Vec<_>>()?;

        // output from `git rev-list --all` on master
        let expected = [
            "9aed6cad276983296289c808f85cdcecdcbc6aff".into(),
            "2c5a4ac9722e245f12b83642fe24848252b9b1ce".into(),
            "e05d3317f7de167d3c66926c4b4d65802aa679fc".into(),
            "75657db53f6f7611241c87745ac793f3e5294faa".into(),
            "957ab9b042e089ad1f9292697764e884ed84a244".into(),
            "dfaea58c308d8ede90abbd439c6f84ea3b95402c".into(),
            "46bcfda1e8b47e02e3168605c170aaf338472326".into(),
            "f78dd0ade418038677cda9ada00989a21af1e242".into(),
            "7b158cc2692f71b0d39f5abcb3ede6197aa55708".into(),
            "8a148895abe4507c87c4e2756b9c7743dbc3deb7".into(),
            "f439ec863a80027439ecdbe78c1a517cb5b3caca".into(),
            "988788c14ac1f5324cdd60335e7e01cfa628be1d".into(),
            "f8103f9989467247cd1097e0aaa538b545a99996".into(),
            "c1bc532c9e6d1b74888bd893a316dfcbda218beb".into(),
        ];

        assert_eq!(&oids[..], expected);
        Ok(())
    })
}

#[test]
fn test_revwalk_with_exclude() -> BitResult<()> {
    BitRepo::find(repos_dir!("revwalk-test"), |repo| {
        //  `git log --graph --oneline --decorate --all`
        //  9aed6ca (HEAD -> master) update readme
        //  2c5a4ac H back on master
        //  dfaea58 three of these
        //  46bcfda three of these
        //  f78dd0a three of these
        //  7b158cc E
        //  8a14889 D
        // |   e05d331 (some-branch) G
        // |   75657db F
        // |   957ab9b off C
        // | /
        //  f439ec8 C
        //  988788c B
        //  f8103f9 A
        //  c1bc532 init output from `git rev-list master`
        let revwalk = repo
            .revwalk_builder()
            .root_revisions(Some(&rev!("master")))?
            .excluding_revisions(Some(&rev!("some-branch")))?
            .build();
        let oids = revwalk.map(|commit| Ok(commit.oid())).collect::<Vec<_>>()?;
        let expected = [
            "9aed6cad276983296289c808f85cdcecdcbc6aff".into(),
            "2c5a4ac9722e245f12b83642fe24848252b9b1ce".into(),
            "dfaea58c308d8ede90abbd439c6f84ea3b95402c".into(),
            "46bcfda1e8b47e02e3168605c170aaf338472326".into(),
            "f78dd0ade418038677cda9ada00989a21af1e242".into(),
            "7b158cc2692f71b0d39f5abcb3ede6197aa55708".into(),
            "8a148895abe4507c87c4e2756b9c7743dbc3deb7".into(),
        ];
        assert_eq!(&oids[..], expected);

        let revwalk = repo
            .revwalk_builder()
            .root_revisions(Some(&rev!("some-branch")))?
            .excluding_revisions(Some(&rev!("master")))?
            .build();
        let oids = revwalk.map(|commit| Ok(commit.oid())).collect::<Vec<_>>()?;
        let expected = [
            "e05d3317f7de167d3c66926c4b4d65802aa679fc".into(),
            "75657db53f6f7611241c87745ac793f3e5294faa".into(),
            "957ab9b042e089ad1f9292697764e884ed84a244".into(),
        ];
        assert_eq!(&oids[..], expected);
        Ok(())
    })
}

// TODO there are bugs in revwalk
// A - B
//  \
//   C - D - E
// #[test]
// fn todo() -> BitResult<()> {
//     BitRepo::with_empty_repo(|repo| {
//         let mut dag = DagBuilder::default();
//         let [a, b, c, d, e] = dag.mk_nodes();
//         dag.add_parents([(b, a), (e, d), (d, c), (c, a)]);
//         let commits = dag.apply_to_repo(repo)?;

//         let commit_b = repo.read_obj_commit(commits[&b])?;
//         let commit_e = repo.read_obj_commit(commits[&e])?;
//         let revwalk = RevwalkBuilder::new(repo)
//             .roots(smallvec![commit_b])
//             .excluding(smallvec![commit_e])
//             .build();
//         dbg!(&commits);
//         let oids = revwalk.map(|commit| Ok(commit.oid())).collect::<Vec<_>>()?;
//         let expected = [commit_b.oid()];
//         assert_eq!(&oids[..], expected);

//         Ok(())
//     })
// }
