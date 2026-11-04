//! Transaction view shapes share syntax, not compiler/checker semantics.
//! Schema arguments remain type nodes; source spelling is an output boundary.
use super::{Type, TypeArgument};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContextView<T> {
    Input(T),
    Output(T),
    CellData(T),
    CellDep,
    HeaderDep,
    WitnessArgs,
    Script,
}

impl<T> ContextView<T> {
    pub fn constructor(&self) -> &'static str {
        match self {
            Self::Input(_) => "InputView",
            Self::Output(_) => "OutputView",
            Self::CellData(_) => "CellDataView",
            Self::CellDep => "CellDepView",
            Self::HeaderDep => "HeaderDepView",
            Self::WitnessArgs => "WitnessArgsView",
            Self::Script => "ScriptView",
        }
    }

    pub fn schema(&self) -> Option<&T> {
        match self {
            Self::Input(schema) | Self::Output(schema) | Self::CellData(schema) => Some(schema),
            Self::CellDep | Self::HeaderDep | Self::WitnessArgs | Self::Script => None,
        }
    }

    pub fn map<U>(&self, mut map: impl FnMut(&T) -> U) -> ContextView<U> {
        match self {
            Self::Input(schema) => ContextView::Input(map(schema)),
            Self::Output(schema) => ContextView::Output(map(schema)),
            Self::CellData(schema) => ContextView::CellData(map(schema)),
            Self::CellDep => ContextView::CellDep,
            Self::HeaderDep => ContextView::HeaderDep,
            Self::WitnessArgs => ContextView::WitnessArgs,
            Self::Script => ContextView::Script,
        }
    }

    pub fn render(&self, render: impl FnOnce(&T) -> String) -> String {
        match self.schema() {
            Some(schema) => format!("{}<{}>", self.constructor(), render(schema)),
            None => self.constructor().to_owned(),
        }
    }
}

impl Type {
    pub fn context_view(view: ContextView<Type>) -> Self {
        let name = view.constructor();
        match view {
            ContextView::Input(schema) | ContextView::Output(schema) | ContextView::CellData(schema) => {
                Self::applied(name, vec![TypeArgument::Type(schema)])
            }
            ContextView::CellDep | ContextView::HeaderDep | ContextView::WitnessArgs | ContextView::Script => Self::Named(name.into()),
        }
    }

    pub fn as_context_view(&self) -> Option<ContextView<&Type>> {
        match self {
            Self::Applied(application) => match application.constructor.as_str() {
                "InputView" => application.unary_type("InputView").map(ContextView::Input),
                "OutputView" => application.unary_type("OutputView").map(ContextView::Output),
                "CellDataView" => application.unary_type("CellDataView").map(ContextView::CellData),
                _ => None,
            },
            Self::Named(name) => match name.as_str() {
                "CellDepView" => Some(ContextView::CellDep),
                "HeaderDepView" => Some(ContextView::HeaderDep),
                "WitnessArgsView" => Some(ContextView::WitnessArgs),
                "ScriptView" => Some(ContextView::Script),
                _ => None,
            },
            _ => None,
        }
    }
}
