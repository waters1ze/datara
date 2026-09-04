pub mod ast;
pub mod cli;
pub mod codegen;
pub mod diagnostics;
pub mod dmir;
pub mod doc;
pub mod driver;
pub mod effects;
pub mod export;
pub mod fmt;
pub mod incremental;
pub mod lexer;
pub mod lint;
pub mod lsp;
pub mod optimizer;
pub mod ownership;
pub mod parser;
pub mod pgo;
pub mod project;
pub mod repl;
pub mod resolver;
pub mod runtime;
pub mod security;
pub mod semantic_graph;
pub mod stdlib;
pub mod types;
pub mod update;

pub use driver::ForgenCompiler;
pub use project::{
    DataraManifest, ProjectDiscovery, ProjectInitializer, ProjectLayout, ProjectRunner,
};
