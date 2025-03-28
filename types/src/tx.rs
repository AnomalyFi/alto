use crate::address::Address;
use crate::wallet::Wallet;
use crate::signed_tx::SignedTx;
use crate::state::State;

pub enum UnitType {
    Transfer,
    SequencerMsg,
}


pub struct UnitContext {
    pub timestamp: u64, // timestamp of the tx.
    pub chain_id: u64, // chain id of the tx.
    pub sender: Address, // sender of the tx.
}


// unit need to be simple and easy to be packed in the tx and executed by the vm.
pub trait Unit : Send + Sync {
    fn unit_type(&self) -> UnitType;
    fn encode(&self) -> Vec<u8>;
    fn decode(&mut self, bytes: &[u8]);

    fn apply(
        &self,
        context: &UnitContext,
        state: &mut Box<dyn State>,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>>;
}


// pub struct Tx<'a, U: Unit> {
pub struct Tx<'a> {
    // timestamp of the tx creation. set by the user.
    // will be verified if the tx is in the valid window once received by validators.
    // if the timestamp is not in the valid window, the tx will be rejected.
    // if tx is in a valid window it is added to mempool.
    // timestamp is used to prevent replay attacks. and counter infinite spam attacks as Tx does not have nonce.
    pub timestamp: u64,
    // units are fundamental unit of a tx. similar to actions.
    pub units: Vec<Box<dyn Unit>>,
    // max fee is the maximum fee the user is willing to pay for the tx.
    pub max_fee: u64,
    // priority fee is the fee the user is willing to pay for the tx to be included in the next block.
    pub priority_fee: u64,
    // chain id is the id of the chain the tx is intended for.
    pub chain_id: u64,


    // id is the transaction id. It is the hash of digest.
    id: &'a [u8;32],
    // digest is encoded tx.
    digest: Vec<u8>,
}


pub trait TxChars {
    // init is used to create a new instance of Tx.
    fn init() -> Self;
    // new is used to create a new instance of Tx with given units and chain id.
    fn new(units: Vec<Box<dyn Unit>>, chain_id: u64) -> Self;
    // set_fee is used to set the max fee and priority fee of the tx.
    fn set_fee(&mut self, max_fee: u64, priority_fee: u64);
    // sign is used to sign the tx with the given wallet.
    fn sign(&self, wallet: Wallet) -> SignedTx;


    // returns tx id.
    fn id(&self) -> &[u8;32];
    // returns digest of the tx.
    fn digest(&self) -> Vec<u8>;
    // encodes the tx, writes to digest and returns the digest.
    fn encode(&mut self) -> Vec<u8>;


    fn decode(bytes: &[u8]) -> Self;
}
