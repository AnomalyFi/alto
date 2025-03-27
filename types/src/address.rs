use more_asserts::assert_le;
use crate::ADDRESSLEN;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct Address(pub [u8;ADDRESSLEN]);

impl Address {
    pub fn new(slice: &[u8]) -> Self {
        assert_le!(slice.len(), ADDRESSLEN, "address slice is too large");
        let mut arr = [0u8; ADDRESSLEN];
        arr[..slice.len()].copy_from_slice(slice);
        Address(arr)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() != 32 {
            return Err("Address must be 32 bytes.");
        }

        Ok(Address(<[u8; 32]>::try_from(bytes.clone()).unwrap()))
    }

    pub fn empty() -> Self {
        Self([0;ADDRESSLEN])
    }

    pub fn is_empty(&self) -> bool {
        self.0 == Self::empty().0
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8;ADDRESSLEN] {
        &self.0
    }
}