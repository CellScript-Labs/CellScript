#![no_std]

use ark_bn254::{Fq2, G1Affine, G2Affine};
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, PrimeField};

pub const GROTH16_PROOF_BYTES: usize = 128;
pub const PLONK_PROOF_BYTES: usize = 768;
pub const PLONK_VK_BYTES: usize = 744;
pub const CONTEXT_MISMATCH: i8 = 22;
pub const VERIFICATION_FAILED: i8 = 20;
pub const DEMO_DIGEST: &str = "12326503012965816391338144612242952408728683609716147019497703475006801258307";

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut output = [0; 32];
    let mut hasher = blake2b_ref::Blake2bBuilder::new(32).personal(b"ckb-default-hash").build();
    hasher.update(bytes);
    hasher.finalize(&mut output);
    output
}

pub fn context_scalar(mut raw_transaction_hash: [u8; 32]) -> [u8; 32] {
    raw_transaction_hash[31] = 0;
    raw_transaction_hash
}

pub fn canonical_be<F: PrimeField>(bytes: &[u8]) -> Option<F> {
    if bytes.len() != 32 {
        return None;
    }
    let value = F::from_be_bytes_mod_order(bytes);
    (value.into_bigint().to_bytes_be() == bytes).then_some(value)
}

pub fn g1(bytes: &[u8]) -> Option<G1Affine> {
    let ([x, y], []) = bytes.as_chunks::<32>() else {
        return None;
    };
    if *x == [0; 32] && *y == [0; 32] {
        return Some(G1Affine::zero());
    }
    let point = G1Affine::new_unchecked(canonical_be(x)?, canonical_be(y)?);
    (point.is_on_curve() && point.is_in_correct_subgroup_assuming_on_curve()).then_some(point)
}

pub fn g2(bytes: &[u8]) -> Option<G2Affine> {
    let ([x1, x0, y1, y0], []) = bytes.as_chunks::<32>() else {
        return None;
    };
    let point =
        G2Affine::new_unchecked(Fq2::new(canonical_be(x0)?, canonical_be(x1)?), Fq2::new(canonical_be(y0)?, canonical_be(y1)?));
    (point.is_on_curve() && point.is_in_correct_subgroup_assuming_on_curve()).then_some(point)
}
