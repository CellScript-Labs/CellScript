use super::*;

impl Formatter {
    pub(super) fn format_data_shape(&mut self, def: &DataShapeDef) -> Result<()> {
        self.push_line(&format!("data_shape {} {{", def.name));
        self.indent_level += 1;
        self.push_line(&format!("element: {},", format_type(&def.element)));
        self.push_line("limits {");
        self.indent_level += 1;
        for (name, value) in
            [("items", def.limits.items), ("payload_bytes", def.limits.payload_bytes), ("encoded_bytes", def.limits.encoded_bytes)]
        {
            self.push_line(&format!("{name}: {value},"));
        }
        self.indent_level -= 1;
        self.push_line("}");
        self.indent_level -= 1;
        self.push_line("}");
        Ok(())
    }
}
