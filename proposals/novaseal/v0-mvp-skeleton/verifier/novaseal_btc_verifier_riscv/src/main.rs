#![no_std]
#![no_main]
#![allow(unsafe_code)]

use core::{hint::spin_loop, panic::PanicInfo};
#[cfg(target_arch = "riscv64")]
use novaseal_btc_verifier_riscv::{EXIT_REJECT_SPAWN_IO, IPC_WORD_COUNT, SPAWN_INPUT_FD_INDEX, decide_words};

#[cfg(target_arch = "riscv64")]
const RISCV_EXIT_SYSCALL_NUMBER: u64 = 93;
#[cfg(target_arch = "riscv64")]
const CKB_VM2_PIPE_READ_SYSCALL_NUMBER: u64 = 2606;
#[cfg(target_arch = "riscv64")]
const CKB_VM2_INHERITED_FD_SYSCALL_NUMBER: u64 = 2607;
#[cfg(target_arch = "riscv64")]
const CKB_VM2_CLOSE_SYSCALL_NUMBER: u64 = 2608;

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    loop {
        spin_loop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    #[cfg(target_arch = "riscv64")]
    {
        let code = match read_spawn_words(SPAWN_INPUT_FD_INDEX) {
            Ok(words) => decide_words(&words).exit_code(),
            Err(()) => EXIT_REJECT_SPAWN_IO,
        };
        exit(code);
    }

    #[cfg(not(target_arch = "riscv64"))]
    {
        loop {
            spin_loop();
        }
    }
}

#[cfg(target_arch = "riscv64")]
fn read_spawn_words(fd_index: u64) -> Result<[u64; IPC_WORD_COUNT], ()> {
    let fd = inherited_fd(fd_index)?;
    let mut words = [0u64; IPC_WORD_COUNT];
    let mut ok = true;
    for word in &mut words {
        if let Ok(value) = pipe_read(fd) {
            *word = value;
        } else {
            ok = false;
            break;
        }
    }
    // A successful nineteenth read means the inherited-fd stream is not the
    // canonical 144-byte IPC envelope, even if the first 18 words are valid.
    if ok && pipe_read(fd).is_ok() {
        ok = false;
    }
    let close_ok = close_fd(fd).is_ok();
    if ok && close_ok { Ok(words) } else { Err(()) }
}

#[cfg(target_arch = "riscv64")]
fn inherited_fd(index: u64) -> Result<u64, ()> {
    let mut fds = [0u64; 1];
    let mut length = fds.len() as u64;
    let status: u64;
    let buffer_addr = fds.as_mut_ptr() as u64;
    let length_addr = (&raw mut length) as u64;
    // SAFETY: CKB VM v2 inherited_fd uses a0=buffer, a1=length pointer, and
    // returns status in a0 while writing the real inherited-fd count to length.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") buffer_addr => status,
            in("a1") length_addr,
            in("a7") CKB_VM2_INHERITED_FD_SYSCALL_NUMBER,
        );
    }
    if status == 0 && index == 0 && length >= 1 { Ok(fds[0]) } else { Err(()) }
}

#[cfg(target_arch = "riscv64")]
fn pipe_read(fd: u64) -> Result<u64, ()> {
    let mut value = 0u64;
    let mut length = core::mem::size_of::<u64>() as u64;
    let status: u64;
    let buffer_addr = (&raw mut value) as u64;
    let length_addr = (&raw mut length) as u64;
    // SAFETY: CKB VM v2 pipe_read uses a0=fd, a1=buffer, a2=length pointer,
    // returns status in a0, and writes the actual byte count through length.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") fd => status,
            in("a1") buffer_addr,
            in("a2") length_addr,
            in("a7") CKB_VM2_PIPE_READ_SYSCALL_NUMBER,
        );
    }
    if status == 0 && length == core::mem::size_of::<u64>() as u64 { Ok(value) } else { Err(()) }
}

#[cfg(target_arch = "riscv64")]
fn close_fd(fd: u64) -> Result<(), ()> {
    let status: u64;
    // SAFETY: CKB VM v2 close uses a0=fd and returns status in a0.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") fd => status,
            in("a7") CKB_VM2_CLOSE_SYSCALL_NUMBER,
        );
    }
    if status == 0 { Ok(()) } else { Err(()) }
}

#[cfg(target_arch = "riscv64")]
fn exit(code: u8) -> ! {
    // SAFETY: CKB VM accepts the RISC-V exit ecall convention; if execution
    // were ever to continue, the trailing spin loop preserves no-success.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a0") u64::from(code),
            in("a7") RISCV_EXIT_SYSCALL_NUMBER,
        );
    }
    loop {
        spin_loop();
    }
}
