//! Single-block BLAKE2b-256 gadget, using the CKB personalization.
//! Algorithm and schedule: RFC 7693, sections 2–3. The fixed application
//! preimages are <=128 bytes; multi-block inputs are deliberately unsupported.
use ark_bn254::Fr;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::SynthesisError;

const IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];
fn mix(v: &mut [UInt64<Fr>], indices: [usize; 4], x: &UInt64<Fr>, y: &UInt64<Fr>) -> Result<(), SynthesisError> {
    let [a, b, c, d] = indices;
    v[a] = UInt64::wrapping_add_many(&[v[a].clone(), v[b].clone(), x.clone()])?;
    v[d] = (&v[d] ^ &v[a]).rotate_right(32);
    v[c] = UInt64::wrapping_add_many(&[v[c].clone(), v[d].clone()])?;
    v[b] = (&v[b] ^ &v[c]).rotate_right(24);
    v[a] = UInt64::wrapping_add_many(&[v[a].clone(), v[b].clone(), y.clone()])?;
    v[d] = (&v[d] ^ &v[a]).rotate_right(16);
    v[c] = UInt64::wrapping_add_many(&[v[c].clone(), v[d].clone()])?;
    v[b] = (&v[b] ^ &v[c]).rotate_right(63);
    Ok(())
}
pub fn ckb_hash(input: &[UInt8<Fr>]) -> Result<Vec<UInt8<Fr>>, SynthesisError> {
    if input.len() > 128 {
        return Err(SynthesisError::Unsatisfiable);
    }
    let mut parameters = IV;
    parameters[0] ^= 0x01010020; // digest=32, key=0, fanout=1, depth=1
    parameters[6] ^= u64::from_le_bytes(*b"ckb-defa");
    parameters[7] ^= u64::from_le_bytes(*b"ult-hash");
    let mut block = input.to_vec();
    block.resize(128, UInt8::constant(0));
    let words = block
        .chunks_exact(8)
        .map(|chunk| Ok(UInt64::from_bits_le(&chunk.to_bits_le()?)))
        .collect::<Result<Vec<_>, SynthesisError>>()?;
    let initial = parameters.map(UInt64::constant);
    let mut v = initial.to_vec();
    v.extend(IV.map(UInt64::constant));
    v[12] = &v[12] ^ UInt64::constant(input.len() as u64);
    v[14] = &v[14] ^ UInt64::constant(u64::MAX);
    for round in 0..12 {
        let s = SIGMA[round % 10];
        for (i, indices) in [
            [0, 4, 8, 12],
            [1, 5, 9, 13],
            [2, 6, 10, 14],
            [3, 7, 11, 15],
            [0, 5, 10, 15],
            [1, 6, 11, 12],
            [2, 7, 8, 13],
            [3, 4, 9, 14],
        ]
        .into_iter()
        .enumerate()
        {
            mix(&mut v, indices, &words[s[2 * i]], &words[s[2 * i + 1]])?;
        }
    }
    let mut output = Vec::with_capacity(32);
    for i in 0..4 {
        output.extend((&initial[i] ^ &v[i] ^ &v[i + 8]).to_bytes_le()?);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_relations::r1cs::ConstraintSystem;
    #[test]
    fn personalized_hash_matches_native_at_boundaries_and_rejects_changed_output() {
        for length in [0, 1, 32, 48, 64, 127, 128] {
            let bytes: Vec<_> = (0..length).map(|i| (i * 197 + 17) as u8).collect();
            let cs = ConstraintSystem::<Fr>::new_ref();
            let input = UInt8::new_witness_vec(cs.clone(), &bytes).unwrap();
            let digest = ckb_hash(&input).unwrap();
            let expected = crate::hash(&bytes);
            digest.enforce_equal(&UInt8::constant_vec(&expected)).unwrap();
            assert!(cs.is_satisfied().unwrap(), "length {length}");
            assert_eq!(digest.value().unwrap(), expected);
            if length == 48 {
                let mut wrong = expected;
                wrong[31] ^= 1;
                digest.enforce_equal(&UInt8::constant_vec(&wrong)).unwrap();
                assert!(!cs.is_satisfied().unwrap());
            }
        }
    }
}
