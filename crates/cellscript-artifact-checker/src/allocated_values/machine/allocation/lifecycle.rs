//! Exact bounded record walk, resize/copy/retire and explicit release. These
//! preserve the monotonic budget; descriptor and source-operation proof remain
//! separate from these arena primitives.
use super::*;

impl Machine<'_> {
    /// Walk from the first record, checking each traversed extent and ordinal.
    /// Accept only an exact live payload address, never an interior pointer.
    pub(super) fn find_arena(
        &self,
        c: &mut Cursor<'_>,
        offset: u32,
        contract: &Contract,
        pointer: Option<u32>,
    ) -> Result<(), CheckerError> {
        let scan = c.label();
        let found = c.label();
        let done = c.label();
        let bad = c.label();
        if let Some(pointer) = pointer {
            c.stack_load(29, pointer)?;
        } else {
            c.stack_address(29, offset)?;
        }
        c.load(5, 29, 0, false)?;
        c.constant(7, u64::from(contract.bytes))?;
        c.branch(6, 7, 5, bad)?;
        c.immediate(0x13, 7, 7, 5, 15)?;
        c.branch(1, 7, 0, bad)?;
        c.load(28, 29, 8, false)?;
        c.constant(7, u64::from(contract.allocations))?;
        c.branch(6, 7, 28, bad)?;
        c.addi(30, 29, 32)?;
        c.add(31, 30, 5)?;
        c.constant(29, 1)?;
        c.bind(scan)?;
        c.branch(6, 28, 29, bad)?;
        c.branch(7, 30, 31, bad)?;
        c.sub(7, 31, 30)?;
        c.constant(5, 32)?;
        c.branch(6, 7, 5, bad)?;
        c.addi(7, 7, -32)?;
        c.load(5, 30, 0, false)?;
        c.branch(0, 5, 0, bad)?;
        c.branch(6, 7, 5, bad)?;
        c.immediate(0x13, 7, 7, 5, 15)?;
        c.branch(1, 7, 0, bad)?;
        c.load(6, 30, 8, false)?;
        c.branch(0, 6, 0, bad)?;
        c.branch(6, 5, 6, bad)?;
        c.addi(7, 6, 15)?;
        c.immediate(0x13, 7, 7, 7, -16)?;
        c.branch(1, 7, 5, bad)?;
        c.load(7, 30, 24, false)?;
        c.branch(1, 7, 29, bad)?;
        c.load(7, 30, 16, false)?;
        c.immediate(0x13, 5, 7, 7, 1)?;
        c.branch(1, 7, 0, bad)?;
        c.addi(7, 30, 32)?;
        c.branch(0, 7, 10, found)?;
        c.add(30, 7, 5)?;
        c.addi(29, 29, 1)?;
        c.jump(scan)?;
        c.bind(found)?;
        c.load(7, 30, 16, false)?;
        c.constant(5, 1)?;
        c.branch(1, 7, 5, bad)?;
        c.jump(done)?;
        c.bind(bad)?;
        self.failure(c.jump_target()?)?;
        c.bind(done)
    }

    fn allocation_child(
        &self,
        c: &mut Cursor<'_>,
        parent: &Site<'_>,
        allocations: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        let child = allocations.get(&c.pc).ok_or_else(|| invalid("resize has no checked allocation at its call boundary"))?;
        if child.owner != parent.owner
            || child.offset != parent.offset
            || child.contract != parent.contract
            || child.start <= parent.start
            || child.end > parent.end
            || !consumed.insert(child.start)
        {
            return Err(invalid("resize allocation belongs to another owner or is reused"));
        }
        // The already recognized child preserves a1..a6 and returns the fresh
        // zeroed payload in a0. Only that complete range may be summarized.
        c.pc = child.end;
        Ok(())
    }

    pub(super) fn lifecycle(
        &self,
        site: &Site<'_>,
        allocations: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 16_384 {
            return Err(invalid("invalid or excessive lifecycle extent"));
        }
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        let done = c.target(site.end);
        let region = (site.offset, site.offset + 32 + site.contract.bytes + site.contract.members.len() as u32 * 32 + 128);
        if site.kind == Primitive::Release {
            c.branch(0, 10, 0, done)?;
            self.find_arena(&mut c, site.offset, site.contract, None)?;
            self.store(&mut c, 0, 30, 16, false, region)?;
            return c.finish();
        }
        if site.kind != Primitive::Resize {
            return Err(invalid("unknown lifecycle operation"));
        }
        let fresh = c.label();
        let release = c.label();
        let copy = c.label();
        let bounded = c.label();
        c.branch(0, 10, 0, fresh)?;
        self.find_arena(&mut c, site.offset, site.contract, None)?;
        c.branch(0, 11, 0, release)?;
        c.stack_store(10, site.offset + 16)?;
        c.stack_store(6, site.offset + 24)?;
        c.addi(10, 11, 0)?;
        self.allocation_child(&mut c, site, allocations, consumed)?;
        c.stack_load(5, site.offset + 16)?;
        c.stack_load(6, site.offset + 24)?;
        c.branch(6, 6, 11, bounded)?;
        c.addi(6, 11, 0)?;
        c.bind(bounded)?;
        c.addi(7, 10, 0)?;
        c.bind(copy)?;
        c.load(28, 5, 0, true)?;
        self.store(&mut c, 28, 7, 0, true, region)?;
        c.addi(5, 5, 1)?;
        c.addi(7, 7, 1)?;
        c.addi(6, 6, -1)?;
        c.branch(1, 6, 0, copy)?;
        c.stack_load(5, site.offset + 16)?;
        self.store(&mut c, 0, 5, -16, false, region)?;
        c.jump(done)?;
        c.bind(release)?;
        self.store(&mut c, 0, 30, 16, false, region)?;
        c.constant(10, 0)?;
        c.jump(done)?;
        c.bind(fresh)?;
        c.addi(10, 11, 0)?;
        self.allocation_child(&mut c, site, allocations, consumed)?;
        c.finish()
    }
}
