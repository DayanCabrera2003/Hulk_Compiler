use std::collections::HashMap;

use hulk_diagnostics::{Diagnostic, DiagnosticBag};
use hulk_hir::Span;
use hulk_hir::{
    AssignTarget, Expr, ExprKind, Hir, MacroDecl, MacroParam, MemberKind, NodeIdGen, Param,
    Resolver, SymbolId, SymbolKind, TypeAnn, TypeEnv, TypeId,
};

/// Expands macro invocations in a HIR program.
///
/// This pass applies three steps for each macro call:
/// - local-variable sanitization to prevent accidental capture;
/// - parameter substitution (`regular`, `*body`, `@symbol`, `$placeholder`);
/// - recursive expansion of nested macro calls introduced by substitution.
#[must_use]
pub fn expand_macros(hir: Hir, bag: &mut DiagnosticBag) -> Hir {
    let Hir {
        mut program,
        mut symbols,
        mut types,
    } = hir;

    let start_id = max_node_id_in_program(&program).saturating_add(1);
    let mut expander = MacroExpander::new(
        collect_macros(&program.macros),
        bag,
        &mut symbols,
        &mut types,
        NodeIdGen::with_start(start_id),
    );

    for function in &mut program.functions {
        expander.expand_expr(&mut function.body);
    }

    for type_decl in &mut program.types {
        for member in &mut type_decl.members {
            match &mut member.kind {
                MemberKind::Attribute { value, .. } => expander.expand_expr(value),
                MemberKind::Method(method) => expander.expand_expr(&mut method.body),
            }
        }
    }

    for macro_decl in &mut program.macros {
        expander.expand_expr(&mut macro_decl.body);
    }

    expander.expand_expr(&mut program.body);

    Hir {
        program,
        symbols,
        types,
    }
}

fn collect_macros(macros: &[MacroDecl]) -> HashMap<String, MacroDecl> {
    let mut by_name = HashMap::with_capacity(macros.len());
    for macro_decl in macros {
        by_name.insert(macro_decl.name.clone(), macro_decl.clone());
    }
    by_name
}

struct MacroExpander<'a> {
    macros: HashMap<String, MacroDecl>,
    bag: &'a mut DiagnosticBag,
    symbols: &'a mut Resolver,
    types: &'a mut TypeEnv,
    node_ids: NodeIdGen,
    expansion_counter: u64,
}

impl<'a> MacroExpander<'a> {
    fn new(
        macros: HashMap<String, MacroDecl>,
        bag: &'a mut DiagnosticBag,
        symbols: &'a mut Resolver,
        types: &'a mut TypeEnv,
        node_ids: NodeIdGen,
    ) -> Self {
        Self {
            macros,
            bag,
            symbols,
            types,
            node_ids,
            expansion_counter: 0,
        }
    }

