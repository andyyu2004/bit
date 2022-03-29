mod file;
mod ssh;

pub use file::*;
pub use ssh::*;

use crate::error::BitResult;
use crate::obj::{BitObject, Oid};
use crate::protocol::{BitProtocolRead, BitProtocolWrite, Capabilities, Capability};
use crate::refs::{BitRef, SymbolicRef};
use crate::remote::{FetchStatus, FetchSummary, Remote};
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;
use std::collections::HashMap;
use tokio::io::AsyncBufReadExt;

pub const MULTI_ACK_BATCH_SIZE: usize = 32;

#[async_trait]
pub trait ProtocolTransport: BitProtocolRead + BitProtocolWrite {
    fn repo(&self) -> &BitRepo;
    async fn fetch(&mut self, remote: &Remote) -> BitResult<FetchSummary> {
        if self.fill_buf().await?.is_empty() {
            bail!("could not read from remote repository")
        }

        let (refs, capabilities) = self.parse_ref_discovery_and_capabilities().await?;
        if refs.is_empty() {
            return Ok(FetchSummary::EMPTY_REMOTE);
        }

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

        let fetch_status = self.negotiate_packs(&remote_mapping).await?;

        // TODO check the refspec for forcedness before updating: create a function `try_update_remote_ref`
        for (&remote, &oid) in &remote_mapping {
            let to = BitRef::Direct(oid);
            self.repo().update_ref_for_fetch(remote, to)?;
        }

        let head_symref = capabilities.iter().find_map(|cap| match cap {
            &Capability::Symref(head, sym) if head == SymbolicRef::HEAD => Some(sym),
            _ => None,
        });

        Ok(FetchSummary { head_symref, status: fetch_status })
    }

    async fn negotiate_packs(
        &mut self,
        remote_mapping: &HashMap<SymbolicRef, Oid>,
    ) -> BitResult<FetchStatus> {
        let mut wanted = vec![];
        let mut local_tips = vec![];
        for (&remote, &remote_oid) in remote_mapping {
            let local_oid = self.repo().try_fully_resolve_ref(remote)?;
            if let Some(local_oid) = local_oid {
                local_tips.push(local_oid)
            }
            if local_oid != Some(remote_oid) {
                wanted.push(remote_oid);
            }
        }

        if wanted.is_empty() {
            return Ok(FetchStatus::UpToDate);
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

        let mut walk = self.repo().revwalk_builder().roots_iter(local_tips)?.build();
        'outer: loop {
            // TODO exit early when "ready" whatever that means
            for _ in 0..MULTI_ACK_BATCH_SIZE {
                let next_commit = match walk.next()? {
                    Some(commit) => commit,
                    None => {
                        self.write_flush_packet().await?;
                        self.recv_packet().await?;
                        break 'outer;
                    }
                };
                self.have(next_commit.oid()).await?;
            }
            self.write_flush_packet().await?;
            self.recv_packet().await?;
        }

        self.done().await?;
        let repo = self.repo().clone();
        self.recv_pack(repo).await?;
        Ok(FetchStatus::NotUpToDate)
    }

    async fn parse_ref_discovery_and_capabilities(
        &mut self,
    ) -> BitResult<(HashMap<SymbolicRef, Oid>, Capabilities)> {
        let packet = self.recv_packet().await?;
        let mut mapping = HashMap::new();

        // if we are cloning an empty repository, `git-upload-pack` just sends a flush
        if packet.is_empty() {
            return Ok((mapping, Capabilities::default()));
        }

        let s = std::str::from_utf8(&packet)?;
        let (ref_line, capabilities) =
            s.split_once('\0').ok_or_else(|| anyhow!("malformed first line"))?;
        let capabilities = capabilities.trim_end();
        let parsed_capabilities = capabilities
            .split_ascii_whitespace()
            .map(|capability| {
                capability.parse().or_else(|_| bail!("unknown capability `{}`", capability))
            })
            .collect::<BitResult<Capabilities>>()?;

        let (oid, sym) = parse_ref_line(ref_line)?;
        mapping.insert(sym, oid);

        loop {
            let packet = self.recv_packet().await?;
            if packet.is_empty() {
                break Ok((mapping, parsed_capabilities));
            }
            let (oid, sym) = parse_ref_line(std::str::from_utf8(&packet)?)?;
            mapping.insert(sym, oid);
        }
    }
}

fn parse_ref_line(s: &str) -> BitResult<(Oid, SymbolicRef)> {
    let (oid, sym) = s.split_once(' ').ok_or_else(|| anyhow!("malformed ref line"))?;
    Ok((oid.parse()?, sym.parse()?))
}
