use commonware_consensus::threshold_simplex::Prover;
use commonware_cryptography::{
    bls12381::primitives::{group, poly::Poly},
    ed25519::PublicKey,
    sha256::Digest,
};
use std::{collections::HashMap, sync::{Arc, Mutex}};
use alto_storage::{database::Database, transactional_db::{Key, Op}};

mod actor;
pub use actor::Actor;
mod ingress;
pub use ingress::Mailbox;
mod supervisor;
mod router;

pub use supervisor::Supervisor;

/// Configuration for the application.
pub struct Config {
    /// Prover used to decode opaque proofs from consensus.
    pub prover: Prover<Digest>,

    /// Participants active in consensus.
    pub participants: Vec<PublicKey>,

    pub identity: Poly<group::Public>,

    pub share: group::Share,

    /// Number of messages from consensus to hold in our backlog
    /// before blocking.
    pub mailbox_size: usize,

    /// State 
    pub state_cache: Arc<Mutex<HashMap<Key, Op>>>,
    pub unfinalized_state: Arc<Mutex<HashMap<Key, Op>>>,
    pub state_db: Arc<Mutex<dyn Database + Send + Sync>>,
}