    fn expand_expr(&mut self, expr: &mut Expr) {
        self.expand_expr_children(expr);

        let macro_call = match &expr.kind {
            ExprKind::Call { callee, args } => {
                if let ExprKind::Ident(name) = &callee.kind {
                    Some((name.clone(), args.clone()))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((macro_name, args)) = macro_call {
            let call_span = expr.span.clone();
            let fallback = expr.clone();
            let expanded = self.expand_macro_call(&macro_name, &args, call_span, fallback);
            *expr = expanded;
        }
    }

    fn expand_expr_children(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Ident(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                self.expand_expr(left);
                self.expand_expr(right);
            }
            ExprKind::UnaryOp { expr, .. } => self.expand_expr(expr),
            ExprKind::Call { callee, args } => {
                self.expand_expr(callee);
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.expand_expr(receiver);
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => self.expand_expr(receiver),
            ExprKind::Index { target, index } => {
                self.expand_expr(target);
                self.expand_expr(index);
            }
            ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
                for item in exprs {
                    self.expand_expr(item);
                }
            }
            ExprKind::VecGenerator {
                element, iterable, ..
            } => {
                self.expand_expr(element);
                self.expand_expr(iterable);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    self.expand_expr(binding);
                }
                self.expand_expr(body);
            }
            ExprKind::Assign { target, value } => {
                self.expand_expr(target);
                self.expand_expr(value);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(_) => {}
                AssignTarget::Field { receiver, .. } => self.expand_expr(receiver),
                AssignTarget::Index { target, index } => {
                    self.expand_expr(target);
                    self.expand_expr(index);
                }
            },
            ExprKind::LetBinding(binding) => self.expand_expr(&mut binding.value),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.expand_expr(condition);
                self.expand_expr(then_branch);
                for (elif_cond, elif_body) in elif_branches {
                    self.expand_expr(elif_cond);
                    self.expand_expr(elif_body);
                }
                if let Some(else_expr) = else_branch {
                    self.expand_expr(else_expr);
                }
            }
            ExprKind::While { condition, body } => {
                self.expand_expr(condition);
                self.expand_expr(body);
            }
            ExprKind::For { iterable, body, .. } => {
                self.expand_expr(iterable);
                self.expand_expr(body);
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    self.expand_expr(arg);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => self.expand_expr(expr),
            ExprKind::Lambda { body, .. } => self.expand_expr(body),
        }
    }

    fn expand_macro_call(
        &mut self,
        macro_name: &str,
        args: &[Expr],
        call_span: Span,
        fallback: Expr,
    ) -> Expr {
        let Some(macro_decl) = self.macros.get(macro_name).cloned() else {
            return fallback;
        };

        if args.len() != macro_decl.params.len() {
            self.bag.push(
                Diagnostic::error(format!(
                    "cantidad de argumentos invalida para macro '{macro_name}'"
                ))
                .with_label(
                    call_span.clone(),
                    format!(
                        "se esperaban {} argumentos y se recibieron {}",
                        macro_decl.params.len(),
                        args.len()
                    ),
                ),
            );
            return fallback;
        }

        let mut substitutions = HashMap::new();
        for (param, arg) in macro_decl.params.iter().zip(args) {
            match self.build_substitution(param, arg) {
                Some(substitution) => {
                    substitutions.insert(param.name().to_owned(), substitution);
                }
                None => {
                    return fallback;
                }
            }
        }

        let expansion_id = self.expansion_counter;
        self.expansion_counter = self.expansion_counter.saturating_add(1);

        let mut expanded = macro_decl.body;
        sanitize_locals(&mut expanded, macro_name, expansion_id);
        substitute_params(&mut expanded, &substitutions, self.bag);
        self.expand_expr(&mut expanded);
        refresh_node_ids(&mut expanded, &mut self.node_ids);
        expanded.span = call_span;
        expanded
    }

    fn build_substitution(&mut self, param: &MacroParam, arg: &Expr) -> Option<Substitution> {
        match param {
            MacroParam::Regular { .. } | MacroParam::Body { .. } => {
                Some(Substitution::Expr(arg.clone()))
            }
            MacroParam::Symbolic { name, .. } => {
                if let ExprKind::Ident(symbol_name) = &arg.kind {
                    Some(Substitution::Symbol(symbol_name.clone()))
                } else {
                    self.bag.push(
                        Diagnostic::error(format!(
                            "el parametro simbolico '{name}' requiere un identificador"
                        ))
                        .with_label(arg.span.clone(), "se esperaba un identificador"),
                    );
                    None
                }
            }
            MacroParam::Placeholder { name, type_ann, .. } => {
                if let ExprKind::Ident(symbol_name) = &arg.kind {
                    let symbol_id = self.allocate_placeholder_symbol(symbol_name, arg.span.clone());
                    let inferred_type = map_type_ann_to_type_id(type_ann);
                    self.types.register_symbol_type(symbol_id, inferred_type);

                    Some(Substitution::Placeholder {
                        ident: symbol_name.clone(),
                        symbol: symbol_id,
                    })
                } else {
                    self.bag.push(
                        Diagnostic::error(format!(
                            "el placeholder '{name}' requiere un identificador"
                        ))
                        .with_label(arg.span.clone(), "se esperaba un identificador"),
                    );
                    None
                }
            }
        }
    }

    fn allocate_placeholder_symbol(&mut self, name: &str, span: Span) -> SymbolId {
        self.symbols.push_scope();
        let symbol_id = self
            .symbols
            .define(name.to_owned(), SymbolKind::Variable, span);
        let _ = self.symbols.pop_scope();
        symbol_id
    }
}

#[derive(Clone)]
enum Substitution {
    Expr(Expr),
    Symbol(String),
    Placeholder { ident: String, symbol: SymbolId },
}

fn map_type_ann_to_type_id(type_ann: &TypeAnn) -> TypeId {
    match type_ann {
        TypeAnn::Named(name) => match name.as_str() {
            "Number" => TypeId::NUMBER,
            "String" => TypeId::STRING,
            "Boolean" => TypeId::BOOLEAN,
            _ => TypeId::OBJECT,
        },
        TypeAnn::Iterable(_) | TypeAnn::Vector(_) | TypeAnn::Functor { .. } => TypeId::OBJECT,
    }
}

fn sanitize_locals(expr: &mut Expr, macro_name: &str, expansion_id: u64) {
    let mut sanitizer = LocalSanitizer {
        macro_name,
        expansion_id,
        scopes: Vec::new(),
    };
    sanitizer.visit_expr(expr);
}

struct LocalSanitizer<'a> {
    macro_name: &'a str,
    expansion_id: u64,
    scopes: Vec<HashMap<String, String>>,
}

