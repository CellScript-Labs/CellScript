#![no_std]
#![no_main]
//! Counter lifecycle with one separately guarded, exact verifier migration.
pub mod migration;
use ckb_std::{ckb_constants::Source, high_level, syscalls};
use migration::*;
ckb_std::entry!(entry);
ckb_std::default_alloc!(4096, 32768, 64);

fn validate() -> Result<(), Error> {
    bounds()?;
    let script = current_script::<COUNTER_SCRIPT_BYTES>()?;
    let args = &script[53..];
    let has_input = group_has_input()?;
    ckb_std::type_id::validate_type_id(&args[..32]).map_err(|_| Error::Identity)?;
    let next = counter_state(0, Source::GroupOutput)?;
    let config_input = find_type_hash(Source::Input, &args[32..])?;
    let config_dep = find_type_hash(Source::CellDep, &args[32..])?;
    let config_output = find_type_hash(Source::Output, &args[32..])?;
    let (index, source) = match (has_input, config_input, config_dep, config_output) {
        (false, None, None, Some(index)) if next[40..] == [0; 8] => (index, Source::Output),
        (true, None, Some(index), None) => (index, Source::CellDep),
        (true, Some(index), None, Some(_)) => (index, Source::Input),
        _ => return Err(Error::Pairing),
    };
    let config = type_script::<CONFIG_SCRIPT_BYTES>(index, source)?;
    let config_args = &config[53..];
    if config_args[32..64] != args[..32] || config_args[64..96] != script[16..48] {
        return Err(Error::Pairing);
    }
    let selected = config_state(index, source, config_args)?;
    if !has_input {
        if selected[8] != 0 {
            return Err(Error::Migration);
        }
        return Ok(());
    }
    let old = counter_state(0, Source::GroupInput)?;
    if old[8..40] != next[8..40] {
        return Err(Error::State);
    }
    preserve_custody()?;
    if let Some(output) = config_output
        && (selected[8] != 0 || config_state(output, Source::Output, config_args)?[8] != 1)
    {
        return Err(Error::Migration);
    }
    let parent = find_cell(Source::CellDep, |index| {
        high_level::load_cell_data_hash(index, Source::CellDep)
            .map(|hash| hash.as_slice() == &selected[9..])
            .map_err(|_| Error::Context)
    })?
    .ok_or(Error::Parent)?;
    // EXEC retains this counter Script group; the chosen parent checks a proof
    // over the final raw transaction, including the config successor on switch.
    let _ = syscalls::exec(parent, Source::CellDep, 0, 0, &[]);
    Err(Error::Exec)
}

pub fn entry() -> i8 {
    validate().map_or_else(|error| error as i8, |()| 0)
}
