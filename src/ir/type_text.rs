//! Explicit output spellings for retained IR types. These formats preserve
//! existing wire bytes; none supplies nominal identity or executable admission.
use super::IrType;

#[derive(Clone, Copy)]
enum Format {
    Source,
    TypedWire,
    AllocatedMemberWire,
}

impl IrType {
    pub(crate) fn source_text(&self) -> String {
        self.render_text(Format::Source)
    }

    // Native typed-record emission is absent from the browser metadata build.
    #[cfg_attr(feature = "wasm", allow(dead_code))]
    pub(crate) fn typed_wire_text(&self) -> String {
        self.render_text(Format::TypedWire)
    }

    pub(crate) fn allocated_member_wire_text(&self) -> String {
        self.render_text(Format::AllocatedMemberWire)
    }

    fn render_text(&self, format: Format) -> String {
        // Allocated member records historically retain source spelling inside
        // aggregates/references, while typed semantics retain wire leaf names.
        let child_format = match format {
            Format::AllocatedMemberWire => Format::Source,
            format => format,
        };
        let child = |ty: &IrType| ty.render_text(child_format);
        match self {
            IrType::U8 => "u8".into(),
            IrType::U16 => "u16".into(),
            IrType::U32 => "u32".into(),
            IrType::I32 => "i32".into(),
            IrType::U64 => "u64".into(),
            IrType::U128 => "u128".into(),
            IrType::Bool => "bool".into(),
            IrType::Unit => if matches!(format, Format::TypedWire) { "unit" } else { "()" }.into(),
            IrType::Address => if matches!(format, Format::Source) { "Address" } else { "address" }.into(),
            IrType::Hash => if matches!(format, Format::Source) { "Hash" } else { "hash" }.into(),
            IrType::Array(inner, len) => format!("[{}; {}]", child(inner), len),
            IrType::Tuple(items) => format!("({})", items.iter().map(child).collect::<Vec<_>>().join(", ")),
            IrType::Ref(inner) => format!("&{}", child(inner)),
            IrType::MutRef(inner) => format!("&mut {}", child(inner)),
            IrType::Named(name) => name.clone(),
            // Application/family arguments have an existing source-spelling
            // wire domain. Do not recursively lowercase or normalize them as
            // a side effect of consolidating this traversal.
            IrType::Applied(application) => application.render(", ", IrType::source_text),
            IrType::Selection(selection) => selection.encode(),
            IrType::Vector(None) => "Vec".into(),
            IrType::Vector(Some(element)) => format!("Vec<{}>", element.source_text()),
            IrType::Context(view) => view.render(|schema| schema.source_text()),
            IrType::Bounded(collection) => collection.render(IrType::source_text),
            IrType::Commitment(value) => format!("Commitment<{}>", value.source_text()),
            IrType::Opening(value) => format!("Opening<{}>", value.source_text()),
            IrType::OpenHandle(handle) => handle.source_text(),
            IrType::WitnessBytes(view) => format!("WitnessBytesView<{},{}>", view.owner.as_str(), view.maximum),
            IrType::Nested(view) => view.canonical_text(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_contracts_preserve_nested_unit_case_and_nominal_spelling() {
        let aggregate =
            IrType::Array(Box::new(IrType::Tuple(vec![IrType::Hash, IrType::Unit, IrType::Named("owner::AddressBook".into())])), 2);
        assert_eq!(aggregate.source_text(), "[(Hash, (), owner::AddressBook); 2]");
        assert_eq!(aggregate.typed_wire_text(), "[(hash, unit, owner::AddressBook); 2]");
        assert_eq!(aggregate.allocated_member_wire_text(), aggregate.source_text());
        assert_eq!(IrType::Hash.allocated_member_wire_text(), "hash");
        assert_eq!(IrType::Address.allocated_member_wire_text(), "address");
        assert_eq!(IrType::Unit.allocated_member_wire_text(), "()");

        let vector = IrType::vector(IrType::Tuple(vec![IrType::Hash, IrType::Unit]));
        assert_eq!(vector.typed_wire_text(), "Vec<(Hash, ())>");
        assert_eq!(IrType::Ref(Box::new(aggregate)).typed_wire_text(), "&[(hash, unit, owner::AddressBook); 2]");
    }
}
