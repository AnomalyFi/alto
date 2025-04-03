use std::time::{Duration, SystemTime};

use bytes::BufMut;
use commonware_cryptography::Hasher;
use commonware_utils::SystemTimeExt;

use crate::signed_tx::SignedTx;

#[derive(Clone, Debug)]
pub struct Batch<H: Hasher>  {
    pub timestamp: SystemTime,
    // TODO: store real transactions not just raws
    pub txs: Vec<SignedTx<H>>,
    pub digest: H::Digest,
}

impl<H: Hasher> Batch<H> {
    fn compute_digest(txs: &Vec<SignedTx<H>>) -> H::Digest {
        let mut hasher = H::new();

        for tx in txs.iter() {
            hasher.update(&tx.payload());
        }

        hasher.finalize()
    }

    pub fn new(txs: Vec<SignedTx<H>>, timestamp: SystemTime) -> Self {
        let digest = Self::compute_digest(&txs);

        Self {
            txs,
            digest,
            timestamp 
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.put_u64(self.timestamp.epoch_millis());
        bytes.put_u64(self.txs.len() as u64);
        for tx in self.txs.iter() {
            bytes.put_u64(tx.size() as u64);
            bytes.extend_from_slice(&tx.payload());
        }
        bytes
    }

    pub fn deserialize(mut bytes: &[u8]) -> Result<Self, String> {
        use bytes::Buf;
        // We expect at least 18 bytes for the header
        if bytes.remaining() < 18 {
            return Err(format!("not enough bytes for header"));
        }
        let timestamp = bytes.get_u64();
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(timestamp);

        let tx_count = bytes.get_u64();
        let mut txs = Vec::with_capacity(tx_count as usize);
        for _ in 0..tx_count {
            // For each transaction, first read the size (u64).
            if bytes.remaining() < 8 {
                return Err(format!("not enough bytes for tx size"));
            }
            let tx_size = bytes.get_u64() as usize;
            // Ensure there are enough bytes left.
            if bytes.remaining() < tx_size {
                return Err(format!("not enough bytes for tx payload, needed: {}, actual: {}", tx_size, bytes.remaining()));
            }
            // Extract tx_size bytes.
            let tx_bytes = bytes.copy_to_bytes(tx_size);
            txs.push(SignedTx::deserialize(&tx_bytes)?);
        }
        // Compute the digest from the transactions.
        let digest = Self::compute_digest(&txs);
        // Since serialize did not include accepted and timestamp, we set accepted to false
        // and set timestamp to the current time.
        Ok(Self {
            timestamp,
            txs,
            digest,
        })
    }

    pub fn contain_tx(&self, digest: &H::Digest) -> bool {
        todo!()
        // self.txs.iter().any(|tx| &tx.digest == digest) 
    }

    pub fn tx(&self, digest: &H::Digest) -> Option<SignedTx<H>> {
        // self.txs.iter().find(|tx| &tx.digest == digest).cloned()
        todo!()
    }
}