use crate::ast::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use crate::resolver::Resolver;
use std::collections::{HashMap, HashSet};

/// Message prefix of the diagnostic emitted for a plain *read* of a moved
/// value (`Expr::Identifier` path below). Only these second-pass reports are
/// promoted to real diagnostics by `check_loop_body_twice`.
const LOOP_CARRIED_USE_MARKER: &str = "Use of moved value";

/// Extracts the first single-quoted token from a diagnostic message (the
/// variable name), used as the variable half of the loop second-pass dedup
/// key `(variable, error code)`.
fn first_quoted(message: &str) -> String {
    let start = match message.find('\'') {
        Some(i) => i + 1,
        None => return String::new(),
    };
    match message[start..].find('\'') {
        Some(end) => message[start..start + end].to_string(),
        None => message[start..].to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueState {
    Active,
    Moved { at_span: SourceSpan, reason: String },
}

#[derive(Debug, Clone)]
pub struct BorrowRecord {
    pub borrower: String,
    pub is_mut: bool,
    pub span: SourceSpan,
    pub scope_depth: usize,
}

#[derive(Clone)]
pub struct OwnershipTracker<'a> {
    pub resolver: &'a Resolver,
    pub states: HashMap<String, ValueState>,
    pub active_borrows: HashMap<String, Vec<BorrowRecord>>,
    pub local_bindings: Vec<String>,
    pub mut_bindings: HashSet<String>,
    pub immutable_bindings: HashSet<String>,
    pub current_scope: usize,
    pub bindings_by_scope: HashMap<usize, Vec<String>>,
    /// Names of classes declared in the program. Values of class type are
    /// owned heap values: passing them by value into a function moves them.
    class_names: HashSet<String>,
    /// User-declared function signatures: name -> parameter list (with
    /// ownership modes) so call sites can be checked interprocedurally.
    fn_signatures: HashMap<String, Vec<crate::ast::Param>>,
}

impl<'a> OwnershipTracker<'a> {
    pub fn new(resolver: &'a Resolver) -> Self {
        Self {
            resolver,
            states: HashMap::new(),
            active_borrows: HashMap::new(),
            local_bindings: Vec::new(),
            mut_bindings: HashSet::new(),
            immutable_bindings: HashSet::new(),
            current_scope: 0,
            bindings_by_scope: HashMap::new(),
            class_names: HashSet::new(),
            fn_signatures: HashMap::new(),
        }
    }

    pub fn check_program(&mut self, program: &Program, diag: &mut DiagnosticEngine) {
        // Collect declared class names and function signatures once, so call
        // sites can be checked interprocedurally (pass-by-value of class-typed
        // values moves them).
        for decl in &program.declarations {
            match decl {
                Decl::Class(c) => {
                    self.class_names.insert(c.name.clone());
                }
                Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                    self.fn_signatures.insert(f.name.clone(), f.params.clone());
                }
                _ => {}
            }
        }
        for decl in &program.declarations {
            self.check_decl(decl, diag);
        }
    }

    fn enter_scope(&mut self) {
        self.current_scope += 1;
        self.bindings_by_scope
            .entry(self.current_scope)
            .or_default()
            .clear();
    }

    fn exit_scope(&mut self, depth: usize) {
        // Release active borrows created in this scope or deeper
        for borrows in self.active_borrows.values_mut() {
            borrows.retain(|b| b.scope_depth < depth);
        }
        self.active_borrows.retain(|_, borrows| !borrows.is_empty());

        if let Some(bindings) = self.bindings_by_scope.remove(&depth) {
            for b in bindings {
                self.local_bindings.retain(|x| x != &b);
                self.mut_bindings.remove(&b);
                self.immutable_bindings.remove(&b);
                self.states.remove(&b);
            }
        }
    }

    fn check_decl(&mut self, decl: &Decl, diag: &mut DiagnosticEngine) {
        match decl {
            Decl::Function(f) | Decl::Flow(f) | Decl::Task(f) => {
                self.states.clear();
                self.active_borrows.clear();
                self.local_bindings.clear();
                self.mut_bindings.clear();
                self.immutable_bindings.clear();
                self.current_scope = 0;
                self.bindings_by_scope.clear();

                for p in &f.params {
                    self.states.insert(p.name.clone(), ValueState::Active);
                }
                self.check_stmt(&f.body, diag, true);
            }
            Decl::Class(c) => {
                for item in &c.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        self.states.clear();
                        self.active_borrows.clear();
                        self.local_bindings.clear();
                        self.mut_bindings.clear();
                        self.immutable_bindings.clear();
                        self.current_scope = 0;
                        self.bindings_by_scope.clear();
                        self.states.insert("this".to_string(), ValueState::Active);
                        for p in &m.params {
                            self.states.insert(p.name.clone(), ValueState::Active);
                        }
                        self.check_stmt(body, diag, true);
                    }
                }
            }
            Decl::Behavior(b) => {
                for item in &b.body_items {
                    if let ClassItem::Method(m) = item
                        && let Some(body) = &m.body
                    {
                        self.states.clear();
                        self.active_borrows.clear();
                        self.local_bindings.clear();
                        self.mut_bindings.clear();
                        self.immutable_bindings.clear();
                        self.current_scope = 0;
                        self.bindings_by_scope.clear();
                        self.states.insert("this".to_string(), ValueState::Active);
                        for p in &m.params {
                            self.states.insert(p.name.clone(), ValueState::Active);
                        }
                        self.check_stmt(body, diag, true);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt, diag: &mut DiagnosticEngine, _is_root_fn: bool) {
        match stmt {
            Stmt::Block(stmts, _) => {
                self.enter_scope();
                for s in stmts {
                    self.check_stmt(s, diag, false);
                }
                self.exit_scope(self.current_scope);
                self.current_scope -= 1;
            }
            Stmt::Let {
                name, init, span, ..
            }
            | Stmt::Const {
                name, init, span, ..
            } => {
                self.check_expr_usage(init, diag);
                self.handle_binding(name, init, false, span, diag);
            }
            Stmt::CompactBind { name, init, span } => {
                self.check_expr_usage(init, diag);
                self.handle_binding(name, init, false, span, diag);
            }
            Stmt::Mut {
                name, init, span, ..
            } => {
                self.check_expr_usage(init, diag);
                self.handle_binding(name, init, true, span, diag);
            }
            Stmt::Val {
                name,
                init,
                is_mut,
                span,
                ..
            } => {
                self.check_expr_usage(init, diag);
                self.handle_binding(name, init, *is_mut, span, diag);
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                self.check_expr_usage(value, diag);

                if let Expr::Identifier(name, target_span) = target {
                    if !self.local_bindings.contains(name) {
                        self.handle_binding(name, value, false, span, diag);
                    } else {
                        // 1. Check if variable was moved
                        if let Some(ValueState::Moved { at_span, reason }) = self.states.get(name) {
                            diag.error_with_help(
                                ErrorCode::BorrowUseAfterMove,
                                format!(
                                    "Cannot assign to '{}' because it was moved at {} ({})",
                                    name, at_span, reason
                                ),
                                Some(target_span.clone()),
                                Some(format!("consider borrowing with 'view {}' or making a clone before moving", name)),
                            );
                        }

                        // 2. Check if variable has active immutable views
                        if let Some(borrows) = self.active_borrows.get(name)
                            && !borrows.is_empty()
                        {
                            let b = &borrows[0];
                            diag.error_with_help(
                                    ErrorCode::BorrowConflictActiveView,
                                    format!("Cannot mutate or reassign '{}' while active borrow exists (borrowed by '{}' at {})", name, b.borrower, b.span),
                                    Some(span.clone()),
                                    Some(format!("ensure the view '{}' has finished its lifecycle before modifying '{}'", b.borrower, name)),
                                );
                        }

                        // 3. Check mutability flag
                        if self.immutable_bindings.contains(name) {
                            diag.error_with_help(
                                ErrorCode::BorrowCannotMutateImmutable,
                                format!("Cannot mutate immutable binding '{}'", name),
                                Some(span.clone()),
                                Some(format!(
                                    "consider declaring '{}' as mutable: 'mut {} = ...'",
                                    name, name
                                )),
                            );
                        }

                        // 4. The assignment re-initializes the variable: reset its
                        // state so a prior move/borrow violation is reported once
                        // for the current use only, without cascading duplicates.
                        self.states.insert(name.to_string(), ValueState::Active);
                        self.active_borrows.remove(name);
                    }
                } else {
                    self.check_expr_usage(target, diag);
                }
            }
            Stmt::Expr(e, _) | Stmt::Out(e, _) | Stmt::Err(e, _) => {
                self.check_expr_usage(e, diag);
            }
            Stmt::Return(opt_e, span) => {
                if let Some(e) = opt_e {
                    self.check_expr_usage(e, diag);

                    // Check for escaping views: returning a view of a local
                    // variable out of function scope. Covers both the borrowed
                    // variable itself (`return v;`) and a direct view creation
                    // in the return expression (`return x.view();`, which
                    // parses as a call whose callee is a member access).
                    let direct_view_creation = match e {
                        Expr::MemberAccess { object, member, .. } if member == "view" => {
                            matches!(&**object, Expr::Identifier(_, _))
                        }
                        Expr::Call { callee, .. } => matches!(
                            &**callee,
                            Expr::MemberAccess { object, member, .. }
                                if member == "view"
                                    && matches!(&**object, Expr::Identifier(_, _))
                        ),
                        _ => false,
                    };

                    let returned_view_of: Option<&String> = match e {
                        Expr::Identifier(var_name, _) => Some(var_name),
                        Expr::MemberAccess { object, .. } => {
                            if let Expr::Identifier(var_name, _) = &**object {
                                Some(var_name)
                            } else {
                                None
                            }
                        }
                        Expr::Call { callee, .. } => {
                            if let Expr::MemberAccess { object, .. } = &**callee
                                && let Expr::Identifier(var_name, _) = &**object
                            {
                                Some(var_name)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };

                    if let Some(var_name) = returned_view_of {
                        // `return x.view();` on a local variable escapes directly.
                        if direct_view_creation && self.local_bindings.contains(var_name) {
                            diag.error(
                                ErrorCode::BorrowEscapingView,
                                format!("Cannot return view '{}' of local variable out of function scope", var_name),
                                Some(span.clone()),
                            );
                        }

                        // `return v;` where 'v' is a borrower of a local variable.
                        for (source, borrows) in &self.active_borrows {
                            if self.local_bindings.contains(source)
                                && borrows.iter().any(|b| b.borrower == *var_name)
                            {
                                diag.error(
                                        ErrorCode::BorrowEscapingView,
                                        format!("Cannot return view '{}' of local variable '{}' out of function scope", var_name, source),
                                        Some(span.clone()),
                                    );
                            }
                        }
                    }
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_expr_usage(condition, diag);

                // Snapshot the state before branching so each branch is checked
                // against the same starting point: a move in the then-branch must
                // not poison the else-branch (and vice versa).
                let snapshot_states = self.states.clone();
                let snapshot_borrows = self.active_borrows.clone();

                self.check_stmt(then_branch, diag, false);
                let then_states = std::mem::replace(&mut self.states, snapshot_states.clone());
                let _then_borrows =
                    std::mem::replace(&mut self.active_borrows, snapshot_borrows.clone());

                let else_states = if let Some(eb) = else_branch {
                    self.check_stmt(eb, diag, false);
                    Some(std::mem::replace(&mut self.states, snapshot_states.clone()))
                } else {
                    None
                };

                // Conservative join:
                // - With an else branch, a variable is definitely Moved after the
                //   `if` only if it was moved in BOTH branches.
                // - Without an else branch, nothing from the then-branch propagates
                //   as definitely-moved (the branch may not have executed).
                // - If either branch ends with the variable Active (e.g. it was
                //   reassigned there), the joined state is Active. False negatives
                //   are acceptable; false positives are not.
                let mut joined = snapshot_states.clone();
                for (name, snap_state) in snapshot_states.iter() {
                    let joined_state = match &else_states {
                        Some(else_map) => {
                            let t = then_states.get(name).unwrap_or(snap_state);
                            let e = else_map.get(name).unwrap_or(snap_state);
                            match (t, e) {
                                (ValueState::Moved { .. }, ValueState::Moved { .. }) => t.clone(),
                                _ => ValueState::Active,
                            }
                        }
                        None => snap_state.clone(),
                    };
                    joined.insert(name.clone(), joined_state);
                }
                self.states = joined;

                // Borrows created inside a branch die with the branch; restore the
                // pre-branch borrow set so branch-only borrows cannot cause
                // false-positive conflicts after the `if`.
                self.active_borrows = snapshot_borrows;
            }
            Stmt::For { iterable, body, .. } => {
                self.check_expr_usage(iterable, diag);
                self.check_loop_body_twice(None, body, diag);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.check_loop_body_twice(Some(condition), body, diag);
            }
            Stmt::Loop { body, .. } => {
                self.check_stmt(body, diag, false);
            }
            Stmt::TryCatch {
                try_block,
                catch_block,
                ..
            } => {
                self.check_stmt(try_block, diag, false);
                self.check_stmt(catch_block, diag, false);
            }
            Stmt::Parallel(body, _) => {
                self.check_stmt(body, diag, false);
            }
            Stmt::ParallelFor { iterable, body, .. } => {
                self.check_expr_usage(iterable, diag);
                self.check_stmt(body, diag, false);
            }
            Stmt::With {
                resource_name,
                init,
                body,
                span,
            } => {
                self.check_expr_usage(init, diag);
                self.handle_binding(resource_name, init, false, span, diag);
                self.check_stmt(body, diag, false);
            }
            Stmt::Unsafe { body, .. } => {
                self.check_stmt(body, diag, false);
            }
            Stmt::Asm { .. } => {}
        }
    }

    fn handle_binding(
        &mut self,
        name: &str,
        init: &Expr,
        is_mut: bool,
        span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        // Check if variable has active immutable views before re-binding or mutating
        if let Some(borrows) = self.active_borrows.get(name)
            && !borrows.is_empty()
        {
            let b = &borrows[0];
            diag.error_with_help(
                    ErrorCode::BorrowConflictActiveView,
                    format!("Cannot mutate or reassign '{}' while active borrow exists (borrowed by '{}' at {})", name, b.borrower, b.span),
                    Some(span.clone()),
                    Some(format!("ensure view '{}' is no longer in scope before modifying '{}'", b.borrower, name)),
                );
            // Clear the stale borrow records so the rebinding below leaves no
            // dangling borrows and later uses do not produce cascading
            // duplicate errors.
            self.active_borrows.remove(name);
        }

        self.local_bindings.push(name.to_string());
        self.states.insert(name.to_string(), ValueState::Active);
        self.bindings_by_scope
            .entry(self.current_scope)
            .or_default()
            .push(name.to_string());

        if is_mut {
            self.mut_bindings.insert(name.to_string());
        } else {
            self.immutable_bindings.insert(name.to_string());
        }

        // Check if init is an explicit borrow or view of an existing variable
        match init {
            Expr::Call { callee, args, .. } => {
                if let Expr::MemberAccess { object, member, .. } = &**callee {
                    if member == "view"
                        && let Expr::Identifier(source_var, src_span) = &**object
                    {
                        self.register_borrow(source_var, name, false, span, src_span, diag);
                    }
                } else if let Expr::Identifier(fn_name, _) = &**callee {
                    if fn_name == "view" && !args.is_empty() {
                        if let Expr::Identifier(source_var, src_span) = &args[0] {
                            self.register_borrow(source_var, name, false, span, src_span, diag);
                        }
                    } else if (fn_name == "mut_view" || fn_name == "mutView")
                        && !args.is_empty()
                        && let Expr::Identifier(source_var, src_span) = &args[0]
                    {
                        self.register_borrow(source_var, name, true, span, src_span, diag);
                    }
                }
            }
            Expr::MemberAccess { object, member, .. } => {
                if member == "view"
                    && let Expr::Identifier(source_var, src_span) = &**object
                {
                    self.register_borrow(source_var, name, false, span, src_span, diag);
                }
            }
            Expr::Identifier(_source_var, _) => {}
            _ => {}
        }
    }

    fn register_borrow(
        &mut self,
        source_var: &str,
        borrower: &str,
        is_mut: bool,
        span: &SourceSpan,
        src_span: &SourceSpan,
        diag: &mut DiagnosticEngine,
    ) {
        // 1. Check if source was moved
        if let Some(ValueState::Moved { at_span, reason }) = self.states.get(source_var) {
            diag.error_with_help(
                ErrorCode::BorrowUseAfterMove,
                format!(
                    "Cannot borrow '{}' because it was previously moved at {} ({})",
                    source_var, at_span, reason
                ),
                Some(src_span.clone()),
                Some(format!(
                    "create the view before moving '{}', or clone the data",
                    source_var
                )),
            );
            return;
        }

        // 2. Check if mutable borrow conflicts with existing active borrows
        if is_mut {
            if let Some(borrows) = self.active_borrows.get(source_var)
                && !borrows.is_empty()
            {
                let first = &borrows[0];
                diag.error_with_help(
                        ErrorCode::BorrowMultipleMutableViews,
                        format!("Cannot borrow '{}' as mutable because it is already borrowed by '{}' at {}", source_var, first.borrower, first.span),
                        Some(span.clone()),
                        Some(format!("Datara enforces XOR view semantics: only one mutable view of '{}' can exist at a time", source_var)),
                    );
                return;
            }
        } else {
            // Immutable borrow: check if already mutably borrowed
            if let Some(borrows) = self.active_borrows.get(source_var)
                && let Some(m_borrow) = borrows.iter().find(|b| b.is_mut)
            {
                diag.error(
                        ErrorCode::BorrowConflictActiveView,
                        format!("Cannot borrow '{}' as immutable because it is already mutably borrowed by '{}' at {}", source_var, m_borrow.borrower, m_borrow.span),
                        Some(span.clone()),
                    );
                return;
            }
        }

        // Record borrow with current scope depth
        self.active_borrows
            .entry(source_var.to_string())
            .or_default()
            .push(BorrowRecord {
                borrower: borrower.to_string(),
                is_mut,
                span: span.clone(),
                scope_depth: self.current_scope,
            });
    }

    /// Loop-carried approximation for `while`/`for` bodies: the body is
    /// checked twice. The first pass runs sequentially (catching
    /// intra-iteration use-after-move). The second pass re-checks the body
    /// with the post-first-pass states, so a value moved LATE in the body
    /// reads as Moved EARLY in the next iteration — a loop-carried violation
    /// the single sequential pass cannot see.
    ///
    /// Only loop-carried *reads* of moved values ("Use of moved value ...")
    /// are reported from the second pass, deduped against everything the
    /// first pass already reported by (variable, error code). All other
    /// second-pass diagnostics (move-after-move cascades, borrow conflicts
    /// the first pass already caught, ...) are suppressed: conservative here
    /// means no new false positives on programs the first pass accepted.
    ///
    /// The second pass runs on an isolated clone of the tracker with a
    /// scratch diagnostic engine, so the loop-exit states (and therefore the
    /// post-loop "used after the loop" behaviour) remain exactly those of the
    /// first sequential pass.
    fn check_loop_body_twice(
        &mut self,
        condition: Option<&Expr>,
        body: &Stmt,
        diag: &mut DiagnosticEngine,
    ) {
        let base = diag.diagnostics.len();
        if let Some(c) = condition {
            self.check_expr_usage(c, diag);
        }
        self.check_stmt(body, diag, false);

        // (variable, error code) keys of everything the first pass reported.
        let mut reported: HashSet<(String, String)> = HashSet::new();
        for d in &diag.diagnostics[base..] {
            reported.insert((first_quoted(&d.message), d.code.clone()));
        }

        let mut second_pass = self.clone();
        let mut scratch = DiagnosticEngine::new(&diag.locale);
        if let Some(c) = condition {
            second_pass.check_expr_usage(c, &mut scratch);
        }
        second_pass.check_stmt(body, &mut scratch, false);

        for d in &scratch.diagnostics {
            if d.code != ErrorCode::BorrowUseAfterMove.as_str()
                || !d.message.starts_with(LOOP_CARRIED_USE_MARKER)
            {
                continue;
            }
            let key = (first_quoted(&d.message), d.code.clone());
            if !reported.insert(key) {
                continue;
            }
            diag.error_count += 1;
            diag.diagnostics.push(d.clone());
        }
    }

    fn check_expr_usage(&mut self, expr: &Expr, diag: &mut DiagnosticEngine) {
        match expr {
            Expr::Identifier(name, span) => {
                if let Some(ValueState::Moved { at_span, reason }) = self.states.get(name) {
                    diag.error(
                        ErrorCode::BorrowUseAfterMove,
                        format!(
                            "Use of moved value '{}'. Value was moved at {} ({})",
                            name, at_span, reason
                        ),
                        Some(span.clone()),
                    );
                }
            }
            Expr::Call { callee, args, span } => {
                self.check_expr_usage(callee, diag);

                // Check for move-by-value triggers (e.g. `destroy(x)`).
                // When the first argument is a plain identifier it is fully
                // handled here (move-after-move, borrow conflict, or marking
                // the value as moved); the generic per-argument recursion below
                // must skip it, otherwise the argument is reported a second
                // time as a "use of moved value" cascade.
                let mut destroy_arg_handled = false;
                if let Expr::Identifier(fn_name, _) = &**callee
                    && fn_name == "destroy"
                    && !args.is_empty()
                    && let Expr::Identifier(arg_name, arg_span) = &args[0]
                {
                    destroy_arg_handled = true;
                    if let Some(ValueState::Moved { at_span, reason }) = self.states.get(arg_name) {
                        diag.error(
                            ErrorCode::BorrowUseAfterMove,
                            format!(
                                "Cannot move '{}' because it was already moved at {} ({})",
                                arg_name, at_span, reason
                            ),
                            Some(arg_span.clone()),
                        );
                    } else if let Some(borrows) = self.active_borrows.get(arg_name) {
                        if !borrows.is_empty() {
                            let b = &borrows[0];
                            diag.error(
                                        ErrorCode::BorrowConflictActiveView,
                                        format!("Cannot move '{}' because it is actively borrowed by '{}' at {}", arg_name, b.borrower, b.span),
                                        Some(arg_span.clone()),
                                    );
                        } else {
                            self.states.insert(
                                arg_name.clone(),
                                ValueState::Moved {
                                    at_span: span.clone(),
                                    reason: "consumed by 'destroy'".to_string(),
                                },
                            );
                        }
                    } else {
                        self.states.insert(
                            arg_name.clone(),
                            ValueState::Moved {
                                at_span: span.clone(),
                                reason: "consumed by 'destroy'".to_string(),
                            },
                        );
                    }
                }

                // Interprocedural move semantics: passing a class-typed local
                // by value into a user-declared function whose parameter is
                // `owned` consumes it. After the call the argument is Moved,
                // so later uses/borrows are correctly rejected.
                let mut moved_by_call: Vec<usize> = Vec::new();
                if let Expr::Identifier(fn_name, _) = &**callee
                    && let Some(params) = self.fn_signatures.get(fn_name).cloned()
                {
                    for (i, a) in args.iter().enumerate() {
                        let Some(p) = params.get(i) else {
                            break;
                        };
                        let is_owned = p.ownership_mode == "owned" || p.ownership_mode == "own";
                        let is_class = p
                            .type_node
                            .as_ref()
                            .is_some_and(|t| self.class_names.contains(&t.name));
                        if !is_owned || !is_class {
                            continue;
                        }
                        if let Expr::Identifier(arg_name, arg_span) = a {
                            match self.states.get(arg_name) {
                                Some(ValueState::Moved { at_span, reason }) => {
                                    diag.error(
                                        ErrorCode::BorrowUseAfterMove,
                                        format!(
                                            "Cannot move '{}' because it was already moved at {} ({})",
                                            arg_name, at_span, reason
                                        ),
                                        Some(arg_span.clone()),
                                    );
                                    moved_by_call.push(i);
                                }
                                _ => {
                                    let is_active_borrower = self
                                        .active_borrows
                                        .values()
                                        .any(|bs| bs.iter().any(|b| b.borrower == *arg_name));
                                    let is_borrowed = self
                                        .active_borrows
                                        .get(arg_name)
                                        .is_some_and(|bs| !bs.is_empty());
                                    if is_borrowed {
                                        let b = &self.active_borrows[arg_name][0];
                                        diag.error(
                                            ErrorCode::BorrowConflictActiveView,
                                            format!("Cannot move '{}' because it is actively borrowed by '{}' at {}", arg_name, b.borrower, b.span),
                                            Some(arg_span.clone()),
                                        );
                                        moved_by_call.push(i);
                                    } else if !is_active_borrower {
                                        self.states.insert(
                                            arg_name.clone(),
                                            ValueState::Moved {
                                                at_span: span.clone(),
                                                reason: format!(
                                                    "consumed by call to '{}'",
                                                    fn_name
                                                ),
                                            },
                                        );
                                        moved_by_call.push(i);
                                    }
                                }
                            }
                        }
                    }
                }

                // Check for simultaneous alias conflicts across arguments
                let mut call_borrowed_mut: HashSet<String> = HashSet::new();
                let mut call_borrowed_immut: HashSet<String> = HashSet::new();
                for a in args {
                    if let Expr::Call {
                        callee: a_callee,
                        args: a_args,
                        span: a_span,
                    } = a
                        && let Expr::Identifier(a_fn, _) = &**a_callee
                        && let Some(Expr::Identifier(src_name, _)) = a_args.first()
                    {
                        if a_fn == "mut_view" || a_fn == "mutView" {
                            if call_borrowed_mut.contains(src_name)
                                || call_borrowed_immut.contains(src_name)
                            {
                                diag.error(
                                            ErrorCode::BorrowMultipleMutableViews,
                                            format!("Illegal simultaneous mutable alias of '{}' in function arguments", src_name),
                                            Some(a_span.clone()),
                                        );
                            }
                            call_borrowed_mut.insert(src_name.clone());
                        } else if a_fn == "view" {
                            if call_borrowed_mut.contains(src_name) {
                                diag.error(
                                            ErrorCode::BorrowConflictActiveView,
                                            format!("Illegal simultaneous mutable and immutable alias of '{}' in function arguments", src_name),
                                            Some(a_span.clone()),
                                        );
                            }
                            call_borrowed_immut.insert(src_name.clone());
                        }
                    }
                }

                for (i, a) in args.iter().enumerate() {
                    // Arguments fully handled above (destroy trigger or
                    // by-value class moves) must not be re-reported by the
                    // generic recursion — that would duplicate the diagnostic.
                    if (destroy_arg_handled && i == 0) || moved_by_call.contains(&i) {
                        continue;
                    }
                    self.check_expr_usage(a, diag);
                }
            }
            Expr::MemberAccess { object, .. } => {
                self.check_expr_usage(object, diag);
            }
            Expr::Binary { left, right, .. } => {
                self.check_expr_usage(left, diag);
                self.check_expr_usage(right, diag);
            }
            Expr::Unary { expr, .. } | Expr::ErrorPropagate(expr, _) => {
                self.check_expr_usage(expr, diag);
            }
            Expr::ObjectInit { fields, .. } => {
                for (_, f_expr) in fields {
                    self.check_expr_usage(f_expr, diag);
                }
            }
            Expr::Pipeline { stages, .. } => {
                for s in stages {
                    self.check_expr_usage(s, diag);
                }
            }
            Expr::Decide { arms, else_arm, .. } => {
                for a in arms {
                    self.check_expr_usage(&a.condition, diag);
                    self.check_expr_usage(&a.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.check_expr_usage(eb, diag);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.check_expr_usage(value, diag);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.check_expr_usage(g, diag);
                    }
                    self.check_expr_usage(&a.body, diag);
                }
            }
            Expr::Select { arms, else_arm, .. } => {
                for a in arms {
                    self.check_expr_usage(&a.condition, diag);
                    self.check_expr_usage(&a.body, diag);
                }
                if let Some(eb) = else_arm {
                    self.check_expr_usage(eb, diag);
                }
            }
            Expr::Lambda { body, .. } => {
                self.check_expr_usage(body, diag);
            }
            Expr::ListLiteral(items, _) => {
                for item in items {
                    self.check_expr_usage(item, diag);
                }
            }
            Expr::MapLiteral(entries, _) => {
                for (k, v) in entries {
                    self.check_expr_usage(k, diag);
                    self.check_expr_usage(v, diag);
                }
            }
            Expr::IndexAccess { object, index, .. } => {
                self.check_expr_usage(object, diag);
                self.check_expr_usage(index, diag);
            }
            Expr::Range { start, end, .. } => {
                self.check_expr_usage(start, diag);
                self.check_expr_usage(end, diag);
            }
            Expr::Tuple(exprs, _) => {
                for e in exprs {
                    self.check_expr_usage(e, diag);
                }
            }
            Expr::InterpolatedString { expressions, .. } => {
                for e in expressions {
                    self.check_expr_usage(e, diag);
                }
            }
            _ => {}
        }
    }
}
