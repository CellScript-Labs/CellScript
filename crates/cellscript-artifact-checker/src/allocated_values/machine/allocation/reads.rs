//! Owned result-buffer reads and their conditional/destructive parents. Final
//! conversion from private copied bytes into source values remains a separate obligation.
use super::descriptor::layout;
use super::*;
use crate::TypedSemanticType;

impl Machine<'_> {
    pub(super) fn read_operation(
        &self,
        site: &Site<'_>,
        children: &BTreeMap<u64, &Site<'_>>,
        consumed: &mut BTreeSet<u64>,
        types: &BTreeMap<&str, &TypedSemanticType>,
    ) -> Result<(), CheckerError> {
        if site.start >= site.end || site.end - site.start > 131_072 {
            return Err(invalid("invalid or excessive read operation extent"));
        }
        let mut c = Cursor::new(site.start, site.end, self.words, self.targets);
        let done = c.target(site.end);
        match site.kind {
            Primitive::Lookup | Primitive::Required => {
                if !super::ordered::key(site)?.2 {
                    return Err(invalid("map lookup targets a set"));
                }
                if site.kind == Primitive::Required {
                    self.member_child(&mut c, site, Primitive::Lookup, children, consumed)?;
                    c.branch(1, 10, 0, done)?;
                    self.failure(c.jump_target()?)?;
                } else {
                    self.member_child(&mut c, site, Primitive::Find, children, consumed)?;
                    c.branch(0, 10, 0, done)?;
                    self.member_child(&mut c, site, Primitive::Read, children, consumed)?;
                    c.constant(10, 1)?;
                }
            }
            Primitive::Take | Primitive::Pop => {
                if site.kind == Primitive::Pop {
                    let (descriptor, width, limit) = layout(site, types)?;
                    self.check_descriptor(&mut c, site, descriptor, width, limit)?;
                    c.addi(11, 6, -1)?;
                }
                self.member_child(&mut c, site, Primitive::Read, children, consumed)?;
                self.member_child(&mut c, site, Primitive::Remove, children, consumed)?;
            }
            _ => return Err(invalid("unknown read operation")),
        }
        c.finish()
    }
}
