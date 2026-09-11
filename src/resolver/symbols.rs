use crate::ast::TypeNode;
use crate::diagnostics::SourceSpan;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub is_mut: bool,
    pub is_export: bool,
    pub span: SourceSpan,
    pub fields: HashMap<String, Symbol>,
    pub methods: HashMap<String, Symbol>,
    pub base_type: Option<String>,
    pub compositions: Vec<String>,
    pub generic_params: Vec<String>,
    pub type_node: Option<TypeNode>,
    pub return_type: Option<TypeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Param,
    Function,
    Class,
    Component,
    Role,
    Trait,
    Field,
    Method,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub symbols: HashMap<String, Symbol>,
}

impl Scope {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            symbols: HashMap::new(),
        }
    }

    pub fn define(&mut self, name: String, sym: Symbol) {
        self.symbols.insert(name, sym);
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}
