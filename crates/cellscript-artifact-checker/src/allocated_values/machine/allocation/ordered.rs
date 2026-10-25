//! Owned ordered search and update control flow. The descriptor fixes key
//! ordering and stride; checked children fix mutation at the returned ordinal.
//! Input staging and source-level operation ordering remain outer obligations.
use super::descriptor::{layout, multiply};
use super::*;
use crate::allocated_values::{key_width, MemberKind};
use crate::TypedSemanticType;

pub(super) fn key(site: &Site<'_>) -> Result<(u32, bool, bool), CheckerError> {
    let descriptor = site.descriptor.ok_or_else(|| invalid("ordered operation has no descriptor"))?;
    let index = descriptor
        .checked_sub(site.offset + 32 + site.contract.bytes)
        .filter(|offset| offset.is_multiple_of(32))
        .ok_or_else(|| invalid("ordered member coordinate is invalid"))?
        / 32;
    let member = site.contract.members.get(index as usize).ok_or_else(|| invalid("ordered member is missing"))?;
    let (ty, map) = match &member.shape {
        MemberKind::OrderedMap { key, .. } => (key, true),
        MemberKind::OrderedSet { key } => (key, false),
        _ => return Err(invalid("ordered operation targets an unordered vector")),
    };
    let width = key_width(ty).ok_or_else(|| invalid("unsupported ordered key"))?;
    Ok((width, matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64"), map))
}

impl Machine<'_> {
    pub(super) fn find(&self, site: &Site<'_>, types: &BTreeMap<&str, &TypedSemanticType>) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 32_768 {
            return Err(invalid("invalid or excessive ordered search extent"));
        }
        let (descriptor, width, limit) = layout(site, types)?;
        let (key_width, numeric, _) = key(site)?;
        let scratch = site.scratch();
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        self.check_descriptor(&mut c, site, descriptor, width, limit)?;
        let search = c.label();
        let compare = c.label();
        let equal = c.label();
        let less = c.label();
        let greater = c.label();
        let found = c.label();
        let absent = c.label();
        let done = c.target(site.end);
        c.constant(12, 0)?;
        c.addi(13, 6, 0)?;
        c.addi(15, 5, 0)?;
        c.bind(search)?;
        c.branch(7, 12, 13, absent)?;
        c.add(14, 12, 13)?;
        c.immediate(0x13, 5, 14, 14, 1)?;
        c.constant(7, u64::from(width))?;
        multiply(&mut c, 7, 14, 7)?;
        c.add(28, 15, 7)?;
        c.stack_address(29, scratch)?;
        c.constant(16, u64::from(key_width))?;
        let step = if numeric {
            c.addi(28, 28, key_width as i32 - 1)?;
            c.addi(29, 29, key_width as i32 - 1)?;
            -1
        } else {
            1
        };
        c.bind(compare)?;
        c.load(30, 28, 0, true)?;
        c.load(31, 29, 0, true)?;
        c.branch(0, 30, 31, equal)?;
        c.branch(6, 30, 31, less)?;
        c.jump(greater)?;
        c.bind(equal)?;
        c.addi(16, 16, -1)?;
        c.branch(0, 16, 0, found)?;
        c.addi(28, 28, step)?;
        c.addi(29, 29, step)?;
        c.jump(compare)?;
        c.bind(less)?;
        c.addi(12, 14, 1)?;
        c.jump(search)?;
        c.bind(greater)?;
        c.addi(13, 14, 0)?;
        c.jump(search)?;
        c.bind(found)?;
        c.constant(10, 1)?;
        c.addi(11, 14, 0)?;
        c.jump(done)?;
        c.bind(absent)?;
        c.constant(10, 0)?;
        c.addi(11, 12, 0)?;
        c.finish()
    }

    pub(super) fn ordered_update(
        &self,
        site: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
    ) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 131_072 {
            return Err(invalid("invalid or excessive ordered update extent"));
        }
        let (_, _, map) = key(site)?;
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        let mut child =
            |c: &mut Cursor<'_>, kind| -> Result<(), CheckerError> { self.member_child(c, site, kind, children, consumed) };
        child(&mut c, Primitive::Find)?;
        let done = c.target(site.end);
        if site.kind == Primitive::Put {
            let insert = c.label();
            c.branch(0, 10, 0, insert)?;
            if map {
                child(&mut c, Primitive::Set)?;
            }
            c.constant(10, 0)?;
            c.jump(done)?;
            c.bind(insert)?;
            child(&mut c, Primitive::Insert)?;
        } else if site.kind == Primitive::Erase {
            c.branch(0, 10, 0, done)?;
            child(&mut c, Primitive::Remove)?;
        } else {
            return Err(invalid("unknown ordered update"));
        }
        c.constant(10, 1)?;
        c.finish()
    }
}
