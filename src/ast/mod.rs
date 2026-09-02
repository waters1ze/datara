use crate::diagnostics::SourceSpan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub declarations: Vec<Decl>,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decl {
    Use(UseDecl),
    Class(ClassDecl),
    Behavior(BehaviorDecl),
    Component(ComponentDecl),
    Role(RoleDecl),
    Function(FunctionDecl),
    Flow(FunctionDecl),
    Task(FunctionDecl),
    Packet(PacketDecl),
    ExternFn(ExternFnDecl),
}

impl Decl {
    pub fn span(&self) -> &SourceSpan {
        match self {
            Decl::Use(d) => &d.span,
            Decl::Class(d) => &d.span,
            Decl::Behavior(d) => &d.span,
            Decl::Component(d) => &d.span,
            Decl::Role(d) => &d.span,
            Decl::Function(d) | Decl::Flow(d) | Decl::Task(d) => &d.span,
            Decl::Packet(d) => &d.span,
            Decl::ExternFn(d) => &d.span,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseDecl {
    pub path: Vec<String>,
    pub group: Vec<String>,
    pub alias: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub base_type: Option<String>,
    pub compositions: Vec<String>,
    pub body_items: Vec<ClassItem>,
    pub is_export: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorDecl {
    pub target_type: String,
    pub body_items: Vec<ClassItem>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDecl {
    pub name: String,
    pub body_items: Vec<ClassItem>,
    pub is_export: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDecl {
    pub name: String,
    pub methods: Vec<MethodDecl>,
    pub is_export: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub generic_constraints: Vec<(String, String)>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeNode>,
    pub body: Box<Stmt>,
    pub is_expression_body: bool,
    pub is_export: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClassItem {
    Field(FieldDecl),
    Method(MethodDecl),
    Using(String, SourceSpan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketField {
    pub name: String,
    pub bits: usize,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketDecl {
    pub name: String,
    pub fields: Vec<PacketField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternFnDecl {
    pub abi: String,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeNode>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDecl {
    pub name: String,
    pub type_node: Option<TypeNode>,
    pub default_value: Option<Expr>,
    pub is_mut: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodDecl {
    pub name: String,
    pub generic_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeNode>,
    pub body: Option<Box<Stmt>>,
    pub is_expression_body: bool,
    pub is_replaces: bool,
    pub replaces_target: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub type_node: Option<TypeNode>,
    pub ownership_mode: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeNode {
    pub name: String,
    pub generic_args: Vec<TypeNode>,
    pub is_option: bool,
    pub error_type: Option<Box<TypeNode>>,
    pub span: SourceSpan,
}

impl TypeNode {
    pub fn full_type_name(&self) -> String {
        if self.generic_args.is_empty() {
            self.name.clone()
        } else {
            let args = self
                .generic_args
                .iter()
                .map(|a| a.full_type_name())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", self.name, args)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Stmt {
    Block(Vec<Stmt>, SourceSpan),
    Let {
        name: String,
        type_node: Option<TypeNode>,
        init: Expr,
        span: SourceSpan,
    },
    Mut {
        name: String,
        type_node: Option<TypeNode>,
        init: Expr,
        span: SourceSpan,
    },
    Const {
        name: String,
        type_node: Option<TypeNode>,
        init: Expr,
        span: SourceSpan,
    },
    Val {
        name: String,
        type_node: Option<TypeNode>,
        init: Expr,
        is_mut: bool,
        span: SourceSpan,
    },
    CompactBind {
        name: String,
        init: Expr,
        span: SourceSpan,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: SourceSpan,
    },
    Expr(Expr, SourceSpan),
    Out(Expr, SourceSpan),
    Err(Expr, SourceSpan),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
        span: SourceSpan,
    },
    For {
        var_name: String,
        iterable: Expr,
        body: Box<Stmt>,
        span: SourceSpan,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
        span: SourceSpan,
    },
    Loop {
        body: Box<Stmt>,
        span: SourceSpan,
    },
    TryCatch {
        try_block: Box<Stmt>,
        err_var: String,
        catch_block: Box<Stmt>,
        span: SourceSpan,
    },
    Parallel(Box<Stmt>, SourceSpan),
    ParallelFor {
        var_name: String,
        iterable: Expr,
        body: Box<Stmt>,
        span: SourceSpan,
    },
    With {
        resource_name: String,
        init: Expr,
        body: Box<Stmt>,
        span: SourceSpan,
    },
    Return(Option<Expr>, SourceSpan),
}

impl Stmt {
    pub fn span(&self) -> &SourceSpan {
        match self {
            Stmt::Block(_, s)
            | Stmt::Let { span: s, .. }
            | Stmt::Mut { span: s, .. }
            | Stmt::Const { span: s, .. }
            | Stmt::Val { span: s, .. }
            | Stmt::CompactBind { span: s, .. }
            | Stmt::Assign { span: s, .. }
            | Stmt::Expr(_, s)
            | Stmt::Out(_, s)
            | Stmt::Err(_, s)
            | Stmt::If { span: s, .. }
            | Stmt::For { span: s, .. }
            | Stmt::While { span: s, .. }
            | Stmt::Loop { span: s, .. }
            | Stmt::TryCatch { span: s, .. }
            | Stmt::Parallel(_, s)
            | Stmt::ParallelFor { span: s, .. }
            | Stmt::With { span: s, .. }
            | Stmt::Return(_, s) => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Literal(LiteralValue, SourceSpan),
    Identifier(String, SourceSpan),
    InterpolatedString {
        parts: Vec<String>,
        expressions: Vec<Expr>,
        span: SourceSpan,
    },
    Binary {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
        span: SourceSpan,
    },
    Unary {
        op: String,
        expr: Box<Expr>,
        span: SourceSpan,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: SourceSpan,
    },
    MemberAccess {
        object: Box<Expr>,
        member: String,
        span: SourceSpan,
    },
    IndexAccess {
        object: Box<Expr>,
        index: Box<Expr>,
        span: SourceSpan,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        span: SourceSpan,
    },
    Tuple(Vec<Expr>, SourceSpan),
    ObjectInit {
        class_name: String,
        generic_args: Vec<TypeNode>,
        fields: Vec<(String, Expr)>,
        span: SourceSpan,
    },
    Pipeline {
        stages: Vec<Expr>,
        span: SourceSpan,
    },
    Decide {
        arms: Vec<DecideArm>,
        else_arm: Option<Box<Expr>>,
        span: SourceSpan,
    },
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
        span: SourceSpan,
    },
    Select {
        arms: Vec<SelectArm>,
        else_arm: Option<Box<Expr>>,
        span: SourceSpan,
    },
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
        span: SourceSpan,
    },
    ListLiteral(Vec<Expr>, SourceSpan),
    MapLiteral(Vec<(Expr, Expr)>, SourceSpan),
    ErrorPropagate(Box<Expr>, SourceSpan),
    OrRecovery {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
        span: SourceSpan,
    },
    ArrayRepeatLiteral {
        elem: Box<Expr>,
        count: usize,
        span: SourceSpan,
    },
}

impl Expr {
    pub fn span(&self) -> &SourceSpan {
        match self {
            Expr::Literal(_, s)
            | Expr::Identifier(_, s)
            | Expr::InterpolatedString { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::MemberAccess { span: s, .. }
            | Expr::IndexAccess { span: s, .. }
            | Expr::Range { span: s, .. }
            | Expr::Tuple(_, s)
            | Expr::ObjectInit { span: s, .. }
            | Expr::Pipeline { span: s, .. }
            | Expr::Decide { span: s, .. }
            | Expr::Match { span: s, .. }
            | Expr::Select { span: s, .. }
            | Expr::Lambda { span: s, .. }
            | Expr::ListLiteral(_, s)
            | Expr::MapLiteral(_, s)
            | Expr::ErrorPropagate(_, s)
            | Expr::OrRecovery { span: s, .. }
            | Expr::ArrayRepeatLiteral { span: s, .. } => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideArm {
    pub condition: Expr,
    pub body: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Pattern {
    Wildcard(SourceSpan),
    Identifier(String, SourceSpan),
    Literal(LiteralValue, SourceSpan),
    Variant {
        enum_name: Option<String>,
        variant_name: String,
        bindings: Vec<String>,
        span: SourceSpan,
    },
}

impl Pattern {
    pub fn span(&self) -> &SourceSpan {
        match self {
            Pattern::Wildcard(s)
            | Pattern::Identifier(_, s)
            | Pattern::Literal(_, s)
            | Pattern::Variant { span: s, .. } => s,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectArm {
    pub condition: Expr,
    pub body: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
    None,
}
