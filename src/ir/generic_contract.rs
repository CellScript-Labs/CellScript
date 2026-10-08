//! Declaration-origin evidence survives specialization, import aliases and
//! entry selection. It is not executable code or a producer validation flag.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrGenericKind {
    Struct,
    Enum,
    Function,
}

impl IrGenericKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Function => "function",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrGenericParameter {
    pub name: String,
    pub constraints: Vec<ValueAbility>,
    pub phantom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrGenericContract {
    pub kind: IrGenericKind,
    pub module: String,
    pub template: String,
    pub concrete_name: String,
    pub arguments: Vec<String>,
    pub parameters: Vec<IrGenericParameter>,
    pub declaration: IrGenericDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrGenericDeclaration {
    Struct { fields: Vec<(String, IrType)>, abilities: Vec<ValueAbility> },
    Enum { variants: Vec<(String, Vec<IrType>)>, abilities: Vec<ValueAbility> },
    Function { params: Vec<IrGenericValueParameter>, return_type: IrType },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrGenericValueParameter {
    pub name: String,
    pub ty: IrType,
    pub source: ParamSource,
    pub mutable: bool,
    pub reference: bool,
}

impl IrGenericContract {
    pub(super) fn from_ast(ast: &Module, name: &str, kind: IrGenericKind) -> Result<Option<Self>> {
        let Some((template, arguments)) = crate::generics::decode_monomorph_name(name) else { return Ok(None) };
        let builtin;
        let definition = ast.interface_templates.iter().find(|item| match (kind, item) {
            (IrGenericKind::Struct, Item::Struct(def)) => def.name == template,
            (IrGenericKind::Enum, Item::Enum(def)) => def.name == template,
            (IrGenericKind::Function, Item::Function(def)) => def.name == template,
            _ => false,
        });
        let definition = match definition {
            Some(definition) => definition,
            None if kind == IrGenericKind::Enum && template == "Option" => {
                builtin = Item::Enum(crate::generics::builtin_option_template());
                &builtin
            }
            None => {
                return Err(CompileError::without_span(format!(
                    "specialization '{name}' has no retained declaration contract in '{}'",
                    ast.name
                )))
            }
        };
        let (parameters, declaration) = match definition {
            Item::Struct(def) => (
                &def.type_params,
                IrGenericDeclaration::Struct {
                    fields: def.fields.iter().map(|field| (field.name.clone(), ast_type_to_ir(&field.ty))).collect(),
                    abilities: def.abilities.clone(),
                },
            ),
            Item::Enum(def) => (
                &def.type_params,
                IrGenericDeclaration::Enum {
                    variants: def
                        .variants
                        .iter()
                        .map(|variant| (variant.name.clone(), variant.fields.iter().map(ast_type_to_ir).collect()))
                        .collect(),
                    abilities: def.abilities.clone(),
                },
            ),
            Item::Function(def) => (
                &def.type_params,
                IrGenericDeclaration::Function {
                    params: def
                        .params
                        .iter()
                        .map(|param| IrGenericValueParameter {
                            name: param.name.clone(),
                            ty: ast_type_to_ir(&param.ty),
                            source: param.source,
                            mutable: param.is_mut,
                            reference: param.is_ref || param.is_read_ref,
                        })
                        .collect(),
                    return_type: def.return_type.as_ref().map(ast_type_to_ir).unwrap_or(IrType::Unit),
                },
            ),
            _ => unreachable!("generic declaration kind selected above"),
        };
        if parameters.len() != arguments.len() {
            return Err(CompileError::without_span("specialization declaration arity differs from its identity"));
        }
        Ok(Some(Self {
            kind,
            module: ast.name.clone(),
            template,
            concrete_name: name.into(),
            arguments,
            declaration,
            parameters: parameters
                .iter()
                .map(|param| IrGenericParameter {
                    name: param.name.clone(),
                    constraints: param.constraints.clone(),
                    phantom: param.phantom,
                })
                .collect(),
        }))
    }
}
