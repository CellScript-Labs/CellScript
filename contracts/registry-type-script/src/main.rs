#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
ckb_std::entry!(program_entry);
ckb_std::default_alloc!(16_384, 1_258_306, 64);

use ckb_std::{
    ckb_constants::Source,
    error::SysError,
    high_level::{load_cell_data, load_cell_lock_hash, load_script},
};

const COMMITMENT_MAGIC: &[u8; 7] = b"CSREGv1";
const COMMITMENT_HASH_BYTES: usize = 32;
const COMMITMENT_DATA_BYTES: usize = COMMITMENT_MAGIC.len() + COMMITMENT_HASH_BYTES;
const CUSTODY_LOCK_HASH_BYTES: usize = 32;

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
    let script = load_script()?;
    let raw_args = script.as_reader().args().raw_data();
    if raw_args.len() != CUSTODY_LOCK_HASH_BYTES {
        return Err(Error::NonCanonicalArgs);
    }
    let mut custody_lock_hash = [0u8; CUSTODY_LOCK_HASH_BYTES];
    custody_lock_hash.copy_from_slice(&raw_args);

    validate_group(Source::GroupInput, &custody_lock_hash)?;
    validate_group(Source::GroupOutput, &custody_lock_hash)?;
    require_custody_input(&custody_lock_hash)?;
    Ok(())
}

fn validate_group(source: Source, custody_lock_hash: &[u8; CUSTODY_LOCK_HASH_BYTES]) -> Result<(), Error> {
    for index in 0.. {
        match load_cell_data(index, source) {
            Ok(data) => {
                validate_commitment_data(&data)?;
                if &load_cell_lock_hash(index, source)? != custody_lock_hash {
                    return Err(Error::InvalidCustodyLock);
                }
            }
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

fn validate_commitment_data(data: &[u8]) -> Result<(), Error> {
    if data.len() == COMMITMENT_DATA_BYTES && data.starts_with(COMMITMENT_MAGIC) {
        Ok(())
    } else {
        Err(Error::InvalidCommitmentData)
    }
}
