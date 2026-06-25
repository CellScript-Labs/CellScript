#![no_std]
#![no_main]

use core::alloc::{GlobalAlloc, Layout};

use ckb_std::{
    ckb_constants::{CellField, Source},
    error::SysError,
    syscalls::{exit, load_cell_by_field, load_cell_data, load_witness},
};

struct NoAlloc;

unsafe impl GlobalAlloc for NoAlloc {
    unsafe fn alloc(&self, _: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

#[global_allocator]
static GLOBAL_ALLOCATOR: NoAlloc = NoAlloc;

core::arch::global_asm!(".global _start", "_start:", "call rust_main", "li a7, 93", "ecall",);

const TOKEN_BYTES: usize = 16;
const LOCK_HASH_BYTES: usize = 32;
const WITNESS_BYTES: usize = 40;
const WITNESS_MAGIC: &[u8; 8] = b"CSARGv1\0";
const ERROR_LOAD: i8 = 10;
const ERROR_LENGTH: i8 = 11;
const ERROR_WITNESS: i8 = 12;
const ERROR_DATA_MISMATCH: i8 = 13;
const ERROR_LOCK_MISMATCH: i8 = 14;

#[no_mangle]
pub extern "C" fn rust_main() -> i8 {
    match run() {
        Ok(()) => 0,
        Err(code) => code,
    }
}

fn run() -> Result<(), i8> {
    let to = load_entry_to_arg()?;
    let input = load_token(0, Source::Input)?;
    let output = load_token(0, Source::Output)?;
    if input != output {
        return Err(ERROR_DATA_MISMATCH);
    }
    let output_lock_hash = load_output_lock_hash(0)?;
    if output_lock_hash != to {
        return Err(ERROR_LOCK_MISMATCH);
    }
    Ok(())
}

fn load_entry_to_arg() -> Result<[u8; LOCK_HASH_BYTES], i8> {
    let mut witness = [0u8; WITNESS_BYTES];
    let len = load_witness_exact(&mut witness, Source::Input)
        .or_else(|_| load_witness_exact(&mut witness, Source::GroupInput))
        .or_else(|_| load_witness_exact(&mut witness, Source::GroupOutput))?;
    if len != WITNESS_BYTES || &witness[..8] != WITNESS_MAGIC {
        return Err(ERROR_WITNESS);
    }
    let mut to = [0u8; LOCK_HASH_BYTES];
    to.copy_from_slice(&witness[8..40]);
    Ok(to)
}

fn load_witness_exact(buf: &mut [u8; WITNESS_BYTES], source: Source) -> Result<usize, i8> {
    load_witness(buf, 0, 0, source).map_err(|_| ERROR_WITNESS)
}

fn load_token(index: usize, source: Source) -> Result<[u8; TOKEN_BYTES], i8> {
    let mut token = [0u8; TOKEN_BYTES];
    let len = load_cell_data(&mut token, 0, index, source).map_err(map_sys_error)?;
    if len != TOKEN_BYTES {
        return Err(ERROR_LENGTH);
    }
    Ok(token)
}

fn load_output_lock_hash(index: usize) -> Result<[u8; LOCK_HASH_BYTES], i8> {
    let mut lock_hash = [0u8; LOCK_HASH_BYTES];
    let len = load_cell_by_field(&mut lock_hash, 0, index, Source::Output, CellField::LockHash).map_err(map_sys_error)?;
    if len != LOCK_HASH_BYTES {
        return Err(ERROR_LENGTH);
    }
    Ok(lock_hash)
}

fn map_sys_error(err: SysError) -> i8 {
    match err {
        SysError::LengthNotEnough(_) => ERROR_LENGTH,
        SysError::IndexOutOfBound | SysError::ItemMissing | SysError::Encoding | SysError::Unknown(_) => ERROR_LOAD,
        _ => ERROR_LOAD,
    }
}

#[panic_handler]
fn panic_handler(_: &core::panic::PanicInfo) -> ! {
    exit(99)
}