impl<'a> LocalSanitizer<'a> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Ident(name) => {
                if let Some(renamed) = self.lookup(name) {
                    *name = renamed;
                }
            }
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::UnaryOp { expr, .. } => self.visit_expr(expr),
            ExprKind::Call { callee, args } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.visit_expr(receiver);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => self.visit_expr(receiver),
            ExprKind::Index { target, index } => {
                self.visit_expr(target);
                self.visit_expr(index);
            }
            ExprKind::Block(exprs) => {
                self.scopes.push(HashMap::new());
                for item in exprs {
                    self.visit_expr(item);
                }
                let _ = self.scopes.pop();
            }
            ExprKind::VecLiteral(items) => {
                for item in items {
                    self.visit_expr(item);
                }
            }
            ExprKind::VecGenerator {
                element,
                binding,
                iterable,
            } => {
                self.visit_expr(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr(element);
                let _ = self.scopes.pop();
            }
            ExprKind::Let { bindings, body } => {
                let mut pushed = 0usize;
                for binding_expr in bindings {
                    if let ExprKind::LetBinding(binding) = &mut binding_expr.kind {
                        self.visit_expr(&mut binding.value);
                        let fresh = self.fresh_name(&binding.name);
                        self.scopes
                            .push(HashMap::from([(binding.name.clone(), fresh.clone())]));
                        binding.name = fresh;
                        pushed = pushed.saturating_add(1);
                    } else {
                        self.visit_expr(binding_expr);
                    }
                }

                self.visit_expr(body);
                for _ in 0..pushed {
                    let _ = self.scopes.pop();
                }
            }
            ExprKind::Assign { target, value } => {
                self.visit_expr(target);
                self.visit_expr(value);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(name) => {
                    if let Some(renamed) = self.lookup(name) {
                        *name = renamed;
                    }
                }
                AssignTarget::Field { receiver, .. } => self.visit_expr(receiver),
                AssignTarget::Index { target, index } => {
                    self.visit_expr(target);
                    self.visit_expr(index);
                }
            },
            ExprKind::LetBinding(binding) => self.visit_expr(&mut binding.value),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                self.visit_expr(condition);
                self.visit_expr(then_branch);
                for (elif_cond, elif_body) in elif_branches {
                    self.visit_expr(elif_cond);
                    self.visit_expr(elif_body);
                }
                if let Some(else_expr) = else_branch {
                    self.visit_expr(else_expr);
                }
            }
            ExprKind::While { condition, body } => {
                self.visit_expr(condition);
                self.visit_expr(body);
            }
            ExprKind::For {
                binding,
                iterable,
                body,
            } => {
                self.visit_expr(iterable);
                let fresh = self.fresh_name(binding);
                self.scopes
                    .push(HashMap::from([(binding.clone(), fresh.clone())]));
                *binding = fresh;
                self.visit_expr(body);
                let _ = self.scopes.pop();
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => self.visit_expr(expr),
            ExprKind::Lambda { params, body, .. } => {
                let mut frame = HashMap::new();
                for Param { name, .. } in params {
                    let fresh = self.fresh_name(name);
                    frame.insert(name.clone(), fresh.clone());
                    *name = fresh;
                }
                self.scopes.push(frame);
                self.visit_expr(body);
                let _ = self.scopes.pop();
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn fresh_name(&self, original: &str) -> String {
        format!(
            "__hulk_macro_{}_{}_{}",
            self.macro_name, self.expansion_id, original
        )
    }
}

fn substitute_params(
    expr: &mut Expr,
    substitutions: &HashMap<String, Substitution>,
    bag: &mut DiagnosticBag,
) {
    match &mut expr.kind {
        ExprKind::Ident(name) => {
            if let Some(sub) = substitutions.get(name).cloned() {
                match sub {
                    Substitution::Expr(value) => {
                        *expr = value;
                    }
                    Substitution::Symbol(symbol_name) => {
                        *name = symbol_name;
                    }
                    Substitution::Placeholder { ident, symbol } => {
                        let _ = symbol;
                        *name = ident;
                    }
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            substitute_params(left, substitutions, bag);
            substitute_params(right, substitutions, bag);
        }
        ExprKind::UnaryOp { expr, .. } => substitute_params(expr, substitutions, bag),
        ExprKind::Call { callee, args } => {
            substitute_params(callee, substitutions, bag);
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            substitute_params(receiver, substitutions, bag);
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => substitute_params(receiver, substitutions, bag),
        ExprKind::Index { target, index } => {
            substitute_params(target, substitutions, bag);
            substitute_params(index, substitutions, bag);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                substitute_params(item, substitutions, bag);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            substitute_params(element, substitutions, bag);
            substitute_params(iterable, substitutions, bag);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                substitute_params(binding, substitutions, bag);
            }
            substitute_params(body, substitutions, bag);
        }
        ExprKind::Assign { target, value } => {
            substitute_params(target, substitutions, bag);
            substitute_params(value, substitutions, bag);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(name) => {
                if let Some(sub) = substitutions.get(name) {
                    match sub {
                        Substitution::Expr(_) => {
                            bag.push(
                                Diagnostic::error(
                                    "un parametro de expresion no puede ser destino de asignacion",
                                )
                                .with_label(expr.span.clone(), "destino de asignacion invalido"),
                            );
                        }
                        Substitution::Symbol(symbol_name) => {
                            *name = symbol_name.clone();
                        }
                        Substitution::Placeholder { ident, symbol } => {
                            let _ = symbol;
                            *name = ident.clone();
                        }
                    }
                }
            }
            AssignTarget::Field { receiver, .. } => substitute_params(receiver, substitutions, bag),
            AssignTarget::Index { target, index } => {
                substitute_params(target, substitutions, bag);
                substitute_params(index, substitutions, bag);
            }
        },
        ExprKind::LetBinding(binding) => substitute_params(&mut binding.value, substitutions, bag),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            substitute_params(condition, substitutions, bag);
            substitute_params(then_branch, substitutions, bag);
            for (elif_cond, elif_body) in elif_branches {
                substitute_params(elif_cond, substitutions, bag);
                substitute_params(elif_body, substitutions, bag);
            }
            if let Some(else_expr) = else_branch {
                substitute_params(else_expr, substitutions, bag);
            }
        }
        ExprKind::While { condition, body } => {
            substitute_params(condition, substitutions, bag);
            substitute_params(body, substitutions, bag);
        }
        ExprKind::For { iterable, body, .. } => {
            substitute_params(iterable, substitutions, bag);
            substitute_params(body, substitutions, bag);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                substitute_params(arg, substitutions, bag);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
            substitute_params(expr, substitutions, bag);
        }
        ExprKind::Lambda { body, .. } => substitute_params(body, substitutions, bag),
    }
}

fn refresh_node_ids(expr: &mut Expr, node_ids: &mut NodeIdGen) {
    expr.id = node_ids.next_id();

    match &mut expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            refresh_node_ids(left, node_ids);
            refresh_node_ids(right, node_ids);
        }
        ExprKind::UnaryOp { expr, .. } => refresh_node_ids(expr, node_ids),
        ExprKind::Call { callee, args } => {
            refresh_node_ids(callee, node_ids);
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            refresh_node_ids(receiver, node_ids);
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => refresh_node_ids(receiver, node_ids),
        ExprKind::Index { target, index } => {
            refresh_node_ids(target, node_ids);
            refresh_node_ids(index, node_ids);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                refresh_node_ids(item, node_ids);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            refresh_node_ids(element, node_ids);
            refresh_node_ids(iterable, node_ids);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                refresh_node_ids(binding, node_ids);
            }
            refresh_node_ids(body, node_ids);
        }
        ExprKind::Assign { target, value } => {
            refresh_node_ids(target, node_ids);
            refresh_node_ids(value, node_ids);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => refresh_node_ids(receiver, node_ids),
            AssignTarget::Index { target, index } => {
                refresh_node_ids(target, node_ids);
                refresh_node_ids(index, node_ids);
            }
        },
        ExprKind::LetBinding(binding) => refresh_node_ids(&mut binding.value, node_ids),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            refresh_node_ids(condition, node_ids);
            refresh_node_ids(then_branch, node_ids);
            for (elif_cond, elif_body) in elif_branches {
                refresh_node_ids(elif_cond, node_ids);
                refresh_node_ids(elif_body, node_ids);
            }
            if let Some(else_expr) = else_branch {
                refresh_node_ids(else_expr, node_ids);
            }
        }
        ExprKind::While { condition, body } => {
            refresh_node_ids(condition, node_ids);
            refresh_node_ids(body, node_ids);
        }
        ExprKind::For { iterable, body, .. } => {
            refresh_node_ids(iterable, node_ids);
            refresh_node_ids(body, node_ids);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                refresh_node_ids(arg, node_ids);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => refresh_node_ids(expr, node_ids),
        ExprKind::Lambda { body, .. } => refresh_node_ids(body, node_ids),
    }
}

