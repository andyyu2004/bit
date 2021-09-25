mod file;
mod ssh;

pub use file::*;
pub use ssh::*;

use crate::error::BitResult;
use crate::protocol::{BitProtocolRead, BitProtocolWrite};
use crate::remote::Remote;
use crate::repo::BitRepo;

#[async_trait]
pub trait Transport: BitProtocolRead + BitProtocolWrite {
    async fn fetch(&mut self, _repo: BitRepo<'_>) -> BitResult<()> {
        let headers = self.recv_packets().await?;
        dbg!(headers);
        panic!();
    }
}
