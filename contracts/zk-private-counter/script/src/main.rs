#![no_std]
#![no_main]
//! Unique initialization and non-burnable lifecycle around a hash-bound
//! CellScript update executable. EXEC preserves the current Script context.
ckb_std::entry!(entry);
const SMALL_HEAP: usize = 4096;
const LARGE_HEAP: usize = 32768;
ckb_std::default_alloc!(SMALL_HEAP, LARGE_HEAP, 64);
use ckb_std::{ckb_constants::Source, error::SysError, high_level, syscalls};
const MAGIC: &[u8; 8] = b"CSZKCNT1";
const SCRIPT_BYTES: usize = 53 + 64;
#[repr(i8)]
enum Error {
    Context = 90,
    Args = 91,
    State = 92,
    Creation = 93,
    Burn = 94,
    Identity = 95,
    Parent = 96,
    Exec = 97,
}
fn exists(index: usize, source: Source) -> Result<bool, Error> {
    match syscalls::load_cell(&mut [], 0, index, source) {
        Ok(_) | Err(SysError::LengthNotEnough(_)) => Ok(true),
        Err(SysError::IndexOutOfBound) => Ok(false),
        _ => Err(Error::Context),
    }
}
fn state(source: Source) -> Result<[u8; 48], Error> {
    let mut data = [0; 48];
    match syscalls::load_cell_data(&mut data, 0, 0, source) {
        Ok(48) if &data[..8] == MAGIC => Ok(data),
        _ => Err(Error::State),
    }
}
fn validate() -> Result<(), Error> {
    let mut script = [0; SCRIPT_BYTES];
    if syscalls::load_script(&mut script, 0) != Ok(SCRIPT_BYTES) {
        return Err(Error::Args);
    }
    // Molecule's Script layout is fixed after CKB consensus validation.
    let args = &script[53..];
    if exists(1, Source::GroupInput)? || exists(1, Source::GroupOutput)? {
        return Err(Error::Identity);
    }
    if !exists(0, Source::GroupOutput)? {
        return Err(Error::Burn);
    }
    ckb_std::type_id::validate_type_id(&args[..32]).map_err(|_| Error::Identity)?;
    let next = state(Source::GroupOutput)?;
    if !exists(0, Source::GroupInput)? {
        if next[40..] != [0; 8] {
            return Err(Error::Creation);
        }
        return Ok(());
    }
    let old = state(Source::GroupInput)?;
    if old[8..40] != next[8..40] {
        return Err(Error::State);
    }
    if high_level::load_cell_lock_hash(0, Source::GroupInput).map_err(|_| Error::Context)?
        != high_level::load_cell_lock_hash(0, Source::GroupOutput).map_err(|_| Error::Context)?
        || high_level::load_cell_capacity(0, Source::GroupInput).map_err(|_| Error::Context)?
            != high_level::load_cell_capacity(0, Source::GroupOutput).map_err(|_| Error::Context)?
    {
        return Err(Error::State);
    }
    let mut parent = None;
    for index in 0..=64 {
        match high_level::load_cell_data_hash(index, Source::CellDep) {
            Err(SysError::IndexOutOfBound) => break,
            Ok(hash) if index < 64 => {
                if hash.as_slice() == &args[32..] && parent.replace(index).is_some() {
                    return Err(Error::Parent);
                }
            }
            _ => return Err(Error::Parent),
        }
    }
    let parent = parent.ok_or(Error::Parent)?;
    let _ = syscalls::exec(parent, Source::CellDep, 0, 0, &[]);
    Err(Error::Exec)
}
pub fn entry() -> i8 {
    match validate() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}
