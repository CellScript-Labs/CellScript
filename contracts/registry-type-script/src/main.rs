#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
ckb_std::entry!(program_entry);
ckb_std::default_alloc!(16_384, 1_258_306, 64);

#[cfg(all(not(test), not(target_arch = "riscv64")))]
#[panic_handler]
fn host_panic_handler(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

use ckb_std::{
    ckb_constants::{CellField, Source},
    error::SysError,
    syscalls,
};

const COMMITMENT_MAGIC: &[u8; 7] = b"CSREGv1";
const COMMITMENT_HASH_BYTES: usize = 32;
const COMMITMENT_DATA_BYTES: usize = COMMITMENT_MAGIC.len() + COMMITMENT_HASH_BYTES;
const CUSTODY_LOCK_HASH_BYTES: usize = 32;
// Molecule Script = total_size + 3 field offsets + code_hash + hash_type +
// args(Bytes length prefix + 32-byte payload).
const SCRIPT_BYTES_WITH_CUSTODY_HASH: usize = 4 + (3 * 4) + 32 + 1 + 4 + CUSTODY_LOCK_HASH_BYTES;
const SCRIPT_ARGS_OFFSET: usize = SCRIPT_BYTES_WITH_CUSTODY_HASH - CUSTODY_LOCK_HASH_BYTES;

#[repr(i8)]
enum Error {
    Syscall = 5,
    NonCanonicalArgs = 6,
    InvalidCommitmentData = 7,
    InvalidCustodyLock = 8,
    MissingCustodyInput = 9,
}

impl From<SysError> for Error {
    fn from(_: SysError) -> Self {
        Self::Syscall
    }
}

pub fn program_entry() -> i8 {
    match validate() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}

fn validate() -> Result<(), Error> {
    let mut script = [0u8; SCRIPT_BYTES_WITH_CUSTODY_HASH];
    match syscalls::load_script(&mut script, 0) {
        Ok(SCRIPT_BYTES_WITH_CUSTODY_HASH) => {}
        Ok(_) | Err(SysError::LengthNotEnough(_)) => return Err(Error::NonCanonicalArgs),
        Err(error) => return Err(error.into()),
    }
    let mut custody_lock_hash = [0u8; CUSTODY_LOCK_HASH_BYTES];
    custody_lock_hash.copy_from_slice(&script[SCRIPT_ARGS_OFFSET..]);

    validate_group(Source::GroupInput, &custody_lock_hash)?;
    validate_group(Source::GroupOutput, &custody_lock_hash)?;
    require_custody_input(&custody_lock_hash)?;
    Ok(())
}

fn validate_group(source: Source, custody_lock_hash: &[u8; CUSTODY_LOCK_HASH_BYTES]) -> Result<(), Error> {
    for index in 0.. {
        let mut data = [0u8; COMMITMENT_DATA_BYTES];
        match syscalls::load_cell_data(&mut data, 0, index, source) {
            Ok(COMMITMENT_DATA_BYTES) => {
                validate_commitment_data(&data)?;
                if &load_cell_lock_hash(index, source)? != custody_lock_hash {
                    return Err(Error::InvalidCustodyLock);
                }
            }
            Ok(_) | Err(SysError::LengthNotEnough(_)) => return Err(Error::InvalidCommitmentData),
            Err(SysError::IndexOutOfBound) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

fn require_custody_input(custody_lock_hash: &[u8; CUSTODY_LOCK_HASH_BYTES]) -> Result<(), Error> {
    for index in 0.. {
        match load_cell_lock_hash(index, Source::Input) {
            Ok(lock_hash) if &lock_hash == custody_lock_hash => return Ok(()),
            Ok(_) => {}
            Err(SysError::IndexOutOfBound) => return Err(Error::MissingCustodyInput),
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

fn load_cell_lock_hash(index: usize, source: Source) -> Result<[u8; CUSTODY_LOCK_HASH_BYTES], SysError> {
    let mut lock_hash = [0u8; CUSTODY_LOCK_HASH_BYTES];
    match syscalls::load_cell_by_field(&mut lock_hash, 0, index, source, CellField::LockHash) {
        Ok(CUSTODY_LOCK_HASH_BYTES) => Ok(lock_hash),
        Ok(_) | Err(SysError::LengthNotEnough(_)) => Err(SysError::Encoding),
        Err(error) => Err(error),
    }
}

fn validate_commitment_data(data: &[u8]) -> Result<(), Error> {
    if data.len() == COMMITMENT_DATA_BYTES && data.starts_with(COMMITMENT_MAGIC) {
        Ok(())
    } else {
        Err(Error::InvalidCommitmentData)
    }
}
