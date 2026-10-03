#![no_std]
#![no_main]

ckb_std::entry!(program_entry);
ckb_std::default_alloc!();

// Shared allocation-free codec; importing this file does not link the
// standalone checker's host parsing or the CellScript compiler into the TCB.
#[path = "../../../crates/cellscript-artifact-checker/src/zk.rs"]
#[allow(dead_code)]
mod wire;

use ckb_std::{ckb_constants::Source, error::SysError, high_level::load_cell_data_hash, syscalls};

const INVALID_TRANSPORT: i8 = 80;
const INVALID_ENVELOPE: i8 = 81;
const INVALID_KEY: i8 = 82;
const INVALID_PROOF: i8 = 83;

fn receive() -> Result<[u8; wire::REQUEST_BYTES], i8> {
    let mut fds = [0u64; 2];
    if syscalls::inherited_fds(&mut fds) != 1 || fds[0] == 0 {
        return Err(INVALID_TRANSPORT);
    }
    let fd = fds[0];
    let mut bytes = [0; wire::REQUEST_BYTES];
    let mut result = Ok(());
    for word in bytes.chunks_exact_mut(8) {
        if syscalls::read(fd, word) != Ok(8) {
            result = Err(INVALID_TRANSPORT);
            break;
        }
    }
    if result.is_ok() {
        match syscalls::read(fd, &mut [0u8; 1]) {
            Ok(0) | Err(SysError::OtherEndClosed) => {}
            _ => result = Err(INVALID_TRANSPORT),
        }
    }
    if syscalls::close(fd).is_err() {
        result = Err(INVALID_TRANSPORT);
    }
    result.map(|()| bytes)
}

fn verification_key(hash: &[u8; 32]) -> Result<[u8; wire::VK_BYTES], i8> {
    // A bounded resolved CellDep search. DepGroup members use the CKB syscall
    // CellDep view, so the selected hash belongs to the actual VK bytes.
    for index in 0..64 {
        let candidate = match load_cell_data_hash(index, Source::CellDep) {
            Ok(candidate) => candidate,
            Err(SysError::IndexOutOfBound) => break,
            Err(_) => return Err(INVALID_KEY),
        };
        if &candidate == hash {
            let mut key = [0; wire::VK_BYTES];
            if syscalls::load_cell_data(&mut key, 0, index, Source::CellDep) != Ok(wire::VK_BYTES) {
                return Err(INVALID_KEY);
            }
            if key[224..232] != 14u64.to_le_bytes() {
                return Err(INVALID_KEY);
            }
            return Ok(key);
        }
    }
    Err(INVALID_KEY)
}

fn verify() -> Result<(), i8> {
    let bytes = receive()?;
    // An admitted parent must bind this key commitment to its exact profile
    // before sending. Parent admission is still unfinished; this research
    // child only authenticates the matching CellDep bytes.
    let mut key_hash = [0; 32];
    key_hash.copy_from_slice(&bytes[76..108]);
    let request = wire::Request::decode(&bytes, &key_hash).map_err(|_| INVALID_ENVELOPE)?;
    let key = verification_key(&request.verification_key)?;
    verifier_core::verify(&key, &request.proof, &request.statement.public_inputs()).map_err(|_| INVALID_PROOF)
}

fn program_entry() -> i8 {
    match verify() {
        Ok(()) => 0,
        Err(code) => code,
    }
}
