use std::hash::Hash;

use crate::signed_tx::{pack_signed_txs, unpack_signed_txs, SignedTx};
use crate::{batch, Batch, Finalization, Notarization};
use bytes::{Buf, BufMut};
use commonware_cryptography::{bls12381::PublicKey, sha256, sha256::Digest as Sha256Digest, Hasher, Sha256};
use commonware_utils::{Array, SizedSerialize};

// @todo add state root, fee manager and results to the block struct.
// what method of state root generation should be used?
#[derive(Clone, Debug)]
pub struct Block {
    /// The parent block's digest.
    pub parent: Sha256Digest,

    /// The height of the block in the blockchain.
    pub height: u64,

    /// The timestamp of the block (in milliseconds since the Unix epoch).
    pub timestamp: u64,

    /// The state root of the block.
    pub state_root: Sha256Digest,

    pub batches: Vec<Sha256Digest>,

    _batches: Vec<Batch<Sha256>>,

    /// Pre-computed digest of the block.
    digest: Sha256Digest,
}

impl Block {
    fn compute_digest(
        parent: &Sha256Digest,
        height: u64,
        timestamp: u64,
        batch_digests: &Vec<Sha256Digest>,
        state_root: &Sha256Digest,
    ) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(parent);
        hasher.update(&height.to_be_bytes());
        hasher.update(&timestamp.to_be_bytes());
        for digest in batch_digests.iter() {
            hasher.update(digest);
        }
        hasher.update(state_root);
        hasher.finalize()
    }

    pub fn new(
        parent: Sha256Digest,
        height: u64,
        timestamp: u64,
        batches: Vec<Batch<Sha256>>,
        state_root: Sha256Digest,
    ) -> Self {
        // let mut txs = txs;
        // @todo this is packing txs in a block.
        let batch_digests = batches.iter().map(|batch| batch.digest).collect();

        let digest = Self::compute_digest(&parent, height, timestamp, &batch_digests, &state_root);
        Self {
            parent,
            height,
            timestamp,
            state_root,
            batches: batch_digests,
            _batches: batches,
            digest,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SERIALIZED_LEN);
        bytes.extend_from_slice(&self.parent);
        bytes.put_u64(self.height);
        bytes.put_u64(self.timestamp);
        bytes.extend_from_slice(&self.state_root);
        bytes.put_u64(self.batches.len() as u64);
        for digest in self.batches.iter() {
            bytes.extend_from_slice(&digest);
        }
        bytes
    }

    pub fn deserialize(mut bytes: &[u8]) -> Option<Self> {
        // Parse the block
        // if bytes.len() != Self::SERIALIZED_LEN {
        //     return None;
        // }
        let parent = Sha256Digest::read_from(&mut bytes).ok()?;
        let height = bytes.get_u64();
        let timestamp = bytes.get_u64();
        let state_root = Sha256Digest::read_from(&mut bytes).ok()?;
        let num_batches = bytes.get_u64();
        let mut batch_digests = Vec::with_capacity(num_batches as usize);
        for _ in 0..num_batches {
            let batch_digest = Sha256Digest::read_from(&mut bytes).ok()?;
            batch_digests.push(batch_digest);
        }

        let digest = Self::compute_digest(&parent, height, timestamp, &batch_digests, &state_root);

        // Return block
        Some(Self {
            parent,
            height,
            timestamp,
            state_root,
            batches: batch_digests,
            _batches: vec![],
            digest,
        })
    }

    pub fn digest(&self) -> Sha256Digest {
        self.digest.clone()
    }
}

// TODO: this should be an estimate of the size of one block since batches size can be variable
impl SizedSerialize for Block {
    // there is an assumed factor `5` multiply by Sha256Digest::SERIALIZED_LEN, which is an estimate how average many batches will be included in one block
    const SERIALIZED_LEN: usize =
        Sha256Digest::SERIALIZED_LEN + u64::SERIALIZED_LEN + u64::SERIALIZED_LEN + Sha256Digest::SERIALIZED_LEN + 5 * Sha256Digest::SERIALIZED_LEN;
}

pub struct Notarized {
    pub proof: Notarization,
    pub block: Block,
}

impl Notarized {
    pub fn new(proof: Notarization, block: Block) -> Self {
        Self { proof, block }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let block = self.block.serialize();
        let mut bytes = Vec::with_capacity(Notarization::SERIALIZED_LEN + block.len());
        bytes.extend_from_slice(&self.proof.serialize());
        bytes.extend_from_slice(&block);
        bytes
    }

    pub fn deserialize(public: Option<&PublicKey>, bytes: &[u8]) -> Option<Self> {
        // Deserialize the proof and block
        let (proof, block) = bytes.split_at_checked(Notarization::SERIALIZED_LEN)?;
        let proof = Notarization::deserialize(public, proof)?;
        let block = Block::deserialize(block)?;

        // Ensure the proof is for the block
        if proof.payload != block.digest() {
            return None;
        }
        Some(Self { proof, block })
    }
}

pub struct Finalized {
    pub proof: Finalization,
    pub block: Block,
}

impl Finalized {
    pub fn new(proof: Finalization, block: Block) -> Self {
        Self { proof, block }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let block = self.block.serialize();
        let mut bytes = Vec::with_capacity(Finalization::SERIALIZED_LEN + block.len());
        bytes.extend_from_slice(&self.proof.serialize());
        bytes.extend_from_slice(&block);
        bytes
    }

    pub fn deserialize(public: Option<&PublicKey>, bytes: &[u8]) -> Option<Self> {
        // Deserialize the proof and block
        let (proof, block) = bytes.split_at_checked(Finalization::SERIALIZED_LEN)?;
        let proof = Finalization::deserialize(public, proof)?;
        let block = Block::deserialize(block)?;

        // Ensure the proof is for the block
        if proof.payload != block.digest() {
            return None;
        }
        Some(Self { proof, block })
    }
}