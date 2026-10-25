use super::*;

impl Formatter {
    pub(super) fn format_collections(&mut self, def: &CollectionsDef) -> Result<()> {
        self.push_line(&format!("collections {} {{", def.name));
        self.indent_level += 1;
        self.push_line("limits {");
        self.indent_level += 1;
        self.push_line(&format!("bytes: {},", def.bytes));
        self.push_line(&format!("allocations: {},", def.allocations));
        self.indent_level -= 1;
        self.push_line("},");
        for member in &def.members {
            let ty = match &member.kind {
                ValueCollectionKind::Vec(element) => format!("Vec<{}>", format_type(element)),
                ValueCollectionKind::OrderedMap { key, value } => format!("OrderedMap<{}, {}>", format_type(key), format_type(value)),
                ValueCollectionKind::OrderedSet(key) => format!("OrderedSet<{}>", format_type(key)),
            };
            self.push_line(&format!("{}: {ty} {{ items: {} }},", member.name, member.items));
        }
        self.indent_level -= 1;
        self.push_line("}");
        Ok(())
    }
}
