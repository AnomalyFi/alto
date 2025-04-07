use commonware_cryptography::Digest;
use commonware_utils::{Array, SizedSerialize};
use std::{
    cmp::{Ord, PartialOrd}, fmt::{Debug, Display}, hash::Hash, marker::PhantomData, ops::Deref
};
use thiserror::Error;

// to resolve issue of https://github.com/rust-lang/rust/issues/76560
// the first byte of MultiIndex indicates the index type
// the rest bytes stores key for that type, e.g. a sha256 index would be [0 | digest(32) | rest(31)]
const SERIALIZED_LEN: usize = 64;

#[derive(Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid length")]
    InvalidLength,
}

pub enum Value<D: Digest> {
    Digest(D),
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct MultiIndex<D: Digest> {
    index: [u8; SERIALIZED_LEN],

    _marker: PhantomData<D>
}


impl<D: Digest> MultiIndex<D> {
    const DIGEST_LENGTH: usize = D::SERIALIZED_LEN;

    pub fn new(value: Value<D>) -> Self {
        let mut bytes = [0; SERIALIZED_LEN];
        match value {
            Value::Digest(digest) => {
                bytes[0] = 0;
                bytes[1..(1+D::SERIALIZED_LEN)].copy_from_slice(&digest);
            }
        }
        Self {
            index: bytes,

            _marker: PhantomData
        }
    }

    pub fn to_value(&self) -> Value<D> {
        match self.index[0] {
            0 => {
                let bytes: Vec<u8> = self.index[1..(1+Self::DIGEST_LENGTH)].to_vec();
                let digest = D::try_from(bytes).unwrap();
                Value::Digest(digest)
            }
            _ => unreachable!(),
        }
    }
}

impl<D: Digest> Array for MultiIndex<D> {
    type Error = Error;
}

impl<D: Digest> SizedSerialize for MultiIndex<D> {
    const SERIALIZED_LEN: usize = SERIALIZED_LEN;
}

impl<D: Digest> From<[u8; SERIALIZED_LEN]> for MultiIndex<D> {
    fn from(value: [u8; SERIALIZED_LEN]) -> Self {

        Self {
            index: value,

            _marker: PhantomData
        }
    }
}

impl<D: Digest> TryFrom<&[u8]> for MultiIndex<D> {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != SERIALIZED_LEN {
            return Err(Error::InvalidLength);
        }
        let array: [u8; SERIALIZED_LEN] =
            value.try_into().map_err(|_| Error::InvalidLength)?;
        Ok(Self{
            index: array,
            _marker: PhantomData
        })
    }
}

impl<D: Digest> TryFrom<&Vec<u8>> for MultiIndex<D> {
    type Error = Error;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        Self::try_from(value.as_slice())
    }
}

impl<D: Digest> TryFrom<Vec<u8>> for MultiIndex<D> {
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() != SERIALIZED_LEN {
            return Err(Error::InvalidLength);
        }

        // If the length is correct, we can safely convert the vector into a boxed slice without any
        // copies.
        let boxed_slice = value.into_boxed_slice();
        let boxed_array: Box<[u8; SERIALIZED_LEN]> =
            boxed_slice.try_into().map_err(|_| Error::InvalidLength)?;
        Ok(Self {
            index: *boxed_array,
            _marker: PhantomData
        })
    }
}

impl<D: Digest> AsRef<[u8]> for MultiIndex<D> {
    fn as_ref(&self) -> &[u8] {
        &self.index
    }
}

impl<D: Digest> Deref for MultiIndex<D> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.index
    }
}

impl<D: Digest> Debug for MultiIndex<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.index[0] {
            0 => {
                let bytes: Vec<u8> = self.index[1..(1+D::SERIALIZED_LEN)].to_vec();
                write!(f, "digest({})", D::try_from(bytes).unwrap())
            }
            _ => unreachable!(),
        }
    }
}

impl<D: Digest> Display for MultiIndex<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
