use super::*;

impl Formatter {
    pub(super) fn format_cell_roles(&mut self, def: &CellRolesDef) -> Result<()> {
        self.push_line(&format!("cell_roles {} {{", def.name));
        self.indent_level += 1;
        self.push_line("limits {");
        self.indent_level += 1;
        for (name, value) in [
            ("input_scan", def.limits.input_scan),
            ("output_scan", def.limits.output_scan),
            ("selected_per_side", def.limits.selected_per_side),
            ("aggregate_bytes", def.limits.aggregate_bytes),
        ] {
            self.push_line(&format!("{name}: {value},"));
        }
        self.indent_level -= 1;
        self.push_line("}");
        for role in &def.roles {
            self.push_line(&format!("{}: {} {{", role.name, format_type(&role.schema)));
            self.indent_level += 1;
            self.push_line(&format!("type_hash: \"0x{}\",", hex::encode(role.type_hash)));
            self.push_line(&format!("lock_hash: \"0x{}\",", hex::encode(role.lock_hash)));
            self.push_line(&format!("identity: {},", role.identity_field));
            let authority = match role.authority {
                CellRoleAuthority::Observe => "observe",
                CellRoleAuthority::LocalType => "local_type",
            };
            self.push_line(&format!("authority: {authority},"));
            let correspondence = match &role.correspondence {
                CellRoleCorrespondence::Preserve => "preserve".into(),
                CellRoleCorrespondence::Create => "create".into(),
                CellRoleCorrespondence::Burn => "burn".into(),
                CellRoleCorrespondence::Merge(key) => format!("merge({})", self.format_expr(&Expr::ByteString(key.clone()))),
            };
            self.push_line(&format!("correspondence: {correspondence},"));
            self.push_line(&format!("inputs: {}..={},", role.inputs.0, role.inputs.1));
            self.push_line(&format!("outputs: {}..={},", role.outputs.0, role.outputs.1));
            self.indent_level -= 1;
            self.push_line("}");
        }
        self.indent_level -= 1;
        self.push_line("}");
        Ok(())
    }
}
