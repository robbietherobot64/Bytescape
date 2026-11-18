use anyhow::Result;
use lexicon_cid::Cid;
use serde::Serialize;
use sha2::{Digest, Sha256};

const SHA2_256: u64 = 0x12;
const DAGCBORCODEC: u64 = 0x71;
const RAWCODEC: u64 = 0x77;

/// Inline multihash wrapping: varint(code) + varint(len) + digest
/// Replaces the yanked `multihash` crate dependency.
fn wrap_multihash(code: u64, digest: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(2 + digest.len());
    // varint encode code (0x12 = 18, fits in 1 byte)
    out.push(code as u8);
    // varint encode length (32, fits in 1 byte)
    out.push(digest.len() as u8);
    out.extend_from_slice(digest);
    Ok(out)
}

pub fn cid_for_cbor<T: Serialize>(data: &T) -> Result<Cid> {
    let bytes = crate::struct_to_cbor(data)?;
    let mut sha = Sha256::new();
    sha.update(&bytes);
    let hash = sha.finalize();
    let mh_bytes = wrap_multihash(SHA2_256, &hash)?;
    // Cid::new_v1 expects a multihash as raw bytes via from_reader or similar
    // Use Cid::read_bytes which parses the varint-encoded multihash
    let cid = Cid::new_v1(DAGCBORCODEC, lexicon_cid::multihash::MultihashGeneric::wrap(SHA2_256, &hash)?);
    Ok(cid)
}

pub fn sha256_to_cid(hash: Vec<u8>) -> Cid {
    let cid = Cid::new_v1(
        RAWCODEC,
        lexicon_cid::multihash::MultihashGeneric::wrap(SHA2_256, &hash).unwrap(),
    );
    cid
}
