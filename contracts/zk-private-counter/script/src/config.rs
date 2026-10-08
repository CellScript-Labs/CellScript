#![no_std]
#![no_main]
//! One authorized 0 -> 1 switch, coupled to the existing counter's proof group.
pub mod migration;
use ckb_std::{ckb_constants::Source, high_level};
use migration::*;
ckb_std::entry!(entry);
ckb_std::default_alloc!(4096, 32768, 64);

fn validate() -> Result<(), Error> {
    bounds()?;
    let script = current_script::<CONFIG_SCRIPT_BYTES>()?;
    let args = &script[53..];
    let has_input = group_has_input()?;
    ckb_std::type_id::validate_type_id(&args[..32]).map_err(|_| Error::Identity)?;
    let config_hash = high_level::load_script_hash().map_err(|_| Error::Context)?;
    let input = find_counter(Source::Input, &args[32..64], &args[64..96], &config_hash)?;
    let output = find_counter(Source::Output, &args[32..64], &args[64..96], &config_hash)?.ok_or(Error::Pairing)?;
    let next = config_state(0, Source::GroupOutput, args)?;
    if !has_input {
        if input.is_some() || next[8] != 0 || counter_state(output, Source::Output)?[40..] != [0; 8] {
            return Err(Error::Pairing);
        }
        return Ok(());
    }
    input.ok_or(Error::Pairing)?;
    preserve_custody()?;
    let old = config_state(0, Source::GroupInput, args)?;
    if old[8] != 0 || next[8] != 1 {
        return Err(Error::Migration);
    }
    // Presence is not a peer call: the required counter Type Script group must
    // independently accept the same transaction under CKB consensus. That group
    // uses the OLD config parent and a proof binding every raw transaction byte.
    Ok(())
}

pub fn entry() -> i8 {
    validate().map_or_else(|error| error as i8, |()| 0)
}
