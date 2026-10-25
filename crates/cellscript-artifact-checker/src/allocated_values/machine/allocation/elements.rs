//! Owned-vector element copies and writes shared by Vec and ordered collections. Exact
//! byte loops bind overlap direction, vacated-byte clearing and length updates.
//! Argument staging and final source/result provenance remain separate.
use super::descriptor::{layout, multiply};
use super::*;
use crate::TypedSemanticType;

impl Machine<'_> {
    pub(super) fn copy_element_bytes(&self, c: &mut Cursor<'_>, region: (u32, u32)) -> Result<(), CheckerError> {
        let copy = c.label();
        c.bind(copy)?;
        c.load(29, 28, 0, true)?;
        self.store(c, 29, 5, 0, true, region)?;
        c.addi(5, 5, 1)?;
        c.addi(28, 28, 1)?;
        c.addi(7, 7, -1)?;
        c.branch(1, 7, 0, copy)
    }

    pub(super) fn reserve_child(
        &self,
        c: &mut Cursor<'_>,
        parent: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        let child = children.get(&c.pc).ok_or_else(|| invalid("element mutation has no checked reserve child"))?;
        if child.owner != parent.owner
            || child.offset != parent.offset
            || child.descriptor != parent.descriptor
            || child.contract != parent.contract
            || child.start <= parent.start
            || child.end > parent.end
            || !consumed.insert(child.start)
        {
            return Err(invalid("element reserve belongs to another member or is reused"));
        }
        c.pc = child.end;
        Ok(())
    }

    pub(super) fn elements(
        &self,
        site: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
        types: &BTreeMap<&str, &TypedSemanticType>,
    ) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 65_536 {
            return Err(invalid("invalid or excessive element mutation extent"));
        }
        let (descriptor, width, limit) = layout(site, types)?;
        let scratch = site.scratch();
        let region = site.region();
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        self.check_descriptor(&mut c, site, descriptor, width, limit)?;
        if matches!(site.kind, Primitive::Set | Primitive::Remove | Primitive::Read) {
            let valid = c.label();
            c.branch(6, 11, 6, valid)?;
            self.failure(c.jump_target()?)?;
            c.bind(valid)?;
            c.constant(7, u64::from(width))?;
            multiply(&mut c, 7, 11, 7)?;
            c.add(5, 5, 7)?;
        }
        match site.kind {
            Primitive::Push => {
                c.addi(11, 6, 1)?;
                self.reserve_child(&mut c, site, children, consumed)?;
                c.stack_load(5, descriptor)?;
                c.stack_load(6, descriptor + 8)?;
                c.constant(7, u64::from(width))?;
                multiply(&mut c, 7, 6, 7)?;
                c.add(5, 5, 7)?;
                c.stack_address(28, scratch)?;
                c.constant(7, u64::from(width))?;
                self.copy_element_bytes(&mut c, region)?;
                c.addi(6, 6, 1)?;
                c.stack_store(6, descriptor + 8)?;
            }
            Primitive::Insert => {
                let valid = c.label();
                c.branch(7, 6, 11, valid)?;
                self.failure(c.jump_target()?)?;
                c.bind(valid)?;
                c.addi(14, 11, 0)?;
                c.addi(11, 6, 1)?;
                self.reserve_child(&mut c, site, children, consumed)?;
                c.stack_load(5, descriptor)?;
                c.stack_load(6, descriptor + 8)?;
                c.constant(7, u64::from(width))?;
                multiply(&mut c, 28, 6, 7)?;
                c.add(28, 5, 28)?;
                c.add(5, 28, 7)?;
                c.sub(7, 6, 14)?;
                c.constant(29, u64::from(width))?;
                multiply(&mut c, 7, 7, 29)?;
                let place = c.label();
                let shift = c.label();
                c.branch(0, 7, 0, place)?;
                c.bind(shift)?;
                c.addi(5, 5, -1)?;
                c.addi(28, 28, -1)?;
                c.load(29, 28, 0, true)?;
                self.store(&mut c, 29, 5, 0, true, region)?;
                c.addi(7, 7, -1)?;
                c.branch(1, 7, 0, shift)?;
                c.bind(place)?;
                c.addi(5, 28, 0)?;
                c.stack_address(28, scratch)?;
                c.constant(7, u64::from(width))?;
                self.copy_element_bytes(&mut c, region)?;
                c.addi(6, 6, 1)?;
                c.stack_store(6, descriptor + 8)?;
            }
            Primitive::Set => {
                c.stack_address(28, scratch)?;
                c.constant(7, u64::from(width))?;
                self.copy_element_bytes(&mut c, region)?;
            }
            Primitive::Read => {
                c.addi(28, 5, 0)?;
                c.stack_address(5, scratch + 64)?;
                c.constant(7, u64::from(width))?;
                self.copy_element_bytes(&mut c, region)?;
            }
            Primitive::Remove => {
                c.addi(6, 6, -1)?;
                c.sub(7, 6, 11)?;
                c.constant(28, u64::from(width))?;
                multiply(&mut c, 7, 7, 28)?;
                c.add(28, 5, 28)?;
                let clear = c.label();
                c.branch(0, 7, 0, clear)?;
                self.copy_element_bytes(&mut c, region)?;
                c.bind(clear)?;
                c.constant(7, u64::from(width))?;
                let zero = c.label();
                c.bind(zero)?;
                self.store(&mut c, 0, 5, 0, true, region)?;
                c.addi(5, 5, 1)?;
                c.addi(7, 7, -1)?;
                c.branch(1, 7, 0, zero)?;
                c.stack_store(6, descriptor + 8)?;
            }
            _ => return Err(invalid("unknown element mutation")),
        }
        c.finish()
    }
}
