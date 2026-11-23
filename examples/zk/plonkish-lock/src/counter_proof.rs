//! Counter witness codec. Mosaic's internal transcript uses its original
//! big-endian affine encoding, including the all-zero identity point.

use cellscript_zk_lock_plonkish_experiment::PLONK_PROOF_BYTES;

const POINTS: usize = 9;
const AFFINE_BYTES: usize = 64;
const COMPRESSED_BYTES: usize = 32;
const EVALUATIONS_OFFSET: usize = POINTS * AFFINE_BYTES;
const COMPRESSED_EVALUATIONS_OFFSET: usize = POINTS * COMPRESSED_BYTES;
pub const COMPACT_PROOF_BYTES: usize = COMPRESSED_EVALUATIONS_OFFSET + PLONK_PROOF_BYTES - EVALUATIONS_OFFSET;

#[cfg(any(feature = "counter", test))]
pub fn compress(proof: &[u8; PLONK_PROOF_BYTES]) -> Option<[u8; COMPACT_PROOF_BYTES]> {
    use ark_serialize::CanonicalSerialize;
    use cellscript_zk_lock_plonkish_experiment::g1;

    let (points, evaluations) = proof.split_at(EVALUATIONS_OFFSET);
    let mut compact = [0; COMPACT_PROOF_BYTES];
    let (encoded, tail) = compact.split_at_mut(COMPRESSED_EVALUATIONS_OFFSET);
    for (point, slot) in points.chunks_exact(AFFINE_BYTES).zip(encoded.chunks_exact_mut(COMPRESSED_BYTES)) {
        g1(point)?.serialize_compressed(slot).ok()?;
    }
    tail.copy_from_slice(evaluations);
    Some(compact)
}

#[cfg(any(all(feature = "counter-compact", not(feature = "counter-groth16")), test))]
pub fn decompress(proof: &[u8; COMPACT_PROOF_BYTES]) -> Option<[u8; PLONK_PROOF_BYTES]> {
    use ark_bn254::G1Affine;
    use ark_ec::AffineRepr;
    use ark_ff::PrimeField;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

    let (points, evaluations) = proof.split_at(COMPRESSED_EVALUATIONS_OFFSET);
    let mut canonical = [0; PLONK_PROOF_BYTES];
    let (decoded, tail) = canonical.split_at_mut(EVALUATIONS_OFFSET);
    for (encoded, slot) in points.chunks_exact(COMPRESSED_BYTES).zip(decoded.chunks_exact_mut(AFFINE_BYTES)) {
        let point = G1Affine::deserialize_compressed(encoded).ok()?;
        // Reject alternate encodings of the identity and any other aliases.
        let mut checked = [0; COMPRESSED_BYTES];
        point.serialize_compressed(checked.as_mut_slice()).ok()?;
        if encoded != checked {
            return None;
        }
        if let Some((x, y)) = point.xy() {
            for (limb, bytes) in x.into_bigint().0.iter().rev().chain(y.into_bigint().0.iter().rev()).zip(slot.chunks_exact_mut(8)) {
                bytes.copy_from_slice(&limb.to_be_bytes());
            }
        }
    }
    tail.copy_from_slice(evaluations);
    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_points_preserve_the_affine_transcript() {
        let mut proof = [0; PLONK_PROOF_BYTES];
        // G1 generator (1, 2), followed by identity points.
        proof[31] = 1;
        proof[63] = 2;
        proof[EVALUATIONS_OFFSET..].fill(0x2a);
        let compact = compress(&proof).unwrap();
        assert_eq!(compact.len(), 480);
        assert_eq!(decompress(&compact), Some(proof));
    }

    #[test]
    fn compressed_identity_has_one_representation() {
        let mut compact = compress(&[0; PLONK_PROOF_BYTES]).unwrap();
        assert_eq!(compact[31], 0x40);
        compact[0] = 1;
        assert_eq!(decompress(&compact), None);
    }
}
