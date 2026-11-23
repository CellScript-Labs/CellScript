use anyhow::{anyhow, ensure, Context as _, Result};
use ark_bn254::Bn254;
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::CanonicalSerialize;
use cellscript_zk_lock_plonkish_experiment::{g1, g2, GROTH16_PROOF_BYTES, PLONK_PROOF_BYTES, PLONK_VK_BYTES};
use mosaic_core::codec::ProofCodec;
use mosaic_serde::snarkjs::{SnarkjsCodec, SnarkjsPlonkCodec};

#[derive(Clone, Copy)]
pub enum Scheme {
    Groth16,
    Plonk,
}

impl Scheme {
    pub fn name(self) -> &'static str {
        match self {
            Self::Groth16 => "groth16",
            Self::Plonk => "plonk",
        }
    }

    pub fn key(self, json: &[u8], public_count: u32) -> Result<Vec<u8>> {
        match self {
            Self::Plonk => {
                let key = codec(SnarkjsPlonkCodec::decode_vk(json))?;
                ensure!(
                    key.len() == PLONK_VK_BYTES && key.ends_with(&public_count.to_le_bytes()),
                    "PLONK key public-input count mismatch"
                );
                Ok(key)
            }
            Self::Groth16 => {
                let raw = codec(SnarkjsCodec.decode_vk(json))?;
                ensure!(raw.len() == 448 + 64 * (public_count as usize + 1), "Groth16 key public-input count mismatch");
                let vk = VerifyingKey::<Bn254> {
                    alpha_g1: g1(&raw[..64]).context("invalid Groth16 alpha")?,
                    beta_g2: g2(&raw[64..192]).context("invalid Groth16 beta")?,
                    gamma_g2: g2(&raw[192..320]).context("invalid Groth16 gamma")?,
                    delta_g2: g2(&raw[320..448]).context("invalid Groth16 delta")?,
                    gamma_abc_g1: raw[448..]
                        .chunks_exact(64)
                        .map(|bytes| g1(bytes).context("invalid Groth16 IC"))
                        .collect::<Result<_>>()?,
                };
                serialize(&vk)
            }
        }
    }

    pub fn proof(self, json: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::Plonk => {
                let proof = codec(SnarkjsPlonkCodec::decode_proof(json))?;
                ensure!(proof.len() == PLONK_PROOF_BYTES, "unexpected PLONK proof width");
                Ok(proof)
            }
            Self::Groth16 => {
                let raw = codec(SnarkjsCodec.decode_proof(json))?;
                ensure!(raw.len() == 256, "unexpected Groth16 proof wire width");
                let proof = Proof::<Bn254> {
                    a: g1(&raw[..64]).context("invalid Groth16 A")?,
                    b: g2(&raw[64..192]).context("invalid Groth16 B")?,
                    c: g1(&raw[192..]).context("invalid Groth16 C")?,
                };
                let proof = serialize(&proof)?;
                ensure!(proof.len() == GROTH16_PROOF_BYTES, "unexpected Groth16 proof width");
                Ok(proof)
            }
        }
    }
}

fn codec<T>(result: std::result::Result<T, mosaic_core::OnChainError>) -> Result<T> {
    result.map_err(|error| anyhow!("snarkjs serialization rejected: {error:?}"))
}

fn serialize(value: &impl CanonicalSerialize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value.serialize_compressed(&mut bytes).map_err(|error| anyhow!("Groth16 serialization failed: {error}"))?;
    Ok(bytes)
}
