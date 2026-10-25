//! Internal nominal types for retained catalogs and role projections. Names are
//! committed by the constructor contract; projections retain the entire catalog
//! so aliases cannot erase the ordinal-to-schema binding.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionType {
    pub catalog: Option<String>,
    pub names: Vec<String>,
    pub schemas: Vec<String>,
    pub selected: Option<u32>,
}

impl SelectionType {
    pub fn encode(&self) -> String {
        let mut parts = Vec::new();
        if let Some(catalog) = &self.catalog {
            if let Some(role) = self.selected {
                parts.push(format!("RoleAt<{role}>"));
            }
            parts.push(format!("Catalog<catalog_{catalog}>"));
            parts.extend(self.names.iter().zip(&self.schemas).map(|(name, schema)| format!("NamedRole<role_{name},{schema}>")));
        } else {
            parts.extend(self.schemas.iter().cloned());
        }
        format!("ExactCellSelection<{}>", parts.join(","))
    }
}

pub(super) fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn wrapped<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value.strip_prefix(prefix)?.strip_suffix('>')
}

/// The grammar is bounded, canonical and independent of the compiler parser.
/// Reject extra delimiters, mixed named/positional roles and noncanonical IDs.
pub fn parse_selection_type(value: &str) -> Option<SelectionType> {
    let body = wrapped(value, "ExactCellSelection<")?;
    let mut parts = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (index, byte) in body.bytes().enumerate() {
        match byte {
            b'<' => {
                depth = depth.checked_add(1)?;
                if depth > 1 {
                    return None;
                }
            }
            b'>' => depth = depth.checked_sub(1)?,
            b',' if depth == 0 => {
                parts.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    parts.push(&body[start..]);
    if parts.len() > 10 {
        return None;
    }
    let selected = if let Some(ordinal) = wrapped(parts[0], "RoleAt<") {
        let ordinal: u32 = ordinal.parse().ok()?;
        parts.remove(0);
        Some(ordinal)
    } else {
        None
    };
    let catalog = parts.first().and_then(|part| wrapped(part, "Catalog<catalog_")).map(str::to_owned);
    let mut names = Vec::new();
    let mut schemas = Vec::new();
    if let Some(catalog) = &catalog {
        if !identifier(catalog) {
            return None;
        }
        parts.remove(0);
        for part in parts {
            let (name, schema) = wrapped(part, "NamedRole<role_")?.split_once(',')?;
            if !identifier(name) || names.iter().any(|previous| previous == name) {
                return None;
            }
            names.push(name.to_owned());
            schemas.push(schema.to_owned());
        }
    } else {
        if selected.is_some() {
            return None;
        }
        schemas.extend(parts.into_iter().map(str::to_owned));
    }
    if !(1..=8).contains(&schemas.len())
        || schemas.iter().any(|schema| !schema.split("::").all(identifier))
        || selected.is_some_and(|role| role as usize >= schemas.len())
    {
        return None;
    }
    let parsed = SelectionType { catalog, names, schemas, selected };
    (parsed.encode() == value).then_some(parsed)
}

pub fn project_role_type(owner: &str, field: &str) -> Option<String> {
    let mut selection = parse_selection_type(owner)?;
    if selection.selected.is_some() {
        return None;
    }
    selection.selected = Some(selection.names.iter().position(|name| name == field)? as u32);
    Some(selection.encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_selection_types_preserve_roles_and_reject_ambiguous_shapes() {
        let root = "ExactCellSelection<Catalog<catalog_Settlement>,NamedRole<role_orders,Order>,NamedRole<role_fees,Fee>>";
        let projected = project_role_type(root, "fees").unwrap();
        let parsed = parse_selection_type(&projected).unwrap();
        assert_eq!(parsed.selected, Some(1));
        assert_eq!(parsed.schemas, ["Order", "Fee"]);
        assert!(project_role_type(&projected, "orders").is_none());
        assert!(project_role_type(root, "absent").is_none());
        for bad in [
            "ExactCellSelection<>",
            "ExactCellSelection<RoleAt<0>>",
            "ExactCellSelection<Catalog<catalog_S>>",
            "ExactCellSelection<Catalog<catalog_S>,Order>",
            "ExactCellSelection<RoleAt<01>,Catalog<catalog_S>,NamedRole<role_orders,Order>,NamedRole<role_fees,Fee>>",
            "ExactCellSelection<RoleAt<1>,Catalog<catalog_S>,NamedRole<role_orders,Order>>",
            "ExactCellSelection<Catalog<catalog_S>,NamedRole<role_orders,Order>,NamedRole<role_orders,Fee>>",
            "ExactCellSelection<Catalog<catalog_S>,NamedRole<role_orders,Order,Fee>>",
            "ExactCellSelection<Catalog<catalog_S>,NamedRole<role_orders,Vec<Order>>>",
            "ExactCellSelection<RoleAt<0>,Order>",
        ] {
            assert!(parse_selection_type(bad).is_none(), "{bad}");
        }
        assert!(parse_selection_type("ExactCellSelection<module::Order,Fee>").is_some());
        // User role/catalog identifiers are not ABI type keywords. Preserve
        // case even when an identifier happens to spell a builtin type name.
        let named = SelectionType {
            catalog: Some("Hash".into()),
            names: vec!["Address".into()],
            schemas: vec!["Order".into()],
            selected: None,
        };
        let canonical = crate::checker::canonical_abi_type(&named.encode());
        assert_eq!(parse_selection_type(&canonical), Some(named));
    }
}
