#![no_std]
#![no_main]
//! Transaction-wide family selection plus independently owned single-state
//! proof groups. Required peer groups are executed by CKB, not called here.
ckb_std::entry!(entry);
ckb_std::default_alloc!(4096, 32768, 64);
use ckb_std::{
    ckb_constants::{CellField, Source},
    error::SysError,
    high_level, syscalls,
};

const SCRIPT_BYTES: usize = 126;
const MAX_CELLS: usize = 64;
const MAX_SET: usize = 4;
#[derive(Clone, Copy)]
#[repr(i8)]
enum Error {
    Context = 111,
    Args = 112,
    Bound = 113,
    Selection = 114,
    Correspondence = 115,
    State = 116,
    Creation = 117,
    Parent = 118,
    Exec = 119,
    Custody = 120,
}
struct Selection {
    count: usize,
    indices: [usize; MAX_SET],
    lanes: [u8; MAX_SET],
}

fn exists(index: usize, source: Source) -> Result<bool, Error> {
    match syscalls::load_cell(&mut [], 0, index, source) {
        Ok(_) | Err(SysError::LengthNotEnough(_)) => Ok(true),
        Err(SysError::IndexOutOfBound) => Ok(false),
        _ => Err(Error::Context),
    }
}
fn select(current: &[u8; SCRIPT_BYTES], source: Source) -> Result<Selection, Error> {
    let mut selected = Selection { count: 0, indices: [0; MAX_SET], lanes: [0; MAX_SET] };
    for index in 0..MAX_CELLS {
        if !exists(index, source)? {
            break;
        }
        let mut script = [0; SCRIPT_BYTES];
        let length = match syscalls::load_cell_by_field(&mut script, 0, index, source, CellField::Type) {
            Ok(size) | Err(SysError::LengthNotEnough(size)) => size,
            Err(SysError::ItemMissing) => continue,
            _ => return Err(Error::Context),
        };
        // CKB validates Molecule Script structure before execution. Read only
        // this fixed prefix: an oversized same-family Script cannot hide as fee.
        if script[16..48] != current[16..48] {
            continue;
        }
        if length < 93 || &script[53..61] != b"CSZKSET1" {
            return Err(Error::Args);
        }
        if script[61..93] != current[61..93] {
            continue;
        }
        if length != SCRIPT_BYTES || script[48] != 4 || script[94..] != current[94..] || script[93] >= 4 {
            return Err(Error::Args);
        }
        let n = selected.count;
        if n == MAX_SET || n > 0 && script[93] <= selected.lanes[n - 1] {
            return Err(Error::Selection);
        }
        selected.indices[n] = index;
        selected.lanes[n] = script[93];
        selected.count += 1;
    }
    Ok(selected)
}
fn state(index: usize, source: Source) -> Result<[u8; 48], Error> {
    let mut data = [0; 48];
    if syscalls::load_cell_data(&mut data, 0, index, source) != Ok(48) || &data[..8] != b"CSZKCNT1" {
        return Err(Error::State);
    }
    Ok(data)
}
fn validate() -> Result<(), Error> {
    for source in [Source::Input, Source::Output, Source::CellDep] {
        if exists(MAX_CELLS, source)? {
            return Err(Error::Bound);
        }
    }
    match syscalls::load_transaction(&mut [], 0) {
        Ok(n) | Err(SysError::LengthNotEnough(n)) if n <= 16_384 => {}
        _ => return Err(Error::Bound),
    }
    let mut script = [0; SCRIPT_BYTES];
    if syscalls::load_script(&mut script, 0) != Ok(SCRIPT_BYTES)
        || script[48] != 4
        || &script[53..61] != b"CSZKSET1"
        || script[93] >= 4
        || script[94..] == [0; 32]
    {
        return Err(Error::Args);
    }
    let inputs = select(&script, Source::Input)?;
    let outputs = select(&script, Source::Output)?;
    if exists(1, Source::GroupInput)? || exists(1, Source::GroupOutput)? || !exists(0, Source::GroupOutput)? {
        return Err(Error::Correspondence);
    }
    if outputs.count == 0 {
        return Err(Error::Correspondence);
    }
    if inputs.count == 0 {
        if exists(0, Source::GroupInput)? {
            return Err(Error::Creation);
        }
        for i in 0..outputs.count {
            if outputs.lanes[i] != i as u8 || state(outputs.indices[i], Source::Output)?[40..] != [0; 8] {
                return Err(Error::Creation);
            }
        }
        // Every creation includes lane 0, whose required Type group authenticates
        // the shared family with the standard Type-ID first-input/output-index
        // rule. Other lanes cannot replace this peer's consensus execution.
        if script[93] == 0 {
            ckb_std::type_id::validate_type_id(&script[61..93]).map_err(|_| Error::Creation)?;
        }
        return Ok(());
    }
    if !exists(0, Source::GroupInput)? || inputs.count != outputs.count || inputs.lanes != outputs.lanes {
        return Err(Error::Correspondence);
    }
    for i in 0..inputs.count {
        if high_level::load_cell_type_hash(inputs.indices[i], Source::Input).map_err(|_| Error::Context)?
            != high_level::load_cell_type_hash(outputs.indices[i], Source::Output).map_err(|_| Error::Context)?
        {
            return Err(Error::Correspondence);
        }
        let input = inputs.indices[i];
        let output = outputs.indices[i];
        if state(input, Source::Input)?[8..40] != state(output, Source::Output)?[8..40] {
            return Err(Error::State);
        }
        if high_level::load_cell_lock_hash(input, Source::Input).map_err(|_| Error::Context)?
            != high_level::load_cell_lock_hash(output, Source::Output).map_err(|_| Error::Context)?
            || high_level::load_cell_capacity(input, Source::Input).map_err(|_| Error::Context)?
                != high_level::load_cell_capacity(output, Source::Output).map_err(|_| Error::Context)?
        {
            return Err(Error::Custody);
        }
    }
    let mut parent = None;
    for index in 0..MAX_CELLS {
        match high_level::load_cell_data_hash(index, Source::CellDep) {
            Err(SysError::IndexOutOfBound) => break,
            Ok(hash) if hash.as_slice() == &script[94..] => {
                if parent.replace(index).is_some() {
                    return Err(Error::Parent);
                }
            }
            Ok(_) => {}
            _ => return Err(Error::Parent),
        }
    }
    let _ = syscalls::exec(parent.ok_or(Error::Parent)?, Source::CellDep, 0, 0, &[]);
    Err(Error::Exec)
}
fn entry() -> i8 {
    match validate() {
        Ok(()) => 0,
        Err(error) => error as i8,
    }
}
