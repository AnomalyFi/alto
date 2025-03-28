use commonware_codec::Writer;
use commonware_cryptography::ed25519::PublicKey;

pub fn serialize_pk(pk: &PublicKey, writer: &mut impl Writer) {
    // let slice: &[u8] = pk.as_ref();
    // let length = slice.len();
    // length.serialize(&mut *writer).unwrap();
}