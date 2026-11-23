use alloc::{vec, vec::Vec};

use ark_bn254::{Bn254, Fr, G1Affine};
use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup};
use ark_ff::{BigInteger, One, PrimeField};
use cellscript_zk_lock_plonkish_experiment::{canonical_be, g1, g2};
use mosaic_core::{
    syscall::{AltBn128Compress, AltBn128Op, InputEndianness, PoseidonParameters, SyscallBackend},
    OnChainError,
};
use tiny_keccak::{Hasher, Keccak};

pub struct Arkworks;

fn point_bytes(point: G1Affine) -> Vec<u8> {
    match point.xy() {
        Some((x, y)) => [x.into_bigint().to_bytes_be(), y.into_bigint().to_bytes_be()].concat(),
        None => vec![0; 64],
    }
}

impl SyscallBackend for Arkworks {
    fn alt_bn128_group_op(&self, op: AltBn128Op, endianness: InputEndianness, input: &[u8]) -> Result<Vec<u8>, OnChainError> {
        if endianness != InputEndianness::BigEndian {
            return Err(OnChainError::UnsupportedOperation);
        }
        let invalid = OnChainError::InvalidPointEncoding;
        match op {
            AltBn128Op::G1Add => {
                let (lhs, rhs) = input.split_first_chunk::<64>().ok_or(invalid)?;
                let sum = g1(lhs).ok_or(invalid)? + g1(rhs).ok_or(invalid)?;
                Ok(point_bytes(sum.into_affine()))
            }
            AltBn128Op::G1Mul => {
                let (point, scalar) = input.split_first_chunk::<64>().ok_or(invalid)?;
                let scalar: Fr = canonical_be(scalar).ok_or(OnChainError::InvalidFieldEncoding)?;
                Ok(point_bytes(g1(point).ok_or(invalid)?.mul_bigint(scalar.into_bigint()).into_affine()))
            }
            AltBn128Op::Pairing => {
                let ([first, second], []) = input.as_chunks::<192>() else {
                    return Err(invalid);
                };
                let (p1, q1) = first.split_at(64);
                let (p2, q2) = second.split_at(64);
                let output = Bn254::multi_pairing(
                    [g1(p1).ok_or(invalid)?, g1(p2).ok_or(invalid)?],
                    [g2(q1).ok_or(invalid)?, g2(q2).ok_or(invalid)?],
                );
                let mut result = vec![0; 32];
                result[31] = u8::from(output.0.is_one());
                Ok(result)
            }
        }
    }

    fn alt_bn128_compression(&self, _: AltBn128Compress, _: &[u8]) -> Result<Vec<u8>, OnChainError> {
        Err(OnChainError::UnsupportedOperation)
    }

    fn poseidon(&self, _: PoseidonParameters, _: InputEndianness, _: &[&[u8]]) -> Result<[u8; 32], OnChainError> {
        Err(OnChainError::UnsupportedOperation)
    }

    fn sha256(&self, _: &[&[u8]]) -> Result<[u8; 32], OnChainError> {
        Err(OnChainError::UnsupportedOperation)
    }

    fn keccak256(&self, inputs: &[&[u8]]) -> Result<[u8; 32], OnChainError> {
        let mut hasher = Keccak::v256();
        for bytes in inputs {
            hasher.update(bytes);
        }
        let mut output = [0; 32];
        hasher.finalize(&mut output);
        Ok(output)
    }
}
