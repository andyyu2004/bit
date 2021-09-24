mod file;
mod ssh;

pub use file::*;
pub use ssh::*;

use crate::error::BitResult;
use crate::obj::Oid;
use crate::remote::Remote;
use crate::repo::BitRepo;

#[async_trait]
pub trait Transport {
    async fn send(&mut self, bytes: &[u8]) -> BitResult<()>;

    async fn fetch(&mut self, repo: BitRepo<'_>) -> BitResult<()> {
        self.want(Oid::UNKNOWN).await
    }

    async fn want(&mut self, oid: Oid) -> BitResult<()> {
        self.send(format!("want {}", oid).as_bytes()).await
    }

    async fn have(&mut self, oid: Oid) -> BitResult<()> {
        self.send(format!("have {}", oid).as_bytes()).await
    }
}
