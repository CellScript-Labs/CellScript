#![no_std]
#![no_main]

// Generic fixed-prefix CKB context adapter. No DAO, voting, amount, owner,
// time comparison, proposal or payout policy belongs here.
use ckb_std::{
    ckb_constants::Source,
    ckb_types::{packed, prelude::*},
    env, syscalls,
};

ckb_std::entry!(entry);
ckb_std::default_alloc!(4096, 4096, 64);

const MAX_TRANSACTION_BYTES: usize = 65_536;
const MAX_DEPS: usize = 128;

fn hex<const N: usize>(text: &[u8]) -> Result<[u8; N], i8> {
    if text.len() != N * 2 {
        return Err(10);
    }
    let digit = |byte| match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(10),
    };
    let mut bytes = [0; N];
    for (index, value) in bytes.iter_mut().enumerate() {
        *value = (digit(text[index * 2])? << 4) | digit(text[index * 2 + 1])?;
    }
    Ok(bytes)
}

fn block_number(index: usize) -> Result<u64, i8> {
    let mut bytes = [0; packed::Header::TOTAL_SIZE];
    let size = syscalls::load_header(&mut bytes, 0, index, Source::CellDep).map_err(|_| 13)?;
    if size != bytes.len() {
        return Err(13);
    }
    let header = packed::HeaderReader::from_slice(&bytes).map_err(|_| 13)?;
    Ok(u64::from_le_bytes(header.raw().number().as_slice().try_into().map_err(|_| 13)?))
}

fn run() -> Result<(), i8> {
    let args = env::argv();
    if args.len() != 4 || !args[3].to_bytes().is_empty() {
        return Err(10);
    }
    let first_outpoint = hex::<36>(args[0].to_bytes())?;
    let first_number = u64::from_le_bytes(hex::<8>(args[1].to_bytes())?);
    let second_number = u64::from_le_bytes(hex::<8>(args[2].to_bytes())?);
    let mut bytes = [0; MAX_TRANSACTION_BYTES];
    let size = syscalls::load_transaction(&mut bytes, 0).map_err(|_| 11)?;
    if size > bytes.len() {
        return Err(11);
    }
    let tx = packed::TransactionReader::from_slice(&bytes[..size]).map_err(|_| 11)?;
    let deps = tx.raw().cell_deps();
    if deps.len() < 2 || deps.len() > MAX_DEPS {
        return Err(11);
    }
    let first = deps.get(0).ok_or(11)?;
    let second = deps.get(1).ok_or(11)?;
    if first.dep_type().as_slice() != [0] || second.dep_type().as_slice() != [0] || first.out_point().as_slice() != first_outpoint {
        return Err(12);
    }
    // Resolved CellDep positions must agree with these raw direct-prefix
    // positions; reject duplicates instead of relying on resolver deduplication.
    for (index, dep) in deps.iter().enumerate() {
        for prior in deps.iter().take(index) {
            if dep.out_point().as_slice() == prior.out_point().as_slice() {
                return Err(12);
            }
        }
    }
    if block_number(0)? != first_number || block_number(1)? != second_number {
        return Err(14);
    }
    Ok(())
}

fn entry() -> i8 {
    match run() {
        Ok(()) => 0,
        Err(error) => error,
    }
}
