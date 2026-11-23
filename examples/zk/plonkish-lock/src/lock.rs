#![no_std]
#![no_main]

use cellscript_zk_lock_plonkish_experiment::{
    context_scalar, hash, CONTEXT_MISMATCH, PLONK_PROOF_BYTES, PLONK_VK_BYTES, VERIFICATION_FAILED,
};
use ckb_std::{
    ckb_constants::Source,
    ckb_types::{packed::WitnessArgs, prelude::*},
    error::SysError,
    high_level::{load_cell_data_hash, load_script, load_tx_hash},
    syscalls,
};

mod backend;

ckb_std::entry!(program_entry);
ckb_std::default_alloc!(16_384, 1_258_306, 64);

const BAD_ARGS: i8 = 10;
const BAD_WITNESS: i8 = 12;
const BAD_KEY: i8 = 14;
const BAD_COMMITMENT: i8 = 15;
const MAX_DEPS: usize = 64;

fn verification_key(expected: &[u8; 32]) -> Result<[u8; PLONK_VK_BYTES], i8> {
    let mut found = None;
    for index in 0..=MAX_DEPS {
        let actual = match load_cell_data_hash(index, Source::CellDep) {
            Ok(data_hash) => data_hash,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return Err(BAD_KEY),
        };
        if index == MAX_DEPS {
            return Err(BAD_KEY);
        }
        if &actual == expected {
            if found.is_some() {
                return Err(BAD_KEY);
            }
            let mut key = [0; PLONK_VK_BYTES];
            if syscalls::load_cell_data(&mut key, 0, index, Source::CellDep) != Ok(key.len()) {
                return Err(BAD_KEY);
            }
            found = Some(key);
        }
    }
    found.ok_or(BAD_KEY)
}

fn verify() -> Result<(), i8> {
    let script = load_script().map_err(|_| BAD_ARGS)?;
    let args = script.args().raw_data();
    let ([vk_hash, commitment], []) = args.as_chunks::<32>() else {
        return Err(BAD_ARGS);
    };
    let key = verification_key(vk_hash)?;

    let mut bytes = [0; 896];
    let size = syscalls::load_witness(&mut bytes, 0, 0, Source::GroupInput).map_err(|_| BAD_WITNESS)?;
    let witness = WitnessArgs::from_slice(bytes.get(..size).ok_or(BAD_WITNESS)?).map_err(|_| BAD_WITNESS)?;
    let lock = witness.lock().to_opt().ok_or(BAD_WITNESS)?.raw_data();
    let (proof, inputs) = lock.split_first_chunk::<PLONK_PROOF_BYTES>().ok_or(BAD_WITNESS)?;
    let Some(([2, 0, 0, 0], fields)) = inputs.split_first_chunk::<4>() else {
        return Err(BAD_WITNESS);
    };
    let ([context, digest], []) = fields.as_chunks::<32>() else {
        return Err(BAD_WITNESS);
    };
    if &hash(digest) != commitment {
        return Err(BAD_COMMITMENT);
    }
    if *context != context_scalar(load_tx_hash().map_err(|_| CONTEXT_MISMATCH)?) {
        return Err(CONTEXT_MISMATCH);
    }

    let mut public_inputs = [0; 64];
    for (output, input) in public_inputs.chunks_exact_mut(32).zip([context, digest]) {
        for (byte, source) in output.iter_mut().zip(input.iter().rev()) {
            *byte = *source;
        }
    }
    mosaic_plonk::PlonkKzgBn254::new(&backend::Arkworks).verify(&key, proof, &public_inputs).map_err(|_| VERIFICATION_FAILED)
}

fn program_entry() -> i8 {
    verify().err().unwrap_or(0)
}
