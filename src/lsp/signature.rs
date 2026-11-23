use super::positions::{position_to_offset, word_at_offset};
use super::{
    action_outputs_to_string, param_to_string, type_to_string, LspServer, ParameterInformation, ParameterLabel, Position,
    SignatureHelp, SignatureInformation,
};
use crate::ast::Item;

impl LspServer {
    pub fn signature_help(&self, uri: &str, position: Position) -> Option<SignatureHelp> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;

        let (call_name, active_param) = self.find_call_at_offset(content, offset)?;

        let signature_info = self.find_signature(uri, &call_name)?;

        Some(SignatureHelp { signatures: vec![signature_info], active_signature: Some(0), active_parameter: Some(active_param) })
    }

    fn find_call_at_offset(&self, content: &str, offset: usize) -> Option<(String, u32)> {
        let before = &content[..offset];
        let paren_pos = before.rfind('(')?;

        let _before_paren = &content[..paren_pos];
        let func_name = word_at_offset(content, paren_pos)?.to_string();

        let args_part = &content[paren_pos + 1..offset];
        let active_param = args_part.chars().filter(|c| *c == ',').count() as u32;

        Some((func_name, active_param))
    }

    fn find_signature(&self, uri: &str, name: &str) -> Option<SignatureInformation> {
        if let Some(ast) = self.ast_cache.get(uri)
            && let Some(info) = self.find_signature_in_items(&ast.items, name)
        {
            return Some(info);
        }

        for module in self.workspace_modules(uri).iter() {
            if let Some(info) = self.find_signature_in_items(&module.ast.items, name) {
                return Some(info);
            }
        }

        None
    }

    fn find_signature_in_items(&self, items: &[Item], name: &str) -> Option<SignatureInformation> {
        for item in items {
            match item {
                Item::Action(a) if a.name == name => {
                    let params: Vec<ParameterInformation> = a
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let return_type = if !a.outputs.is_empty() {
                        action_outputs_to_string(&a.outputs)
                    } else {
                        a.return_type.as_ref().map(type_to_string).unwrap_or_default()
                    };
                    let label = format!(
                        "action {}({}) -> {}",
                        a.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        return_type
                    );
                    return Some(SignatureInformation { label, documentation: a.doc_comment.clone(), parameters: params });
                }
                Item::Function(f) if f.name == name => {
                    let params: Vec<ParameterInformation> = f
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let return_type = f.return_type.as_ref().map(type_to_string).unwrap_or_default();
                    let label = format!(
                        "fn {}({}) -> {}",
                        f.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        return_type
                    );
                    return Some(SignatureInformation { label, documentation: f.doc_comment.clone(), parameters: params });
                }
                Item::Lock(l) if l.name == name => {
                    let params: Vec<ParameterInformation> = l
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let label = format!(
                        "lock {}({}) -> {}",
                        l.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        type_to_string(&l.return_type)
                    );
                    return Some(SignatureInformation { label, documentation: None, parameters: params });
                }
                _ => {}
            }
        }
        None
    }
}
