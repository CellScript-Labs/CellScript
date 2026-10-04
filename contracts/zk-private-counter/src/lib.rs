//! An authorizing single-Cell counter circuit. Setup provenance is separate
//! from circuit satisfaction, proof verification and local execution evidence.
use anyhow::{bail, ensure, Result};
use ark_bn254::{Bn254, Fr};
use ark_ff::PrimeField;
use ark_groth16::{Groth16, Proof, ProvingKey, VerifyingKey};
use ark_r1cs_std::{fields::fp::FpVar, prelude::*};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use ark_std::rand::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod blake2b;
pub mod package;
#[path = "../../../crates/cellscript-artifact-checker/src/zk.rs"]
pub mod wire;

pub const APPLICATION: &str = "cellscript-private-counter-v1";
pub const STATE_MAGIC: &[u8; 8] = b"CSZKCNT1";
pub const SECRET_DOMAIN: &[u8; 16] = b"CS-counter-key-1";
pub const STATE_BYTES: usize = 48;

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    let mut digest = [0; 32];
    let mut h = blake2b_ref::Blake2bBuilder::new(32).personal(b"ckb-default-hash").build();
    h.update(bytes);
    h.finalize(&mut digest);
    digest
}
pub fn domain() -> [u8; 32] {
    hash(b"cellscript-private-counter-v1/domain")
}
pub fn action() -> [u8; 32] {
    hash(b"cellscript-private-counter-v1/increment")
}
pub fn owner(secret: &[u8; 32]) -> [u8; 32] {
    let mut preimage = SECRET_DOMAIN.to_vec();
    preimage.extend(secret);
    hash(&preimage)
}
pub fn state(commitment: &[u8; 32], counter: u64) -> [u8; STATE_BYTES] {
    let mut data = [0; STATE_BYTES];
    data[..8].copy_from_slice(STATE_MAGIC);
    data[8..40].copy_from_slice(commitment);
    data[40..].copy_from_slice(&counter.to_le_bytes());
    data
}
pub fn parse_state(data: &[u8]) -> Result<([u8; 32], u64)> {
    ensure!(data.len() == STATE_BYTES && &data[..8] == STATE_MAGIC, "non-canonical counter state");
    Ok((data[8..40].try_into()?, u64::from_le_bytes(data[40..48].try_into()?)))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Witness {
    pub secret: [u8; 32],
    pub old_counter: u64,
    pub new_counter: u64,
}
#[derive(Clone)]
pub struct CounterCircuit {
    pub statement: wire::Statement,
    pub witness: Witness,
}

impl CounterCircuit {
    pub fn template() -> Self {
        let secret = [0; 32];
        Self {
            statement: wire::Statement {
                domain: domain(),
                action: action(),
                script_hash: [0; 32],
                old_data_hash: hash(&state(&owner(&secret), 0)),
                new_data_hash: hash(&state(&owner(&secret), 1)),
                input_transaction_hash: [0; 32],
                input_output_index: 0,
                transaction_hash: [0; 32],
            },
            witness: Witness { secret, old_counter: 0, new_counter: 1 },
        }
    }
    /// Independent native relation oracle, also used to reject invalid proving requests.
    pub fn validate(&self) -> Result<()> {
        ensure!(self.statement.domain == domain() && self.statement.action == action(), "counter domain/action mismatch");
        ensure!(self.witness.old_counter.checked_add(1) == Some(self.witness.new_counter), "counter must increment without overflow");
        let commitment = owner(&self.witness.secret);
        ensure!(self.statement.old_data_hash == hash(&state(&commitment, self.witness.old_counter)), "secret or old state mismatch");
        ensure!(self.statement.new_data_hash == hash(&state(&commitment, self.witness.new_counter)), "owner or new state mismatch");
        Ok(())
    }
}
pub fn public_inputs(statement: &wire::Statement) -> Vec<Fr> {
    statement.public_inputs()[4..].chunks_exact(32).map(Fr::from_le_bytes_mod_order).collect()
}
impl ConstraintSynthesizer<Fr> for CounterCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let encoded = self.statement.public_inputs();
        let mut fields = Vec::with_capacity(15);
        for (i, scalar) in encoded[4..].chunks_exact(32).enumerate() {
            let public = FpVar::new_input(cs.clone(), || Ok(Fr::from_le_bytes_mod_order(scalar)))?;
            let width = if i == 12 { 4 } else { 16 };
            let bytes = UInt8::new_witness_vec(cs.clone(), &scalar[..width])?;
            Boolean::le_bits_to_fp(&bytes.to_bits_le()?)?.enforce_equal(&public)?;
            fields.push(bytes);
        }
        // All public inputs have constrained canonical bit representations. Even
        // context fields not interpreted by this relation remain proof-bound.
        let pair = |index: usize| [fields[index].as_slice(), fields[index + 1].as_slice()].concat();
        pair(0).enforce_equal(&UInt8::constant_vec(&domain()))?;
        pair(2).enforce_equal(&UInt8::constant_vec(&action()))?;
        let secret = UInt8::new_witness_vec(cs.clone(), &self.witness.secret)?;
        let mut secret_preimage = UInt8::constant_vec(SECRET_DOMAIN);
        secret_preimage.extend(secret);
        let commitment = blake2b::ckb_hash(&secret_preimage)?;
        let before = UInt64::new_witness(cs.clone(), || Ok(self.witness.old_counter))?;
        let after = UInt64::new_witness(cs, || Ok(self.witness.new_counter))?;
        // Both integers are 64-bit, but equality is in a >65-bit field: this
        // rejects overflow rather than accepting modular u64 addition.
        (before.to_fp()? + Fr::from(1u64)).enforce_equal(&after.to_fp()?)?;
        for (counter, index) in [(&before, 6), (&after, 8)] {
            let mut preimage = UInt8::constant_vec(STATE_MAGIC);
            preimage.extend(commitment.clone());
            preimage.extend(counter.to_bytes_le()?);
            blake2b::ckb_hash(&preimage)?.enforce_equal(&pair(index))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CircuitIdentity {
    pub application: String,
    pub r1cs_sha256: String,
    pub constraints: usize,
    pub public_inputs: usize,
    pub witnesses: usize,
}
pub fn circuit_identity() -> Result<CircuitIdentity> {
    let cs = ConstraintSystem::<Fr>::new_ref();
    CounterCircuit::template().generate_constraints(cs.clone())?;
    cs.finalize();
    let matrices = cs.to_matrices().ok_or_else(|| anyhow::anyhow!("constraint matrices unavailable"))?;
    let mut digest = Sha256::new();
    digest.update(b"cellscript-counter-r1cs-v1");
    for value in [matrices.num_constraints, matrices.num_instance_variables, matrices.num_witness_variables] {
        digest.update((value as u64).to_le_bytes());
    }
    for matrix in [&matrices.a, &matrices.b, &matrices.c] {
        for row in matrix {
            digest.update((row.len() as u64).to_le_bytes());
            for (coefficient, variable) in row {
                digest.update((*variable as u64).to_le_bytes());
                digest.update(serialize(coefficient)?);
            }
        }
    }
    Ok(CircuitIdentity {
        application: APPLICATION.into(),
        r1cs_sha256: hex::encode(digest.finalize()),
        constraints: matrices.num_constraints,
        public_inputs: matrices.num_instance_variables - 1,
        witnesses: matrices.num_witness_variables,
    })
}
pub fn serialize(value: &impl CanonicalSerialize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value.serialize_compressed(&mut bytes)?;
    Ok(bytes)
}
pub fn decode_exact<T: CanonicalDeserialize>(bytes: &[u8]) -> Result<T> {
    let mut remaining = bytes;
    let value = T::deserialize_compressed(&mut remaining)?;
    ensure!(remaining.is_empty(), "trailing canonical artifact bytes");
    Ok(value)
}
/// This generates parameters; it does not establish setup trust or production admission.
pub fn setup(rng: &mut (impl RngCore + CryptoRng)) -> Result<(ProvingKey<Bn254>, VerifyingKey<Bn254>)> {
    Ok(Groth16::<Bn254>::circuit_specific_setup(CounterCircuit::template(), rng)?)
}
pub fn prove(pk: &ProvingKey<Bn254>, circuit: CounterCircuit, rng: &mut (impl RngCore + CryptoRng)) -> Result<[u8; 128]> {
    circuit.validate()?;
    let inputs = public_inputs(&circuit.statement);
    let proof = Groth16::<Bn254>::prove(pk, circuit, rng)?;
    ensure!(Groth16::<Bn254>::verify(&pk.vk, &inputs, &proof)?, "proof does not verify with supplied proving key");
    serialize(&proof)?.try_into().map_err(|_| anyhow::anyhow!("unexpected proof width"))
}
pub fn verify(vk: &[u8], statement: &wire::Statement, proof: &[u8]) -> Result<()> {
    ensure!(vk.len() == wire::VK_BYTES && proof.len() == wire::PROOF_BYTES, "wrong fixed artifact width");
    ensure!(vk[224..232] == 16u64.to_le_bytes(), "wrong VK input count");
    let vk: VerifyingKey<Bn254> = decode_exact(vk)?;
    let proof: Proof<Bn254> = decode_exact(proof)?;
    if !Groth16::<Bn254>::verify(&vk, &public_inputs(statement), &proof)? {
        bail!("invalid counter proof");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_ff::Field;
    fn satisfied(circuit: CounterCircuit) -> bool {
        let cs = ConstraintSystem::<Fr>::new_ref();
        circuit.generate_constraints(cs.clone()).unwrap();
        cs.is_satisfied().unwrap()
    }
    fn counter(before: u64, after: u64) -> CounterCircuit {
        let mut c = CounterCircuit::template();
        c.witness.old_counter = before;
        c.witness.new_counter = after;
        let owner = owner(&c.witness.secret);
        c.statement.old_data_hash = hash(&state(&owner, before));
        c.statement.new_data_hash = hash(&state(&owner, after));
        c
    }
    #[test]
    fn authorization_and_counter_constraints_reject_wrong_witnesses() {
        for before in [0, u64::MAX - 1] {
            let c = counter(before, before + 1);
            c.validate().unwrap();
            assert!(satisfied(c));
        }
        for c in [counter(0, 0), counter(0, 2), counter(u64::MAX, 0)] {
            assert!(c.validate().is_err());
            assert!(!satisfied(c));
        }
        let mut wrong_secret = counter(0, 1);
        wrong_secret.witness.secret[0] ^= 1;
        assert!(wrong_secret.validate().is_err());
        assert!(!satisfied(wrong_secret));
        let mut wrong_owner = counter(0, 1);
        wrong_owner.statement.new_data_hash = hash(&state(&owner(&[1; 32]), 1));
        assert!(wrong_owner.validate().is_err());
        assert!(!satisfied(wrong_owner));
        for field in [0, 1, 2, 3] {
            let mut c = counter(0, 1);
            match field {
                0 => c.statement.domain[0] ^= 1,
                1 => c.statement.action[0] ^= 1,
                2 => c.statement.old_data_hash[0] ^= 1,
                _ => c.statement.new_data_hash[0] ^= 1,
            }
            assert!(!satisfied(c));
        }
    }
    #[test]
    fn state_codec_and_public_input_ranges_are_exact() {
        let data = state(&[9; 32], u64::MAX);
        assert_eq!(parse_state(&data).unwrap(), ([9; 32], u64::MAX));
        assert!(parse_state(&data[..47]).is_err());
        let cs = ConstraintSystem::<Fr>::new_ref();
        CounterCircuit::template().generate_constraints(cs.clone()).unwrap();
        assert!(cs.is_satisfied().unwrap());
        // The first limb of script_hash is a context input rather than an
        // application equality. Its range/packing still must be constrained.
        cs.borrow_mut().unwrap().instance_assignment[5] += Fr::from(2u64).pow([128]);
        assert!(!cs.is_satisfied().unwrap());
    }
}
