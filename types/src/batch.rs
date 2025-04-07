use std::hash::Hash;
use std::time::{Duration, SystemTime};

use bytes::BufMut;
use commonware_cryptography::Hasher;
use commonware_utils::{SizedSerialize, SystemTimeExt};
use bytes::Buf;

use crate::signed_tx::SignedTx;

#[derive(Clone, Debug)]
pub struct Batch<H: Hasher>  {
    pub timestamp: SystemTime,
    // TODO: store real transactions not just raws
    pub txs: Vec<SignedTx<H>>,
    pub digest: H::Digest,
}

impl<H: Hasher> SizedSerialize for Batch<H> {
    const SERIALIZED_LEN: usize = size_of::<u64>()*2;
}

impl<H: Hasher> Batch<H> {
    fn compute_digest(txs: &Vec<SignedTx<H>>) -> H::Digest {
        let mut hasher = H::new();

        for tx in txs.iter() {
            hasher.update(&tx.digest());
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
        if bytes.remaining() < Self::SERIALIZED_LEN {
            return Err(format!("not enough bytes for header"));
        }
        let timestamp = bytes.get_u64();
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(timestamp);

        let tx_count = bytes.get_u64();
        let mut txs = Vec::with_capacity(tx_count as usize);
        for _ in 0..tx_count {
            // For each transaction, first read the size (u64).
            if bytes.remaining() < size_of::<u64>() {
                return Err("not enough bytes for tx size".to_string());
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
        if bytes.remaining() != 0 {
            return Err(format!("left residue after decoding all the txs: {}", bytes.remaining()));
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
        self.txs.iter().any(|tx| &tx.digest == digest)
    }

    pub fn tx(&self, digest: &H::Digest) -> Option<SignedTx<H>> {
        self.txs.iter().find(|tx| &tx.digest == digest).map_or(None, |tx| Some(tx.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use commonware_cryptography::Sha256;
    use commonware_utils::SystemTimeExt;

    use crate::signed_tx::SignedTx;

    use super::Batch;

    #[test]
    fn test_encode_decode() {
        let tx = SignedTx::<Sha256>::random();
        let batch = Batch::new(vec![tx], SystemTime::now());
        let payload = batch.serialize();

        let batch_recover = Batch::<Sha256>::deserialize(&payload).unwrap();

        assert_eq!(batch.timestamp.epoch_millis(), batch_recover.timestamp.epoch_millis());
        assert_eq!(batch.txs.len(), batch_recover.txs.len());
        assert_eq!(batch.txs[0].digest, batch_recover.txs[0].digest);
        assert_eq!(batch.txs[0].payload(), batch_recover.txs[0].payload());
        assert_eq!(batch.digest, batch_recover.digest);
    }

    #[test]
    fn test_residue() {
        let tx = SignedTx::<Sha256>::random();
        let batch = Batch::new(vec![tx], SystemTime::now());
        let mut payload = batch.serialize();
        payload.push(10);

        let decode_result = Batch::<Sha256>::deserialize(&payload);

        let err_str = decode_result.map_err(|e| e.to_string()).err().unwrap();
        print!("{}\n", err_str);
        assert!(err_str.contains("left residue after decoding all the txs"));
    }
}