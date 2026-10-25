//! Fixed-count sorted construction. Validate nondecreasing input before reserve,
//! count distinct keys and copy with last-value-wins duplicate handling.
//! The input pointer's source provenance remains an enclosing-operation duty.
use super::descriptor::layout;
use super::*;
use crate::TypedSemanticType;

fn compare(c: &mut Cursor<'_>, key: (u32, bool), branches: [usize; 3]) -> Result<(), CheckerError> {
    let (width, numeric) = key;
    let [less, equal, greater] = branches;
    c.addi(28, 13, 0)?;
    c.addi(29, 14, 0)?;
    let step = if numeric {
        c.addi(28, 28, width as i32 - 1)?;
        c.addi(29, 29, width as i32 - 1)?;
        -1
    } else {
        1
    };
    let repeat = c.label();
    c.constant(10, u64::from(width))?;
    c.bind(repeat)?;
    c.load(30, 28, 0, true)?;
    c.load(31, 29, 0, true)?;
    c.branch(6, 30, 31, less)?;
    c.branch(6, 31, 30, greater)?;
    c.addi(10, 10, -1)?;
    c.branch(0, 10, 0, equal)?;
    c.addi(28, 28, step)?;
    c.addi(29, 29, step)?;
    c.jump(repeat)
}

impl Machine<'_> {
    pub(super) fn sorted(
        &self,
        site: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
        types: &BTreeMap<&str, &TypedSemanticType>,
    ) -> Result<(), CheckerError> {
        let Primitive::Sorted(count) = site.kind else {
            return Err(invalid("not a sorted operation"));
        };
        if count > 1024 || site.start >= site.end || site.end - site.start > 65_536 {
            return Err(invalid("invalid or excessive sorted construction extent"));
        }
        let (descriptor, width, limit) = layout(site, types)?;
        let (key_width, numeric, _) = super::ordered::key(site)?;
        let key = (key_width, numeric);
        let scratch = site.offset + 32 + site.contract.bytes + site.contract.members.len() as u32 * 32;
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        self.check_descriptor(&mut c, site, descriptor, width, limit)?;
        let empty = c.label();
        c.branch(0, 6, 0, empty)?;
        self.failure(c.jump_target()?)?;
        c.bind(empty)?;
        if count == 0 {
            return c.finish();
        }
        let bad = c.label();
        let done = c.target(site.end);
        c.stack_load(14, scratch)?;
        c.constant(5, u64::from(count) * u64::from(width))?;
        c.add(6, 14, 5)?;
        c.branch(6, 6, 14, bad)?;
        for (start, end) in [(descriptor, descriptor + 32), (site.offset, site.offset + 32 + site.contract.bytes)] {
            let disjoint = c.label();
            c.stack_address(7, start)?;
            c.branch(7, 7, 6, disjoint)?;
            c.stack_address(7, end)?;
            c.branch(6, 14, 7, bad)?;
            c.bind(disjoint)?;
        }
        c.constant(16, 1)?;
        if count > 1 {
            let scan = c.label();
            let unique = c.label();
            let next = c.label();
            c.addi(13, 14, 0)?;
            c.addi(14, 14, width as i32)?;
            c.constant(15, u64::from(count - 1))?;
            c.bind(scan)?;
            compare(&mut c, key, [unique, next, bad])?;
            c.bind(unique)?;
            c.addi(16, 16, 1)?;
            c.bind(next)?;
            c.addi(13, 14, 0)?;
            c.addi(14, 14, width as i32)?;
            c.addi(15, 15, -1)?;
            c.branch(1, 15, 0, scan)?;
        }
        c.addi(11, 16, 0)?;
        self.reserve_child(&mut c, site, children, consumed)?;
        c.stack_store(16, descriptor + 8)?;
        c.stack_load(13, descriptor)?;
        c.stack_load(14, scratch)?;
        c.constant(15, u64::from(count))?;
        let copy = c.label();
        let advance = c.label();
        let fresh = c.label();
        c.jump(copy)?;
        c.bind(advance)?;
        c.addi(13, 13, -(width as i32))?;
        compare(&mut c, key, [fresh, copy, bad])?;
        c.bind(fresh)?;
        c.addi(13, 13, width as i32)?;
        c.bind(copy)?;
        c.addi(5, 13, 0)?;
        c.addi(28, 14, 0)?;
        c.constant(7, u64::from(width))?;
        self.copy_element_bytes(&mut c, (site.offset, scratch + 128))?;
        c.addi(13, 13, width as i32)?;
        c.addi(14, 14, width as i32)?;
        c.addi(15, 15, -1)?;
        c.branch(1, 15, 0, advance)?;
        c.jump(done)?;
        c.bind(bad)?;
        self.failure(c.jump_target()?)?;
        c.finish()
    }
}
