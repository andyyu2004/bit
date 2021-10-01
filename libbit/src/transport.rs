mod file;
mod ssh;

use std::collections::HashMap;

use fallible_iterator::FallibleIterator;
pub use file::*;
pub use ssh::*;

use crate::error::BitResult;
use crate::obj::{BitObject, Oid};
use crate::protocol::{BitProtocolRead, BitProtocolWrite, Capabilities, Capability};
use crate::refs::{BitRef, SymbolicRef};
use crate::remote::Remote;
use crate::repo::BitRepo;

pub const MULTI_ACK_BATCH_SIZE: usize = 32;

#[async_trait]
pub trait Transport: BitProtocolRead + BitProtocolWrite {
    async fn fetch(&mut self, repo: BitRepo<'_>, remote: &Remote) -> BitResult<()> {
        let (refs, capabilities) = self.parse_ref_discovery_and_capabilities().await?;

        ensure!(
            capabilities.contains(&Capability::MultiAckDetailed),
            "require `multi_ack_detailed` capability"
        );
        ensure!(
            capabilities.contains(&Capability::SideBand64k),
            "require `side-band-64k` capability"
        );
        ensure!(capabilities.contains(&Capability::OfsDelta), "require `ofs-delta` capability");

        let remote_mapping = refs
            .into_iter()
            .filter_map(|(sym, oid)| Some((remote.fetch.match_ref(sym)?, oid)))
            .collect::<HashMap<_, _>>();
        self.negotiate_packs(repo, &remote_mapping).await?;

        // TODO check the refspec for forcedness before updating: create a function `try_update_remote_ref`
        for (&remote, &oid) in &remote_mapping {
            let to = BitRef::Direct(oid);
            repo.update_ref_for_fetch(remote, to)?;
        }
        Ok(())
    }

    async fn negotiate_packs(
        &mut self,
        repo: BitRepo<'_>,
        remote_mapping: &HashMap<SymbolicRef, Oid>,
    ) -> BitResult<()> {
        let mut wanted = vec![];
        let mut local_tips = vec![];
        for (&remote, &remote_oid) in remote_mapping {
            let local_oid = repo.try_fully_resolve_ref(remote)?;
            if let Some(local_oid) = local_oid {
                local_tips.push(local_oid)
            }
            if local_oid != Some(remote_oid) {
                wanted.push(remote_oid);
            }
        }

        for (i, &oid) in wanted.iter().enumerate() {
            if i == 0 {
                let capabilities =
                    [Capability::MultiAckDetailed, Capability::OfsDelta, Capability::SideBand64k]
                        .map(|cap| cap.to_string())
                        .join(" ");
                self.write_packet(format!("want {} {}\n", oid, capabilities).as_bytes()).await?;
            } else {
                self.want(oid).await?;
            }
        }
        self.write_flush_packet().await?;

        if wanted.is_empty() {
            return Ok(());
        }

        let mut walk = repo.revwalk_builder().roots_iter(local_tips)?.build();
        loop {
            // TODO exit early when "ready" whatever that means
            for _ in 0..MULTI_ACK_BATCH_SIZE {
                let next_commit = match walk.next()? {
                    Some(commit) => commit,
                    None => break,
                };
                self.have(next_commit.oid()).await?;
            }

            // TODO same as above
            break self.done().await?;
            // TODO handle ack/nak/error response to our done
        }

        loop {
            let packet = self.recv_packet().await?;
            let s = std::str::from_utf8(&packet)?;
            if s.starts_with("ACK") {
                if s.ends_with("common\n") || s.ends_with("ready\n") || s.ends_with("continue\n") {
                    continue;
                }
                // found the final ack?
                break;
            } else {
                // TODO probably a NAK
                todo!("recv {}", s);
            }
        }
        self.recv_pack(repo).await?;
        Ok(())
    }

    async fn parse_ref_discovery_and_capabilities(
        &mut self,
    ) -> BitResult<(HashMap<SymbolicRef, Oid>, Capabilities)> {
        let mut mapping = HashMap::new();

        let packet = self.recv_packet().await?;
        let mut iter = packet.split(|&byte| byte == 0x00);
        let ref_line = iter.next().ok_or_else(|| anyhow!("malformed first line"))?;
        let capabilities = iter.next().ok_or_else(|| anyhow!("malformed first line"))?;
        let parsed_capabilities = capabilities
            .split(|&b| b == b' ')
            .map(|bytes| Ok(std::str::from_utf8(bytes)?.trim_end().parse()?))
            .collect::<BitResult<Capabilities>>()?;
        ensure!(iter.next().is_none());

        let (oid, sym) = parse_ref_line(ref_line)?;
        mapping.insert(sym, oid);

        loop {
            let packet = self.recv_packet().await?;
            if packet.is_empty() {
                break Ok((mapping, parsed_capabilities));
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
