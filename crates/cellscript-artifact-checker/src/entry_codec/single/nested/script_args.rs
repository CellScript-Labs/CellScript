//! Bind mixed Script.args/witness parameters to disjoint wrapper-owned buffers.
//! The Script serialization comes from LOAD_SCRIPT, never from witness bytes.
use super::*;

const SIZE: u32 = 4104;
const START: u32 = 4112;
const LENGTH: u32 = 4120;
const CURSOR: u32 = 4128;
const BUFFER: u32 = 4136;

fn address(c: &mut Cursor<'_>, dest: u32, offset: u32) -> Result<(), CheckerError> {
    c.li(dest, u64::from(offset))?;
    c.r(dest, 2, dest, 0)
}

/// Independently check address materialization while preserving all declared
/// live registers. Scratch choice is not part of the ABI; memory location and
/// clobber safety are. Already decoded a0-a7 arguments are never scratch.
fn stack(c: &mut Cursor<'_>, register: u32, offset: u32, store: bool, byte: bool, live: &[u32]) -> Result<(), CheckerError> {
    let scratch = (c.word()? >> 7) & 31;
    if ![5, 6, 7, 28, 29, 30, 31].contains(&scratch) || scratch == register || live.contains(&scratch) {
        return Err(invalid("Script.args stack helper clobbers a live register"));
    }
    address(c, scratch, offset)?;
    if store {
        c.store(register, scratch, 0, byte)
    } else {
        c.load(register, scratch, 0, byte)
    }
}

fn u32_at(c: &mut Cursor<'_>, dest: u32, offset: u32, live: &[u32]) -> Result<(), CheckerError> {
    c.li(dest, 0)?;
    let live = live.iter().copied().chain([dest]).collect::<Vec<_>>();
    for index in 0..4 {
        stack(c, 5, offset + index, false, true, &live)?;
        if index != 0 {
            c.shift(5, 5, index * 8, false)?;
        }
        c.r(dest, dest, 5, 0x6000)?;
    }
    Ok(())
}

pub(super) fn load(c: &mut Cursor<'_>, fail: u64) -> Result<(), CheckerError> {
    c.li(5, 1024)?;
    stack(c, 5, SIZE, true, false, &[])?;
    address(c, 10, BUFFER)?;
    address(c, 11, SIZE)?;
    c.li(12, 0)?;
    c.li(17, 2052)?;
    c.expect(|word| word == 0x73)?;
    c.b(10, 0, 0, c.address + 8)?;
    c.jump(fail)?;
    stack(c, 5, SIZE, false, false, &[])?;
    c.li(6, 1025)?;
    c.r(7, 5, 6, 0x3000)?;
    c.b(7, 0, 1, c.address + 8)?;
    c.jump(fail)?;
    u32_at(c, 28, BUFFER, &[])?;
    stack(c, 5, SIZE, false, false, &[28])?;
    c.r(7, 5, 28, 0x40000000)?;
    guard(c, fail)?;
    c.li(6, 16)?;
    c.r(7, 28, 6, 0x3000)?;
    guard(c, fail)?;
    u32_at(c, 29, BUFFER + 12, &[28])?;
    c.li(6, 16)?;
    c.r(7, 29, 6, 0x3000)?;
    guard(c, fail)?;
    c.i(6, 29, 4)?;
    c.r(7, 28, 6, 0x3000)?;
    guard(c, fail)?;
    address(c, 5, BUFFER)?;
    c.r(5, 5, 29, 0)?;
    c.li(30, 0)?;
    for index in 0..4 {
        c.load(6, 5, index, true)?;
        if index != 0 {
            c.shift(6, 6, index as u32 * 8, false)?;
        }
        c.r(30, 30, 6, 0x6000)?;
    }
    c.i(31, 29, 4)?;
    c.r(6, 31, 30, 0)?;
    c.r(7, 28, 6, 0x3000)?;
    guard(c, fail)?;
    stack(c, 31, START, true, false, &[30])?;
    stack(c, 30, LENGTH, true, false, &[])?;
    c.li(5, 0)?;
    stack(c, 5, CURSOR, true, false, &[])
}

pub(super) fn argument(c: &mut Cursor<'_>, argument: &Argument, outgoing: u32, fail: u64) -> Result<(), CheckerError> {
    let Shape::Fixed { width, pointer } = argument.shape else {
        return Err(invalid("Script.args parameter lacks a fixed bounded representation"));
    };
    if width > 1024 {
        return Err(invalid("Script.args parameter exceeds the loader capacity"));
    }
    stack(c, 28, CURSOR, false, false, &[30, 31])?;
    stack(c, 29, LENGTH, false, false, &[28, 30, 31])?;
    c.i(6, 28, width as i32)?;
    c.r(7, 29, 6, 0x3000)?;
    guard(c, fail)?;
    stack(c, 29, START, false, false, &[28, 30, 31])?;
    c.r(29, 29, 28, 0)?;
    address(c, 5, BUFFER)?;
    c.r(5, 5, 29, 0)?;
    if pointer {
        register_arg(c, argument.abi, 5, outgoing)?;
        immediate_arg(c, argument.abi + 1, u64::from(width), outgoing)?;
    } else {
        let dest = if argument.abi < 8 { 10 + argument.abi } else { 29 };
        c.li(dest, 0)?;
        for index in 0..width {
            c.load(6, 5, index as i32, true)?;
            if index != 0 {
                c.shift(6, 6, index * 8, false)?;
            }
            c.r(dest, dest, 6, 0x6000)?;
        }
        if argument.signed {
            c.shift(dest, dest, 32, false)?;
            c.expect(|word| word == (0x42005013 | dest << 15 | dest << 7))?;
        }
        if argument.abi >= 8 {
            register_arg(c, argument.abi, dest, outgoing)?;
        }
    }
    stack(c, 28, CURSOR, false, false, &[30, 31])?;
    c.i(28, 28, width as i32)?;
    stack(c, 28, CURSOR, true, false, &[30, 31])
}

pub(super) fn finish(c: &mut Cursor<'_>, fail: u64) -> Result<(), CheckerError> {
    stack(c, 5, CURSOR, false, false, &[])?;
    stack(c, 6, LENGTH, false, false, &[5])?;
    c.r(7, 6, 5, 0x40000000)?;
    guard(c, fail)
}
