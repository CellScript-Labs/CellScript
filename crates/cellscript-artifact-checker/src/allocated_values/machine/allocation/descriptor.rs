//! Descriptor admission, bounded growth and explicit destruction. The enclosing
//! typed operation still owns argument/value transport and operation selection.
use super::*;
use crate::allocated_values::{key_width, value_width, MemberKind};
use crate::TypedSemanticType;

pub(super) fn multiply(c: &mut Cursor<'_>, rd: u32, left: u32, right: u32) -> Result<(), CheckerError> {
    c.exact((1 << 25) | (right << 20) | (left << 15) | (rd << 7) | 0x33)
}

pub(super) fn layout(site: &Site<'_>, types: &BTreeMap<&str, &TypedSemanticType>) -> Result<(u32, u32, u32), CheckerError> {
    let descriptor = site.descriptor.ok_or_else(|| invalid("missing descriptor coordinate"))?;
    let index = descriptor
        .checked_sub(site.offset + 32 + site.contract.bytes)
        .filter(|offset| offset.is_multiple_of(32))
        .ok_or_else(|| invalid("descriptor is outside its owner members"))?
        / 32;
    let member = site.contract.members.get(index as usize).ok_or_else(|| invalid("unknown descriptor member"))?;
    let width = match &member.shape {
        MemberKind::Vec { item } => value_width(item, types, &mut BTreeSet::new()),
        MemberKind::OrderedSet { key } => key_width(key),
        MemberKind::OrderedMap { key, value } => {
            key_width(key).zip(value_width(value, types, &mut BTreeSet::new())).and_then(|(key, value)| key.checked_add(value))
        }
    }
    .ok_or_else(|| invalid("descriptor has no fixed element layout"))?;
    Ok((descriptor, width, member.items))
}

impl Machine<'_> {
    fn descriptor_load(&self, c: &mut Cursor<'_>, site: &Site<'_>, register: u32, offset: u32) -> Result<(), CheckerError> {
        if let Some(borrowed) = site.borrowed {
            c.stack_load(register, borrowed.pointer)?;
            if offset <= 2047 {
                c.addi(register, register, offset as i32)?;
            } else {
                let scratch = if register == 31 { 30 } else { 31 };
                c.constant(scratch, u64::from(offset))?;
                c.add(register, register, scratch)?;
            }
            c.load(register, register, 0, false)
        } else {
            c.stack_load(register, offset)
        }
    }

    pub(super) fn check_descriptor(
        &self,
        c: &mut Cursor<'_>,
        site: &Site<'_>,
        descriptor: u32,
        width: u32,
        limit: u32,
    ) -> Result<(), CheckerError> {
        let bad = c.label();
        let zero = c.label();
        let done = c.label();
        self.descriptor_load(c, site, 5, descriptor + 24)?;
        c.constant(6, 1)?;
        c.branch(1, 5, 6, bad)?;
        self.descriptor_load(c, site, 6, descriptor + 8)?;
        self.descriptor_load(c, site, 7, descriptor + 16)?;
        c.constant(28, u64::from(limit))?;
        c.branch(6, 7, 6, bad)?;
        c.branch(6, 28, 7, bad)?;
        self.descriptor_load(c, site, 10, descriptor)?;
        c.branch(0, 7, 0, zero)?;
        self.find_arena(c, site.offset, site.contract, site.borrowed.map(|borrowed| borrowed.pointer))?;
        self.descriptor_load(c, site, 7, descriptor + 16)?;
        c.constant(28, u64::from(width))?;
        multiply(c, 28, 7, 28)?;
        c.branch(1, 6, 28, bad)?;
        self.descriptor_load(c, site, 6, descriptor + 8)?;
        c.addi(5, 10, 0)?;
        c.jump(done)?;
        c.bind(zero)?;
        c.branch(1, 10, 0, bad)?;
        c.constant(5, 0)?;
        c.jump(done)?;
        c.bind(bad)?;
        self.failure(c.jump_target()?)?;
        c.bind(done)
    }

    fn lifecycle_child(
        &self,
        c: &mut Cursor<'_>,
        parent: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        let child = children.get(&c.pc).ok_or_else(|| invalid("descriptor has no checked lifecycle child"))?;
        let kind = if parent.kind == Primitive::Reserve { Primitive::Resize } else { Primitive::Release };
        if child.kind != kind
            || child.owner != parent.owner
            || child.offset != parent.offset
            || child.contract != parent.contract
            || child.start <= parent.start
            || child.end > parent.end
            || !consumed.insert(child.start)
        {
            return Err(invalid("descriptor lifecycle child is mismatched or reused"));
        }
        c.pc = child.end;
        Ok(())
    }

    pub(super) fn descriptor(
        &self,
        site: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
        types: &BTreeMap<&str, &TypedSemanticType>,
    ) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 32_768 {
            return Err(invalid("invalid or excessive descriptor extent"));
        }
        let (descriptor, width, limit) = layout(site, types)?;
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        self.check_descriptor(&mut c, site, descriptor, width, limit)?;
        if site.kind == Primitive::Drop {
            c.addi(10, 5, 0)?;
            self.lifecycle_child(&mut c, site, children, consumed)?;
            for field in [0, 8, 16, 24] {
                c.stack_store(0, descriptor + field)?;
            }
        } else if site.kind == Primitive::Reserve {
            let bad = c.label();
            let done = c.target(site.end);
            let enough = c.label();
            let bounded = c.label();
            c.constant(28, u64::from(limit))?;
            c.branch(6, 28, 11, bad)?;
            c.branch(7, 7, 11, done)?;
            c.immediate(0x13, 1, 12, 7, 1)?;
            c.branch(7, 12, 11, enough)?;
            c.addi(12, 11, 0)?;
            c.bind(enough)?;
            c.branch(7, 28, 12, bounded)?;
            c.addi(12, 28, 0)?;
            c.bind(bounded)?;
            c.addi(10, 5, 0)?;
            c.constant(11, u64::from(width))?;
            multiply(&mut c, 11, 12, 11)?;
            self.lifecycle_child(&mut c, site, children, consumed)?;
            c.stack_store(10, descriptor)?;
            c.stack_store(12, descriptor + 16)?;
            c.jump(done)?;
            c.bind(bad)?;
            self.failure(c.jump_target()?)?;
        } else {
            return Err(invalid("unknown descriptor update"));
        }
        c.finish()
    }
}
