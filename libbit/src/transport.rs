mod file;
mod ssh;

use std::collections::HashMap;

pub use file::*;
pub use ssh::*;

use crate::error::BitResult;
use crate::obj::Oid;
use crate::protocol::{BitProtocolRead, BitProtocolWrite};
use crate::refs::{BitRef, RefUpdateCause, SymbolicRef};
use crate::remote::Remote;
use crate::repo::BitRepo;

#[async_trait]
pub trait Transport: BitProtocolRead + BitProtocolWrite {
    async fn fetch(&mut self, repo: BitRepo<'_>, remote: &Remote) -> BitResult<()> {
        let refs = self.parse_ref_discovery().await?;
        let remote_mapping = refs
            .into_iter()
            .filter_map(|(sym, oid)| Some((remote.fetch.match_ref(sym)?, oid)))
            .collect::<HashMap<_, _>>();
        self.negotiate_packs(repo, &remote_mapping).await?;

        for (&remote, &oid) in &remote_mapping {
            let to = BitRef::Direct(oid);
            repo.update_ref(remote, to, RefUpdateCause::Fetch { to })?;
        }
        Ok(())
    }

    async fn negotiate_packs(
        &mut self,
        repo: BitRepo<'_>,
        remote_mapping: &HashMap<SymbolicRef, Oid>,
    ) -> BitResult<()> {
        let mut wanted = vec![];
        for (&remote, &remote_oid) in remote_mapping {
            let local_oid = repo.try_fully_resolve_ref(remote)?.unwrap_or(Oid::UNKNOWN);
            if local_oid == remote_oid {
                continue;
            }
            wanted.push(local_oid);
        }

        for &oid in &wanted {
            self.want(oid).await?;
        }
        self.write_flush_packet().await?;

        if wanted.is_empty() {
            return Ok(());
        }

        // let has = repo.revwalk();

        // for &oid in &has {
        //     self.have(oid).await?;
        // }
        self.write_flush_packet().await?;
        Ok(())
    }

    async fn parse_ref_discovery(&mut self) -> BitResult<HashMap<SymbolicRef, Oid>> {
        let mut mapping = HashMap::new();

        let packet = self.recv_packet().await?;
        let mut iter = packet.split(|&byte| byte == 0x00);
        let ref_line = iter.next().ok_or_else(|| anyhow!("malformed first line"))?;
        let _capabilities = iter.next().ok_or_else(|| anyhow!("malformed first line"))?;
        ensure!(iter.next().is_none());

        let (oid, sym) = parse_ref_line(ref_line)?;
        mapping.insert(sym, oid);

        loop {
            let packet = self.recv_packet().await?;
            if packet.is_empty() {
                break Ok(mapping);
            }
            let (oid, sym) = parse_ref_line(&packet)?;
            mapping.insert(sym, oid);
        }
    }
}

fn parse_ref_line(bytes: &[u8]) -> BitResult<(Oid, SymbolicRef)> {
    let s = std::str::from_utf8(bytes)?;
    let (oid, sym) = s.split_once(' ').ok_or_else(|| anyhow!("malformed ref line"))?;
    Ok((oid.parse()?, sym.parse()?))
}
