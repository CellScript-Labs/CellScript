//! Fixed, bounded wire contract shared by the two migration Scripts.
//! These helpers grant no authority to an arbitrary configuration Script: the
//! application manifest pins its complete Script identity at initialization.
use ckb_std::{
    ckb_constants::{CellField, Source},
    error::SysError,
    high_level, syscalls,
};

pub const MAX_CELLS: usize = 64;
pub const MAX_TRANSACTION_BYTES: usize = 16_384;
pub const COUNTER_SCRIPT_BYTES: usize = 53 + 64;
pub const CONFIG_SCRIPT_BYTES: usize = 53 + 160;
pub const CONFIG_BYTES: usize = 41;
pub const CONFIG_MAGIC: &[u8; 8] = b"CSZKCFG1";

#[derive(Debug, Clone, Copy)]
#[repr(i8)]
pub enum Error {
    Context = 100,
    Args = 101,
    Bound = 102,
    Identity = 103,
    State = 104,
    Configuration = 105,
    Parent = 106,
    Exec = 107,
    Pairing = 108,
    Migration = 109,
    Custody = 110,
}

pub fn exists(index: usize, source: Source) -> Result<bool, Error> {
    match syscalls::load_cell(&mut [], 0, index, source) {
        Ok(_) | Err(SysError::LengthNotEnough(_)) => Ok(true),
        Err(SysError::IndexOutOfBound) => Ok(false),
        _ => Err(Error::Context),
    }
}

pub fn bounds() -> Result<(), Error> {
    // Source lists are contiguous. Check their endpoint before any scan or
    // Type ID helper, including sources not used by a particular transition.
    for source in [Source::Input, Source::Output, Source::CellDep] {
        if exists(MAX_CELLS, source)? {
            return Err(Error::Bound);
        }
    }
    match syscalls::load_transaction(&mut [], 0) {
        Ok(size) | Err(SysError::LengthNotEnough(size)) if size <= MAX_TRANSACTION_BYTES => Ok(()),
        Ok(_) | Err(SysError::LengthNotEnough(_)) => Err(Error::Bound),
        _ => Err(Error::Context),
    }
}

pub fn current_script<const N: usize>() -> Result<[u8; N], Error> {
    let mut script = [0; N];
    if syscalls::load_script(&mut script, 0) != Ok(N) || script.get(48) != Some(&4) {
        return Err(Error::Args);
    }
    Ok(script)
}

pub fn type_script<const N: usize>(index: usize, source: Source) -> Result<[u8; N], Error> {
    let mut script = [0; N];
    if syscalls::load_cell_by_field(&mut script, 0, index, source, CellField::Type) != Ok(N) || script.get(48) != Some(&4) {
        return Err(Error::Args);
    }
    Ok(script)
}

pub fn group_has_input() -> Result<bool, Error> {
    if exists(1, Source::GroupInput)? || exists(1, Source::GroupOutput)? || !exists(0, Source::GroupOutput)? {
        return Err(Error::Identity);
    }
    exists(0, Source::GroupInput)
}

pub fn preserve_custody() -> Result<(), Error> {
    if high_level::load_cell_lock_hash(0, Source::GroupInput).map_err(|_| Error::Context)?
        != high_level::load_cell_lock_hash(0, Source::GroupOutput).map_err(|_| Error::Context)?
        || high_level::load_cell_capacity(0, Source::GroupInput).map_err(|_| Error::Context)?
            != high_level::load_cell_capacity(0, Source::GroupOutput).map_err(|_| Error::Context)?
    {
        return Err(Error::Custody);
    }
    Ok(())
}

pub fn find_cell(source: Source, mut matches: impl FnMut(usize) -> Result<bool, Error>) -> Result<Option<usize>, Error> {
    let mut selected = None;
    for index in 0..MAX_CELLS {
        if !exists(index, source)? {
            break;
        }
        if matches(index)? && selected.replace(index).is_some() {
            return Err(Error::Identity);
        }
    }
    Ok(selected)
}

pub fn find_type_hash(source: Source, hash: &[u8]) -> Result<Option<usize>, Error> {
    find_cell(source, |index| {
        high_level::load_cell_type_hash(index, source)
            .map(|value| value.is_some_and(|value| value.as_slice() == hash))
            .map_err(|_| Error::Context)
    })
}

pub fn find_counter(source: Source, counter_id: &[u8], counter_code: &[u8], config_hash: &[u8]) -> Result<Option<usize>, Error> {
    find_cell(source, |index| {
        let mut script = [0; COUNTER_SCRIPT_BYTES];
        match syscalls::load_cell_by_field(&mut script, 0, index, source, CellField::Type) {
            // Molecule Script offsets are consensus-validated; matching the
            // exact length, hash type, code hash and all args matches the full
            // Script, not only its Type ID prefix.
            Ok(COUNTER_SCRIPT_BYTES) => {
                Ok(script[48] == 4 && &script[16..48] == counter_code && &script[53..85] == counter_id && &script[85..] == config_hash)
            }
            Ok(_) | Err(SysError::ItemMissing | SysError::LengthNotEnough(_)) => Ok(false),
            _ => Err(Error::Context),
        }
    })
}

pub fn counter_state(index: usize, source: Source) -> Result<[u8; 48], Error> {
    let mut data = [0; 48];
    if syscalls::load_cell_data(&mut data, 0, index, source) != Ok(48) || &data[..8] != b"CSZKCNT1" {
        return Err(Error::State);
    }
    Ok(data)
}

pub fn config_state(index: usize, source: Source, args: &[u8]) -> Result<[u8; CONFIG_BYTES], Error> {
    if args.len() != 160 || args[96..128] == args[128..160] || args[96..128] == [0; 32] || args[128..160] == [0; 32] {
        return Err(Error::Args);
    }
    let mut data = [0; CONFIG_BYTES];
    if syscalls::load_cell_data(&mut data, 0, index, source) != Ok(CONFIG_BYTES) || &data[..8] != CONFIG_MAGIC || data[8] > 1 {
        return Err(Error::Configuration);
    }
    let offset = 96 + usize::from(data[8]) * 32;
    if data[9..] != args[offset..offset + 32] {
        return Err(Error::Configuration);
    }
    Ok(data)
}
