#![no_main]
#![no_std]

use ckb_std::{
    ckb_constants::Source,
    ckb_types::{packed::WitnessArgs, prelude::*},
    env,
    high_level::{load_cell_data_hash, load_input_out_point, load_script_hash, load_tx_hash},
    syscalls,
};

#[cfg(not(feature = "counter-groth16"))]
mod backend;
// Use the counter circuit's exact statement and scalar encoding.
#[path = "../../../../crates/cellscript-artifact-checker/src/zk.rs"]
#[allow(dead_code)]
mod wire;

ckb_std::entry!(program_entry);
ckb_std::default_alloc!(16_384, 1_258_306, 64);

const BAD_ARGS: i8 = 10;
const BAD_WITNESS: i8 = 12;
const BAD_KEY: i8 = 14;
const BAD_CONTEXT: i8 = 16;
const BAD_PROOF: i8 = 20;

#[cfg(feature = "counter-groth16")]
const PROOF_BYTES: usize = wire::PROOF_BYTES;
#[cfg(feature = "counter-groth16")]
const KEY_BYTES: usize = wire::VK_BYTES;
#[cfg(not(feature = "counter-groth16"))]
use cellscript_zk_lock_plonkish_experiment::{PLONK_PROOF_BYTES as PROOF_BYTES, PLONK_VK_BYTES as KEY_BYTES};

fn decode_hex(bytes: &[u8]) -> Result<[u8; 32], i8> {
    fn digit(byte: u8) -> Result<u8, i8> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(BAD_ARGS),
        }
    }
    let (pairs, []) = bytes.as_chunks::<2>() else {
        return Err(BAD_ARGS);
    };
    let pairs: &[[u8; 2]; 32] = pairs.try_into().map_err(|_| BAD_ARGS)?;
    let mut output = [0; 32];
    for (byte, &[high, low]) in output.iter_mut().zip(pairs) {
        *byte = (digit(high)? << 4) | digit(low)?;
    }
    Ok(output)
}

fn statement() -> Result<wire::Statement, i8> {
    let [domain, action, arg2, arg3] = env::argv() else {
        return Err(BAD_ARGS);
    };
    if !arg2.to_bytes().is_empty() || !arg3.to_bytes().is_empty() {
        return Err(BAD_ARGS);
    }
    let input = load_input_out_point(0, Source::GroupInput).map_err(|_| BAD_CONTEXT)?;
    let input_transaction_hash = input.tx_hash().as_slice().try_into().map_err(|_| BAD_CONTEXT)?;
    let input_output_index = u32::from_le_bytes(input.index().as_slice().try_into().map_err(|_| BAD_CONTEXT)?);
    Ok(wire::Statement {
        domain: decode_hex(domain.to_bytes())?,
        action: decode_hex(action.to_bytes())?,
        script_hash: load_script_hash().map_err(|_| BAD_CONTEXT)?,
        old_data_hash: load_cell_data_hash(0, Source::GroupInput).map_err(|_| BAD_CONTEXT)?,
        new_data_hash: load_cell_data_hash(0, Source::GroupOutput).map_err(|_| BAD_CONTEXT)?,
        input_transaction_hash,
        input_output_index,
        transaction_hash: load_tx_hash().map_err(|_| BAD_CONTEXT)?,
    })
}

#[cfg(feature = "counter-groth16")]
fn verify_proof(key: &[u8; KEY_BYTES], proof: &[u8], public: &[u8; wire::PUBLIC_INPUT_BYTES]) -> Result<(), i8> {
    if key[224..232] != ((wire::PUBLIC_INPUT_COUNT + 1) as u64).to_le_bytes() {
        return Err(BAD_KEY);
    }
    verifier_core::verify(key, proof, public).map_err(|_| BAD_PROOF)
}

#[cfg(not(feature = "counter-groth16"))]
fn verify_proof(key: &[u8; KEY_BYTES], proof: &[u8], public: &[u8; wire::PUBLIC_INPUT_BYTES]) -> Result<(), i8> {
    if !key.ends_with(&(wire::PUBLIC_INPUT_COUNT as u32).to_le_bytes()) {
        return Err(BAD_KEY);
    }
    let mut fields = [0; wire::PUBLIC_INPUT_COUNT * 32];
    fields.copy_from_slice(&public[4..]);
    for field in fields.chunks_exact_mut(32) {
        field.reverse();
    }
    mosaic_plonk::PlonkKzgBn254::new(&backend::Arkworks).verify(key, proof, &fields).map_err(|_| BAD_PROOF)
}

fn verify() -> Result<(), i8> {
    let statement = statement()?;
    // Dep 1 is authenticated by the compiled parent before the checked Spawn.
    let mut key = [0; KEY_BYTES];
    if syscalls::load_cell_data(&mut key, 0, 1, Source::CellDep) != Ok(KEY_BYTES) {
        return Err(BAD_KEY);
    }
    let mut bytes = [0; PROOF_BYTES + 20];
    let size = syscalls::load_witness(&mut bytes, 0, 0, Source::GroupInput).map_err(|_| BAD_WITNESS)?;
    let witness = WitnessArgs::from_slice(bytes.get(..size).ok_or(BAD_WITNESS)?).map_err(|_| BAD_WITNESS)?;
    let proof = witness.input_type().to_opt().ok_or(BAD_WITNESS)?.raw_data();
    if proof.len() != PROOF_BYTES {
        return Err(BAD_WITNESS);
    }
    verify_proof(&key, &proof, &statement.public_inputs())
}

fn program_entry() -> i8 {
    verify().err().unwrap_or(0)
}