fn max_node_id_in_program(program: &hulk_hir::Program) -> u32 {
    let mut max_id = 0_u32;

    for function in &program.functions {
        visit_max_node_id(&function.body, &mut max_id);
    }
    for type_decl in &program.types {
        for member in &type_decl.members {
            match &member.kind {
                MemberKind::Attribute { value, .. } => visit_max_node_id(value, &mut max_id),
                MemberKind::Method(method) => visit_max_node_id(&method.body, &mut max_id),
            }
        }
    }
    for macro_decl in &program.macros {
        visit_max_node_id(&macro_decl.body, &mut max_id);
    }
    visit_max_node_id(&program.body, &mut max_id);

    max_id
}

fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    *max_id = (*max_id).max(expr.id.0);

    match &expr.kind {
        ExprKind::Number(_)
        | ExprKind::StringLit(_)
        | ExprKind::Bool(_)
        | ExprKind::Ident(_)
        | ExprKind::Self_
        | ExprKind::Base => {}
        ExprKind::BinOp { left, right, .. } => {
            visit_max_node_id(left, max_id);
            visit_max_node_id(right, max_id);
        }
        ExprKind::UnaryOp { expr, .. } => visit_max_node_id(expr, max_id),
        ExprKind::Call { callee, args } => {
            visit_max_node_id(callee, max_id);
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::MethodCall { receiver, args, .. } => {
            visit_max_node_id(receiver, max_id);
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::FieldAccess { receiver, .. } => visit_max_node_id(receiver, max_id),
        ExprKind::Index { target, index } => {
            visit_max_node_id(target, max_id);
            visit_max_node_id(index, max_id);
        }
        ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
            for item in exprs {
                visit_max_node_id(item, max_id);
            }
        }
        ExprKind::VecGenerator {
            element, iterable, ..
        } => {
            visit_max_node_id(element, max_id);
            visit_max_node_id(iterable, max_id);
        }
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                visit_max_node_id(binding, max_id);
            }
            visit_max_node_id(body, max_id);
        }
        ExprKind::Assign { target, value } => {
            visit_max_node_id(target, max_id);
            visit_max_node_id(value, max_id);
        }
        ExprKind::AssignTarget(target) => match target {
            AssignTarget::Ident(_) => {}
            AssignTarget::Field { receiver, .. } => visit_max_node_id(receiver, max_id),
            AssignTarget::Index { target, index } => {
                visit_max_node_id(target, max_id);
                visit_max_node_id(index, max_id);
            }
        },
        ExprKind::LetBinding(binding) => visit_max_node_id(&binding.value, max_id),
        ExprKind::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            visit_max_node_id(condition, max_id);
            visit_max_node_id(then_branch, max_id);
            for (elif_cond, elif_body) in elif_branches {
                visit_max_node_id(elif_cond, max_id);
                visit_max_node_id(elif_body, max_id);
            }
            if let Some(else_expr) = else_branch {
                visit_max_node_id(else_expr, max_id);
            }
        }
        ExprKind::While { condition, body } => {
            visit_max_node_id(condition, max_id);
            visit_max_node_id(body, max_id);
        }
        ExprKind::For { iterable, body, .. } => {
            visit_max_node_id(iterable, max_id);
            visit_max_node_id(body, max_id);
        }
        ExprKind::New { args, .. } => {
            for arg in args {
                visit_max_node_id(arg, max_id);
            }
        }
        ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => visit_max_node_id(expr, max_id),
        ExprKind::Lambda { body, .. } => visit_max_node_id(body, max_id),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hulk_hir::{
        BinOpKind, LetBinding, Program, SourceFile, Span, TypeAnn, TypedAst, UnaryOpKind,
    };

    use super::*;

    #[test]
    fn repeat_macro_is_expanded_with_sanitized_locals() {
        let source = Arc::new(SourceFile::new("macros.hulk", "def repeat ..."));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 12);

        let macro_decl = MacroDecl {
            name: "repeat".to_owned(),
            params: vec![
                MacroParam::Regular {
                    name: "n".to_owned(),
                    type_ann: TypeAnn::Named("Number".to_owned()),
                    span: span.clone(),
                },
                MacroParam::Body {
                    name: "expr".to_owned(),
                    type_ann: TypeAnn::Named("Object".to_owned()),
                    span: span.clone(),
                },
            ],
            body: Expr::new(
                ExprKind::Let {
                    bindings: vec![Expr::new(
                        ExprKind::LetBinding(LetBinding {
                            name: "total".to_owned(),
                            type_ann: None,
                            value: Box::new(Expr::new(
                                ExprKind::Ident("n".to_owned()),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            span: span.clone(),
                        }),
                        span.clone(),
                        node_ids.next_id(),
                    )],
                    body: Box::new(Expr::new(
                        ExprKind::While {
                            condition: Box::new(Expr::new(
                                ExprKind::BinOp {
                                    op: BinOpKind::Ge,
                                    left: Box::new(Expr::new(
                                        ExprKind::Ident("total".to_owned()),
                                        span.clone(),
                                        node_ids.next_id(),
                                    )),
                                    right: Box::new(Expr::new(
                                        ExprKind::Number(0.0),
                                        span.clone(),
                                        node_ids.next_id(),
                                    )),
                                },
                                span.clone(),
                                node_ids.next_id(),
                            )),
                            body: Box::new(Expr::new(
                                ExprKind::Block(vec![
                                    Expr::new(
                                        ExprKind::Assign {
                                            target: Box::new(Expr::new(
                                                ExprKind::AssignTarget(AssignTarget::Ident(
                                                    "total".to_owned(),
                                                )),
                                                span.clone(),
                                                node_ids.next_id(),
                                            )),
                                            value: Box::new(Expr::new(
                                                ExprKind::BinOp {
                                                    op: BinOpKind::Sub,
                                                    left: Box::new(Expr::new(
                                                        ExprKind::Ident("total".to_owned()),
                                                        span.clone(),
                                                        node_ids.next_id(),
                                                    )),
                                                    right: Box::new(Expr::new(
                                                        ExprKind::Number(1.0),
                                                        span.clone(),
                                                        node_ids.next_id(),
                                                    )),
                                                },
                                                span.clone(),
                                                node_ids.next_id(),
                                            )),
                                        },
                                        span.clone(),
                                        node_ids.next_id(),
                                    ),
                                    Expr::new(
                                        ExprKind::Ident("expr".to_owned()),
                                        span.clone(),
                                        node_ids.next_id(),
                                    ),
                                ]),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )),
                },
                span.clone(),
                node_ids.next_id(),
            ),
            span: span.clone(),
        };

        let call = Expr::new(
            ExprKind::Call {
                callee: Box::new(Expr::new(
                    ExprKind::Ident("repeat".to_owned()),
                    span.clone(),
                    node_ids.next_id(),
                )),
                args: vec![
                    Expr::new(ExprKind::Number(10.0), span.clone(), node_ids.next_id()),
                    Expr::new(
                        ExprKind::Block(vec![Expr::new(
                            ExprKind::Call {
                                callee: Box::new(Expr::new(
                                    ExprKind::Ident("print".to_owned()),
                                    span.clone(),
                                    node_ids.next_id(),
                                )),
                                args: vec![Expr::new(
                                    ExprKind::StringLit("hello".to_owned()),
                                    span.clone(),
                                    node_ids.next_id(),
                                )],
                            },
                            span.clone(),
                            node_ids.next_id(),
                        )]),
                        span.clone(),
                        node_ids.next_id(),
                    ),
                ],
            },
            span.clone(),
            node_ids.next_id(),
        );

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![macro_decl],
            body: call,
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);

        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);
        assert!(!bag.has_errors());

        let mut idents = Vec::new();
        collect_identifiers(&expanded.program.body, &mut idents);

        assert!(!idents.iter().any(|name| name == "repeat"));
        assert!(!idents.iter().any(|name| name == "expr"));
        assert!(idents
            .iter()
            .any(|name| name.starts_with("__hulk_macro_repeat_0_total")));
        assert!(idents.iter().any(|name| name == "print"));
    }

    #[test]
    fn non_macro_call_is_not_modified() {
        let source = Arc::new(SourceFile::new("macros.hulk", "print(1);"));
        let mut node_ids = NodeIdGen::new();
        let span = Span::new(source, 0, 9);

        let program = Program {
            functions: vec![],
            types: vec![],
            protocols: vec![],
            macros: vec![],
            body: Expr::new(
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        ExprKind::Ident("print".to_owned()),
                        span.clone(),
                        node_ids.next_id(),
                    )),
                    args: vec![Expr::new(
                        ExprKind::UnaryOp {
                            op: UnaryOpKind::Neg,
                            expr: Box::new(Expr::new(
                                ExprKind::Number(-1.0),
                                span.clone(),
                                node_ids.next_id(),
                            )),
                        },
                        span.clone(),
                        node_ids.next_id(),
                    )],
                },
                span.clone(),
                node_ids.next_id(),
            ),
        };

        let mut symbols = Resolver::new();
        symbols.resolve_program(&program);
        let hir = Hir::from_typed(TypedAst {
            program,
            symbols,
            types: TypeEnv::new(),
        });

        let mut bag = DiagnosticBag::new();
        let expanded = expand_macros(hir, &mut bag);

        match expanded.program.body.kind {
            ExprKind::Call { .. } => {}
            _ => panic!("expected call expression to remain unchanged"),
        }
    }

    fn collect_identifiers(expr: &Expr, out: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Ident(name) => out.push(name.clone()),
            ExprKind::Number(_)
            | ExprKind::StringLit(_)
            | ExprKind::Bool(_)
            | ExprKind::Self_
            | ExprKind::Base => {}
            ExprKind::BinOp { left, right, .. } => {
                collect_identifiers(left, out);
                collect_identifiers(right, out);
            }
            ExprKind::UnaryOp { expr, .. } => collect_identifiers(expr, out),
            ExprKind::Call { callee, args } => {
                collect_identifiers(callee, out);
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                collect_identifiers(receiver, out);
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::FieldAccess { receiver, .. } => collect_identifiers(receiver, out),
            ExprKind::Index { target, index } => {
                collect_identifiers(target, out);
                collect_identifiers(index, out);
            }
            ExprKind::Block(exprs) | ExprKind::VecLiteral(exprs) => {
                for item in exprs {
                    collect_identifiers(item, out);
                }
            }
            ExprKind::VecGenerator {
                element, iterable, ..
            } => {
                collect_identifiers(element, out);
                collect_identifiers(iterable, out);
            }
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    collect_identifiers(binding, out);
                }
                collect_identifiers(body, out);
            }
            ExprKind::Assign { target, value } => {
                collect_identifiers(target, out);
                collect_identifiers(value, out);
            }
            ExprKind::AssignTarget(target) => match target {
                AssignTarget::Ident(name) => out.push(name.clone()),
                AssignTarget::Field { receiver, .. } => collect_identifiers(receiver, out),
                AssignTarget::Index { target, index } => {
                    collect_identifiers(target, out);
                    collect_identifiers(index, out);
                }
            },
            ExprKind::LetBinding(binding) => collect_identifiers(&binding.value, out),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                collect_identifiers(condition, out);
                collect_identifiers(then_branch, out);
                for (elif_cond, elif_body) in elif_branches {
                    collect_identifiers(elif_cond, out);
                    collect_identifiers(elif_body, out);
                }
                if let Some(else_expr) = else_branch {
                    collect_identifiers(else_expr, out);
                }
            }
            ExprKind::While { condition, body } => {
                collect_identifiers(condition, out);
                collect_identifiers(body, out);
            }
            ExprKind::For { iterable, body, .. } => {
                collect_identifiers(iterable, out);
                collect_identifiers(body, out);
            }
            ExprKind::New { args, .. } => {
                for arg in args {
                    collect_identifiers(arg, out);
                }
            }
            ExprKind::Is { expr, .. } | ExprKind::As { expr, .. } => {
                collect_identifiers(expr, out);
            }
            ExprKind::Lambda { body, .. } => collect_identifiers(body, out),
        }
    }
}
