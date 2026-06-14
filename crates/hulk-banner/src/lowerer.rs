use std::collections::HashMap;

use hulk_hir::{
    AssignTarget, BinOpKind, Expr, ExprKind, FunctionDecl, Hir, LetBinding, MemberKind, Symbol,
    SymbolId, SymbolKind, TypeAnn, TypeId, UnaryOpKind,
};

use crate::ir::{BannerFunction, BannerProgram, Instr, TempId, TypeDescriptor, Value};

// Produces a linear three-address BANNER program from a desugared HIR.
//
// Architectural note: the lowerer holds a borrow of the HIR while emitting
// instructions through `&mut self` methods. To avoid clashing borrows in
// `lower_program`, all HIR-derived data that is iterated under `&mut self`
// is pre-extracted into owned structures first. After that, every
// `emit_expr` call only touches `self`.
pub(crate) struct Lowerer<'h> {
    hir: &'h Hir,
    instrs: Vec<Instr>,
    next_temp: u32,
    next_label: u32,
    // Maps SymbolId of a `let` binding to the temporary that holds its value.
    locals: HashMap<SymbolId, TempId>,
    // Fallback map for desugared `let` bindings that the resolver never saw
    // (e.g., synthetic variables introduced by the `for`-loop desugar pass).
    // Keyed by binding name; shadowing is handled by always overwriting on bind
    // and restoring on scope exit via `locals_by_name_stack`.
    locals_by_name: HashMap<String, TempId>,
    // Maps a parameter name to the temporary assigned in the current frame.
    // Param names are used (instead of SymbolId) because Param nodes have no
    // NodeId and cannot be resolved through `hir.resolved_symbol`.
    param_temps: HashMap<String, TempId>,
    // Number of `ShadowPush` emissions made inside the current `let` frame.
    // Saved/restored around each `Let` so nested lets pop only their own.
    shadow_count: usize,
    self_temp: Option<TempId>,
    // Reserved for future passes that need the enclosing type name (e.g., reflection, vtable generation).
    current_type_name: Option<String>,
    current_parent_type_name: Option<String>,
    current_method_name: Option<String>,
}

impl<'h> Lowerer<'h> {
    pub(crate) fn new(hir: &'h Hir) -> Self {
        Self {
            hir,
            instrs: Vec::new(),
            next_temp: 0,
            next_label: 0,
            locals: HashMap::new(),
            locals_by_name: HashMap::new(),
            param_temps: HashMap::new(),
            shadow_count: 0,
            self_temp: None,
            current_type_name: None,
            current_parent_type_name: None,
            current_method_name: None,
        }
    }

    pub(crate) fn lower_program(mut self) -> BannerProgram {
        // Decouple the borrow on `self.hir` from the upcoming `&mut self`
        // calls so iteration over HIR collections does not conflict with
        // emission methods.
        let hir = self.hir;

        // Pre-extract every type's structure into an owned form. No
        // `emit_expr` calls happen during this iteration.
        struct TypeEntry {
            name: String,
            parent: Option<String>,
            parent_args: Vec<Expr>,
            attrs: Vec<(String, Option<TypeAnn>, Expr)>,
            type_params: Vec<hulk_hir::Param>,
            methods: Vec<FunctionDecl>,
        }

        let type_entries: Vec<TypeEntry> = hir
            .program
            .types
            .iter()
            .map(|td| TypeEntry {
                name: td.name.clone(),
                parent: td.parent.as_ref().map(|p| p.name.clone()),
                parent_args: td
                    .parent
                    .as_ref()
                    .map(|p| p.args.clone())
                    .unwrap_or_default(),
                attrs: td
                    .members
                    .iter()
                    .filter_map(|m| {
                        if let MemberKind::Attribute {
                            name,
                            type_ann,
                            value,
                        } = &m.kind
                        {
                            Some((name.clone(), type_ann.clone(), value.clone()))
                        } else {
                            None
                        }
                    })
                    .collect(),
                type_params: td.params.clone(),
                methods: td
                    .members
                    .iter()
                    .filter_map(|m| {
                        if let MemberKind::Method(fd) = &m.kind {
                            Some(fd.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
            })
            .collect();

        let fn_entries: Vec<FunctionDecl> = hir.program.functions.clone();
        let main_body: Expr = hir.program.body.clone();

        // Build a map from type name → constructor params for implicit inheritance.
        let params_by_type: HashMap<String, Vec<hulk_hir::Param>> = hir
            .program
            .types
            .iter()
            .map(|td| (td.name.clone(), td.params.clone()))
            .collect();

        // Lower each type into a `TypeDescriptor` containing an `__init__`
        // function plus one `BannerFunction` per declared method.
        let mut types = Vec::new();
        for entry in type_entries {
            // When a type has no explicit constructor params but inherits from a
            // parent that does, inherit the parent's params and forward them.
            let parent_params: Option<&[hulk_hir::Param]> =
                if entry.type_params.is_empty() && entry.parent.is_some() {
                    entry
                        .parent
                        .as_ref()
                        .and_then(|p| params_by_type.get(p))
                        .map(|v| v.as_slice())
                        .filter(|p| !p.is_empty())
                } else {
                    None
                };

            let init_fn = self.lower_type_init(
                &entry.name,
                entry.parent.as_deref(),
                &entry.parent_args,
                &entry.type_params,
                parent_params,
                &entry.attrs,
            );

            let mut methods: Vec<BannerFunction> = vec![init_fn];
            for method in &entry.methods {
                let lowered = self.lower_method(&entry.name, entry.parent.as_deref(), method);
                methods.push(lowered);
            }

            // Field metadata: names and pointer flags for GC tracing.
            // Use the explicit type annotation when present; fall back to
            // the inferred type of the initializer expression.
            let fields: Vec<String> = entry
                .attrs
                .iter()
                .map(|(name, _, _)| name.clone())
                .collect();
            let field_kinds: Vec<crate::ir::FieldKind> = entry
                .attrs
                .iter()
                .map(|(_, ann, value)| {
                    if let Some(a) = ann {
                        match a {
                            TypeAnn::Named(n) if n == "Number" => crate::ir::FieldKind::Number,
                            TypeAnn::Named(n) if n == "Boolean" => crate::ir::FieldKind::Boolean,
                            _ => crate::ir::FieldKind::Reference,
                        }
                    } else {
                        let ty = self.hir.expr_type(value.id).unwrap_or(TypeId::OBJECT);
                        if ty == TypeId::NUMBER {
                            crate::ir::FieldKind::Number
                        } else if ty == TypeId::BOOLEAN {
                            crate::ir::FieldKind::Boolean
                        } else {
                            crate::ir::FieldKind::Reference
                        }
                    }
                })
                .collect();
            let pointer_map: Vec<bool> = field_kinds
                .iter()
                .map(|k| matches!(k, crate::ir::FieldKind::Reference))
                .collect();

            types.push(TypeDescriptor {
                name: entry.name,
                parent: entry.parent,
                fields,
                pointer_map,
                field_kinds,
                methods,
            });
        }

        // Lower top-level functions. Each function gets a fresh frame.
        let mut functions = Vec::new();
        for fd in &fn_entries {
            functions.push(self.lower_function(fd));
        }

        // Lower the program entry expression as `__main__`.
        self.reset_for_function();
        let body_val = self.emit_expr(&main_body);
        self.emit(Instr::Return(body_val));
        let main = BannerFunction {
            name: "__main__".to_string(),
            params: vec![],
            param_names: vec![],
            param_runtime_hints: vec![],
            body: std::mem::take(&mut self.instrs),
        };

        BannerProgram {
            types,
            functions,
            main,
        }
    }

    // -------- Per-function frame setup --------

    /// Map a parameter's type annotation to the runtime sentinel the codegen
    /// uses to dispatch builtin methods and resolve struct field accesses.
    ///
    /// - `Number[]` → `"$vector"`: enables `xs.size()` / `for (x in xs)` to
    ///   route through the C-runtime helpers.
    /// - `String`   → `"$string"`: enables `s.size()` / `s.charAt(i)` /
    ///   `s.substring(start, len)` to dispatch the same way.
    /// - Any other named type (user-defined struct) → the type name itself,
    ///   so that `param.field` expressions can resolve the struct layout in
    ///   codegen. Without this, `resolve_field` cannot find the struct type
    ///   for a non-self receiver (e.g., `other.x` in `dot(other: Vector)`).
    /// - `Number*` / primitive types / wildcard → `None` (resolved through
    ///   the vtable or handled as scalars).
    fn param_runtime_hint(ann: Option<&TypeAnn>) -> Option<String> {
        match ann? {
            TypeAnn::Vector(_) => Some("$vector".to_owned()),
            TypeAnn::Named(n) if n == "String" => Some("$string".to_owned()),
            // Primitive scalar types have no struct layout; leave them as-is.
            TypeAnn::Named(n) if matches!(n.as_str(), "Number" | "Boolean" | "Object") => None,
            // User-defined struct type: register the type name so codegen can
            // resolve field accesses on this parameter (e.g. `other.x`).
            TypeAnn::Named(n) => Some(n.clone()),
            _ => None,
        }
    }

    fn reset_for_function(&mut self) {
        self.instrs.clear();
        self.next_temp = 0;
        self.next_label = 0;
        self.locals.clear();
        self.locals_by_name.clear();
        self.param_temps.clear();
        self.shadow_count = 0;
        self.self_temp = None;
        self.current_type_name = None;
        self.current_parent_type_name = None;
        self.current_method_name = None;
    }

    fn setup_params(&mut self, params: &[hulk_hir::Param]) -> (Vec<TempId>, Vec<String>) {
        let mut temps = Vec::with_capacity(params.len());
        let mut names = Vec::with_capacity(params.len());
        for p in params {
            let t = self.fresh_temp();
            self.param_temps.insert(p.name.clone(), t);
            temps.push(t);
            names.push(p.name.clone());
        }
        (temps, names)
    }

    fn lower_function(&mut self, fd: &FunctionDecl) -> BannerFunction {
        self.reset_for_function();
        let (param_temps, param_names) = self.setup_params(&fd.params);
        let param_runtime_hints: Vec<Option<String>> = fd
            .params
            .iter()
            .map(|p| Self::param_runtime_hint(p.type_ann.as_ref()))
            .collect();
        let body_val = self.emit_expr(&fd.body);
        self.emit(Instr::Return(body_val));
        BannerFunction {
            name: fd.name.clone(),
            params: param_temps,
            param_names,
            param_runtime_hints,
            body: std::mem::take(&mut self.instrs),
        }
    }

    fn lower_type_init(
        &mut self,
        type_name: &str,
        parent_name: Option<&str>,
        parent_args: &[Expr],
        type_params: &[hulk_hir::Param],
        parent_params: Option<&[hulk_hir::Param]>,
        attrs: &[(String, Option<TypeAnn>, Expr)],
    ) -> BannerFunction {
        self.reset_for_function();
        // `self` is the first parameter for __init__.
        let t_self = self.fresh_temp();
        self.self_temp = Some(t_self);
        self.current_type_name = Some(type_name.to_string());
        self.current_parent_type_name = parent_name.map(|s| s.to_string());
        self.current_method_name = Some("__init__".to_string());

        let (mut user_params, mut param_names) = self.setup_params(type_params);

        // Chain the parent constructor when the type inherits.
        // Skip when the parent is a protocol — protocols define no
        // constructor and emitting StaticCall("Protocol.__init__") would
        // crash at codegen with "static call target not declared". A type
        // inheriting a protocol still has the protocol's methods available
        // through structural conformance; no chained init is needed.
        let parent_is_protocol = parent_name
            .map(|p| self.hir.program.protocols.iter().any(|pr| pr.name == p))
            .unwrap_or(false);
        if let Some(parent) = parent_name.filter(|_| !parent_is_protocol) {
            let mut args: Vec<Value> = vec![Value::Temp(t_self)];
            if parent_args.is_empty() {
                if let Some(pp) = parent_params {
                    // Implicit constructor inheritance: no explicit params declared,
                    // so register the parent's params as our own and forward them.
                    let (inherited_temps, inherited_names) = self.setup_params(pp);
                    for &t in &inherited_temps {
                        args.push(Value::Temp(t));
                    }
                    user_params = inherited_temps;
                    param_names = inherited_names;
                }
            } else {
                for arg in parent_args {
                    let v = self.emit_expr(arg);
                    args.push(v);
                }
            }
            let dst = self.fresh_temp();
            self.emit(Instr::StaticCall {
                dst,
                type_name: parent.to_string(),
                method: "__init__".to_string(),
                args,
            });
        }

        // Initialize each declared attribute on `self`.
        for (name, _, value) in attrs {
            let v = self.emit_expr(value);
            self.emit(Instr::SetField {
                object: Value::Temp(t_self),
                field: name.clone(),
                value: v,
            });
        }

        self.emit(Instr::Return(Value::Temp(t_self)));

        // The __init__ signature is (self, ...type_params).
        let mut params = vec![t_self];
        params.append(&mut user_params);
        let mut names = vec!["self".to_string()];
        names.append(&mut param_names);
        let mut hints: Vec<Option<String>> = vec![None]; // self
        hints.extend(
            type_params
                .iter()
                .map(|p| Self::param_runtime_hint(p.type_ann.as_ref())),
        );
        // Pad to the actual param count in case parent_params were used.
        while hints.len() < params.len() {
            hints.push(None);
        }

        BannerFunction {
            name: format!("{type_name}.__init__"),
            params,
            param_names: names,
            param_runtime_hints: hints,
            body: std::mem::take(&mut self.instrs),
        }
    }

    fn lower_method(
        &mut self,
        type_name: &str,
        parent_name: Option<&str>,
        method: &FunctionDecl,
    ) -> BannerFunction {
        self.reset_for_function();
        let t_self = self.fresh_temp();
        self.self_temp = Some(t_self);
        self.current_type_name = Some(type_name.to_string());
        self.current_parent_type_name = parent_name.map(|n| n.to_string());
        self.current_method_name = Some(method.name.clone());

        let (mut user_params, mut param_names) = self.setup_params(&method.params);

        let body_val = self.emit_expr(&method.body);
        self.emit(Instr::Return(body_val));

        let mut params = vec![t_self];
        params.append(&mut user_params);
        let mut names = vec!["self".to_string()];
        names.append(&mut param_names);
        let mut hints: Vec<Option<String>> = vec![None]; // self
        hints.extend(
            method
                .params
                .iter()
                .map(|p| Self::param_runtime_hint(p.type_ann.as_ref())),
        );

        BannerFunction {
            name: format!("{type_name}.{}", method.name),
            params,
            param_names: names,
            param_runtime_hints: hints,
            body: std::mem::take(&mut self.instrs),
        }
    }

    // -------- Helpers --------

    fn fresh_temp(&mut self) -> TempId {
        let t = TempId(self.next_temp);
        self.next_temp += 1;
        t
    }

    fn fresh_label(&mut self, hint: &str) -> String {
        let label = format!("{hint}_{}", self.next_label);
        self.next_label += 1;
        label
    }

    fn emit(&mut self, instr: Instr) {
        self.instrs.push(instr);
    }

    // Conservative reference-vs-value classifier used to decide when a
    // freshly bound temporary must be pushed onto the GC shadow stack.
    // Builtin numeric and boolean types are unboxed; everything else
    // (strings, vectors, user types, Object) is treated as a heap pointer.
    fn is_reference(ty: TypeId) -> bool {
        ty != TypeId::NUMBER && ty != TypeId::BOOLEAN
    }

    // If `val` is a numeric constant or its HIR source type is Number/Boolean,
    // emit a hulk_number_to_string call and return a Value::Temp pointing to the
    // resulting HulkStr. Otherwise return val unchanged (already a string/ptr).
    fn coerce_to_str_val(&mut self, val: Value, expr: &Expr) -> Value {
        let is_numeric = match &val {
            Value::ConstNum(_) | Value::ConstBool(_) => true,
            _ => {
                let ty = self.hir.expr_type(expr.id).unwrap_or(TypeId::OBJECT);
                !Self::is_reference(ty)
            }
        };
        if is_numeric {
            let tmp = self.fresh_temp();
            self.emit(Instr::Call {
                dst: tmp,
                callee: Value::Global("hulk_number_to_string".to_string()),
                args: vec![val],
            });
            Value::Temp(tmp)
        } else {
            val
        }
    }

    // -------- Expression dispatcher --------

    fn emit_expr(&mut self, expr: &Expr) -> Value {
        match &expr.kind {
            ExprKind::Number(v) => Value::ConstNum(*v),
            ExprKind::StringLit(s) => Value::ConstStr(s.clone()),
            ExprKind::Bool(b) => Value::ConstBool(*b),
            ExprKind::Ident(_name) => self.emit_ident(expr),
            ExprKind::Self_ => Value::Temp(
                // Resolver emits a diagnostic and rejects programs that use `self` outside a method.
                self.self_temp
                    .expect("Self_ outside of a method body"),
            ),
            ExprKind::Base => {
                // When a local variable named `base` shadows the keyword, the
                // resolver stores a Variable or Parameter symbol for this
                // node. In that case emit it like a regular identifier.
                // When `base` is the method-override pseudo-function the
                // resolver stores a Function symbol; emit via self_temp so
                // emit_call can dispatch the static parent-method call.
                let is_local = self
                    .hir
                    .resolved_symbol(expr.id)
                    .and_then(|sym| self.hir.symbols.table().get(sym))
                    .map(|s| {
                        matches!(s.kind, SymbolKind::Variable | SymbolKind::Parameter)
                    })
                    .unwrap_or(false);
                if is_local {
                    self.emit_ident(expr)
                } else {
                    Value::Temp(
                        self.self_temp
                            .expect("Base outside of a method body"),
                    )
                }
            }
            ExprKind::BinOp { op, left, right } => self.emit_binop(left, *op, right),
            ExprKind::UnaryOp { op, expr: operand } => {
                let v = self.emit_expr(operand);
                let dst = self.fresh_temp();
                self.emit(Instr::UnOp {
                    dst,
                    op: *op,
                    operand: v,
                });
                Value::Temp(dst)
            }
            ExprKind::Call { callee, args } => self.emit_call(callee, args),
            ExprKind::MethodCall { receiver, method, args } => {
                let rv = self.emit_expr(receiver);
                let arg_vals: Vec<Value> = args.iter().map(|a| self.emit_expr(a)).collect();
                let dst = self.fresh_temp();
                self.emit(Instr::MethodCall {
                    dst,
                    receiver: rv,
                    method: method.clone(),
                    args: arg_vals,
                });
                Value::Temp(dst)
            }
            ExprKind::New { type_ann, args } => {
                let TypeAnn::Named(type_name) = type_ann else {
                    panic!(
                        "lowerer encountered a New with a non-Named TypeAnn after semantic analysis: {type_ann:?}"
                    );
                };
                let arg_vals: Vec<Value> = args.iter().map(|a| self.emit_expr(a)).collect();
                let dst = self.fresh_temp();
                self.emit(Instr::New {
                    dst,
                    type_name: type_name.clone(),
                    args: arg_vals,
                });
                Value::Temp(dst)
            }
            ExprKind::FieldAccess { receiver, field } => {
                let rv = self.emit_expr(receiver);
                let dst = self.fresh_temp();
                self.emit(Instr::GetField {
                    dst,
                    object: rv,
                    field: field.clone(),
                });
                Value::Temp(dst)
            }
            ExprKind::Index { target, index } => {
                let tv = self.emit_expr(target);
                let iv = self.emit_expr(index);
                let dst = self.fresh_temp();
                self.emit(Instr::GetIndex {
                    dst,
                    target: tv,
                    index: iv,
                });
                Value::Temp(dst)
            }
            ExprKind::Block(exprs) => {
                if exprs.is_empty() {
                    return Value::ConstNull;
                }
                let mut last = Value::ConstNull;
                for e in exprs {
                    last = self.emit_expr(e);
                }
                last
            }
            ExprKind::Let { bindings, body } => self.emit_let(bindings, body),
            ExprKind::LetBinding(lb) => self.emit_let_binding_expr(lb, expr),
            ExprKind::Assign { target, value } => self.emit_assign(target, value),
            ExprKind::AssignTarget(target) => panic!(
                "bare AssignTarget node reached lowerer — should only appear inside Assign (node {:?})",
                target
            ),
            ExprKind::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => self.emit_if(condition, then_branch, elif_branches, else_branch.as_deref()),
            ExprKind::While { condition, body } => self.emit_while(condition, body),
            ExprKind::VecLiteral(elems) => self.emit_vec_literal(elems),
            ExprKind::Is { expr: inner, type_ann } => {
                let tv = self.emit_expr(inner);
                let TypeAnn::Named(type_name) = type_ann else {
                    panic!("lowerer encountered an Is with a non-Named TypeAnn: {type_ann:?}");
                };
                let dst = self.fresh_temp();
                self.emit(Instr::Call {
                    dst,
                    callee: Value::Global("__hulk_is".to_string()),
                    args: vec![tv, Value::Global(type_name.clone())],
                });
                Value::Temp(dst)
            }
            ExprKind::As { expr: inner, type_ann } => {
                let tv = self.emit_expr(inner);
                let TypeAnn::Named(type_name) = type_ann else {
                    panic!("lowerer encountered an As with a non-Named TypeAnn: {type_ann:?}");
                };
                let dst = self.fresh_temp();
                self.emit(Instr::Call {
                    dst,
                    callee: Value::Global("__hulk_as".to_string()),
                    args: vec![tv, Value::Global(type_name.clone())],
                });
                Value::Temp(dst)
            }
            ExprKind::For { .. } => panic!(
                "lowerer encountered a node that should have been desugared: For"
            ),
            ExprKind::Lambda { .. } => panic!(
                "lowerer encountered a node that should have been desugared: Lambda"
            ),
            ExprKind::VecGenerator { .. } => panic!(
                "lowerer encountered a node that should have been desugared: VecGenerator"
            ),
        }
    }

    fn emit_ident(&mut self, expr: &Expr) -> Value {
        // Resolver stores a symbol for every Ident that passes semantic analysis.
        // Desugared nodes (e.g., from for-loop desugar) may have no resolved symbol;
        // in that case fall back to the name-keyed map.
        if let Some(sym) = self.hir.resolved_symbol(expr.id) {
            let table = self.hir.symbols.table();
            let symbol: &Symbol = table.get(sym).expect("symbol not in table");
            match &symbol.kind {
                SymbolKind::Variable => {
                    if let Some(&t) = self.locals.get(&sym) {
                        Value::Temp(t)
                    } else {
                        // Desugared binding: the LetBinding got a fresh NodeId so
                        // it was stored in locals_by_name instead of locals.
                        let name = table.name_of(sym).expect("variable has no name");
                        let t = *self.locals_by_name.get(name).unwrap_or_else(|| {
                            panic!("variable '{name}' not in locals or locals_by_name")
                        });
                        Value::Temp(t)
                    }
                }
                SymbolKind::Parameter => {
                    let name = table.name_of(sym).expect("param has no name");
                    let t = *self
                        .param_temps
                        .get(name)
                        .expect("param not in param_temps");
                    Value::Temp(t)
                }
                SymbolKind::SelfValue => Value::Temp(
                    self.self_temp
                        .expect("SelfValue ident outside of a method body"),
                ),
                SymbolKind::Function | SymbolKind::BuiltinFunction | SymbolKind::Macro => {
                    let n = table.name_of(sym).unwrap().to_string();
                    Value::Global(n)
                }
                SymbolKind::BuiltinValue => {
                    let name = table.name_of(sym).unwrap();
                    match name {
                        "PI" => Value::ConstNum(std::f64::consts::PI),
                        "E" => Value::ConstNum(std::f64::consts::E),
                        other => panic!("unknown BuiltinValue: {other}"),
                    }
                }
                other => panic!("Ident resolved to unexpected SymbolKind: {other:?}"),
            }
        } else {
            // Fallback for desugared Idents without a resolved symbol.
            let ExprKind::Ident(name) = &expr.kind else {
                panic!("emit_ident called on non-Ident node without resolved symbol");
            };
            if let Some(&t) = self.locals_by_name.get(name) {
                return Value::Temp(t);
            }
            if let Some(&t) = self.param_temps.get(name) {
                return Value::Temp(t);
            }
            // Treat as a global function/builtin name.
            Value::Global(name.clone())
        }
    }

    fn emit_binop(&mut self, left: &Expr, op: BinOpKind, right: &Expr) -> Value {
        let lv = self.emit_expr(left);
        let rv = self.emit_expr(right);
        let dst = self.fresh_temp();
        match op {
            BinOpKind::Concat => {
                // Coerce numeric/boolean operands to strings before concatenating.
                // hulk_number_to_string(double) -> void* (HulkStr heap object).
                let lv_str = self.coerce_to_str_val(lv, left);
                let rv_str = self.coerce_to_str_val(rv, right);
                self.emit(Instr::Call {
                    dst,
                    callee: Value::Global("__hulk_concat".to_string()),
                    args: vec![lv_str, rv_str],
                });
            }
            BinOpKind::ConcatSpaced => {
                panic!("ConcatSpaced should have been desugared before BANNER lowering");
            }
            BinOpKind::Eq | BinOpKind::Ne if self.either_operand_is_string(left, right) => {
                // At least one operand is a String: use the byte-level runtime
                // comparator instead of the float-comparison path so that Ptr-typed
                // LLVM values do not end up in `feq`/`fne` instructions.
                self.emit(Instr::Call {
                    dst,
                    callee: Value::Global("__hulk_str_eq".to_string()),
                    args: vec![lv, rv],
                });
                if op == BinOpKind::Ne {
                    // Negate the equality result to get inequality.
                    let neg_dst = self.fresh_temp();
                    self.emit(Instr::UnOp {
                        dst: neg_dst,
                        op: UnaryOpKind::Not,
                        operand: Value::Temp(dst),
                    });
                    return Value::Temp(neg_dst);
                }
            }
            _ => {
                self.emit(Instr::BinOp {
                    dst,
                    op,
                    left: lv,
                    right: rv,
                });
            }
        }
        Value::Temp(dst)
    }

    /// Returns `true` when the HIR type of either operand is `String`.
    ///
    /// Also treats `ConstStr` values and `Value::ConstStr` literals as strings
    /// so that purely literal comparisons like `"a" == "b"` are covered even
    /// when type inference has not annotated the node.
    fn either_operand_is_string(&self, left: &Expr, right: &Expr) -> bool {
        self.expr_is_string(left) || self.expr_is_string(right)
    }

    /// Returns `true` when the expression's HIR type is `String`, or when the
    /// expression is a string literal.
    fn expr_is_string(&self, expr: &Expr) -> bool {
        if matches!(expr.kind, ExprKind::StringLit(_)) {
            return true;
        }
        let ty = self.hir.expr_type(expr.id).unwrap_or(TypeId::OBJECT);
        ty == TypeId::STRING
    }

    fn emit_call(&mut self, callee: &Expr, args: &[Expr]) -> Value {
        // `base(args)` becomes a static dispatch to the parent's
        // implementation of the current method, prepending `self`.
        if let ExprKind::Base = &callee.kind {
            // Resolver validates that base() only appears inside a method of a type with a parent.
            let parent = self
                .current_parent_type_name
                .clone()
                .expect("base() call outside of an inheriting method");
            let method = self
                .current_method_name
                .clone()
                .expect("base() call outside of a method body");
            let self_t = self
                .self_temp
                .expect("base() call without a self temporary");
            let mut arg_vals: Vec<Value> = vec![Value::Temp(self_t)];
            for a in args {
                let v = self.emit_expr(a);
                arg_vals.push(v);
            }
            let dst = self.fresh_temp();
            self.emit(Instr::StaticCall {
                dst,
                type_name: parent,
                method,
                args: arg_vals,
            });
            return Value::Temp(dst);
        }

        let cv = self.emit_expr(callee);
        let arg_vals: Vec<Value> = args.iter().map(|a| self.emit_expr(a)).collect();
        let dst = self.fresh_temp();
        self.emit(Instr::Call {
            dst,
            callee: cv,
            args: arg_vals,
        });
        Value::Temp(dst)
    }

    fn emit_let(&mut self, bindings: &[Expr], body: &Expr) -> Value {
        // Each `let` introduces its own GC shadow frame: bindings push, the
        // matching pops happen after the body. Saving/restoring the counter
        // keeps nested lets from clobbering an outer frame's pop count.
        let saved = self.shadow_count;
        self.shadow_count = 0;
        for b in bindings {
            self.emit_expr(b);
        }
        let body_val = self.emit_expr(body);
        let pops = self.shadow_count;
        for _ in 0..pops {
            self.emit(Instr::ShadowPop);
        }
        self.shadow_count = saved;
        body_val
    }

    fn emit_let_binding_expr(&mut self, lb: &LetBinding, expr: &Expr) -> Value {
        let val = self.emit_expr(&lb.value);
        let dst = self.fresh_temp();
        self.emit(Instr::Copy { dst, src: val });

        if let Some(sym_id) = self.hir.resolved_symbol(expr.id) {
            self.locals.insert(sym_id, dst);
        } else {
            // Desugared binding (e.g., from for-loop): track by name.
            self.locals_by_name.insert(lb.name.clone(), dst);
        }

        // Only push reference types onto the GC shadow stack.
        let ty = self.hir.expr_type(lb.value.id).unwrap_or(TypeId::OBJECT);
        if Self::is_reference(ty) {
            self.emit(Instr::ShadowPush(Value::Temp(dst)));
            self.shadow_count += 1;
        }
        Value::Temp(dst)
    }

    fn emit_assign(&mut self, target: &Expr, value: &Expr) -> Value {
        let v = self.emit_expr(value);
        let target_kind = match &target.kind {
            ExprKind::AssignTarget(t) => t,
            _ => panic!("Assign target is not an AssignTarget node"),
        };
        match target_kind {
            AssignTarget::Ident(name) => {
                // Resolved-symbol lookup is the fast path. Macro expansion
                // refreshes node IDs on the substituted body, which drops
                // expr_symbols entries for AssignTarget nodes; in that
                // case fall back to a name lookup against the active
                // locals_by_name / param_temps maps. Without the fallback,
                // any macro whose body contains `:=` panics the lowerer.
                if let Some(sym) = self.hir.resolved_symbol(target.id) {
                    let table = self.hir.symbols.table();
                    // SymbolIds are only created by the SymbolTable, so they are always valid.
                    let symbol = table.get(sym).expect("symbol not in table");
                    match &symbol.kind {
                        SymbolKind::Variable => {
                            // Variables are inserted into `locals` when their LetBinding is processed.
                            let dst = *self.locals.get(&sym).expect("variable not in locals");
                            self.emit(Instr::Copy {
                                dst,
                                src: v.clone(),
                            });
                            return v;
                        }
                        SymbolKind::Parameter => {
                            // Parameter names are always present in the symbol table.
                            let name = table.name_of(sym).expect("param has no name").to_string();
                            // Params are registered in `param_temps` during function frame setup.
                            let dst = *self
                                .param_temps
                                .get(&name)
                                .expect("param not in param_temps");
                            self.emit(Instr::Copy {
                                dst,
                                src: v.clone(),
                            });
                            return v;
                        }
                        other => {
                            panic!(
                                "AssignTarget::Ident resolved to unexpected SymbolKind: {other:?}"
                            )
                        }
                    }
                }

                // Fallback: macro-expanded body lost its resolved symbol.
                // Look up the local by name (sanitised macro locals land
                // in locals_by_name, function params in param_temps).
                if let Some(&dst) = self.locals_by_name.get(name) {
                    self.emit(Instr::Copy {
                        dst,
                        src: v.clone(),
                    });
                    return v;
                }
                if let Some(&dst) = self.param_temps.get(name) {
                    self.emit(Instr::Copy {
                        dst,
                        src: v.clone(),
                    });
                    return v;
                }
                panic!(
                    "AssignTarget::Ident '{name}' has no resolved symbol and no matching local/param"
                );
            }
            AssignTarget::Field { receiver, field } => {
                let rv = self.emit_expr(receiver);
                self.emit(Instr::SetField {
                    object: rv,
                    field: field.clone(),
                    value: v.clone(),
                });
                v
            }
            AssignTarget::Index { target: t, index } => {
                let tv = self.emit_expr(t);
                let iv = self.emit_expr(index);
                self.emit(Instr::SetIndex {
                    target: tv,
                    index: iv,
                    value: v.clone(),
                });
                v
            }
        }
    }

    fn emit_if(
        &mut self,
        condition: &Expr,
        then_branch: &Expr,
        elif_branches: &[(Expr, Expr)],
        else_branch: Option<&Expr>,
    ) -> Value {
        let t_res = self.fresh_temp();
        let then_label = self.fresh_label("then");
        let elif_labels: Vec<String> = (0..elif_branches.len())
            .map(|i| self.fresh_label(&format!("elif{i}")))
            .collect();
        let end_label = self.fresh_label("endif");

        // Evaluate the leading condition and conditionally branch to the
        // primary `then` block.
        let cv = self.emit_expr(condition);
        self.emit(Instr::JumpIf {
            condition: cv,
            label: then_label.clone(),
        });

        // Each elif condition is evaluated in order along the fall-through
        // path; the first matching one jumps to its own body.
        for (i, (elif_cond, _)) in elif_branches.iter().enumerate() {
            let cv = self.emit_expr(elif_cond);
            self.emit(Instr::JumpIf {
                condition: cv,
                label: elif_labels[i].clone(),
            });
        }

        // Fall-through path: lowers the `else` branch (or null) and jumps to
        // the join point.
        let else_val = match else_branch {
            Some(e) => self.emit_expr(e),
            None => Value::ConstNull,
        };
        self.emit(Instr::Copy {
            dst: t_res,
            src: else_val,
        });
        self.emit(Instr::Jump(end_label.clone()));

        // Each elif body sits between the fall-through path and the `then`
        // body. They jump to `end` after writing the result.
        for (i, (_, elif_body)) in elif_branches.iter().enumerate() {
            self.emit(Instr::Label(elif_labels[i].clone()));
            let bv = self.emit_expr(elif_body);
            self.emit(Instr::Copy {
                dst: t_res,
                src: bv,
            });
            self.emit(Instr::Jump(end_label.clone()));
        }

        // The `then` block is laid out last so it falls through to `end`,
        // saving one explicit jump.
        self.emit(Instr::Label(then_label));
        let tv = self.emit_expr(then_branch);
        self.emit(Instr::Copy {
            dst: t_res,
            src: tv,
        });

        self.emit(Instr::Label(end_label));
        Value::Temp(t_res)
    }

    fn emit_while(&mut self, condition: &Expr, body: &Expr) -> Value {
        let loop_label = self.fresh_label("loop");
        let end_label = self.fresh_label("endloop");

        self.emit(Instr::Label(loop_label.clone()));
        let cv = self.emit_expr(condition);
        // The loop exits when the condition is false; `JumpIf` only branches
        // on truthy values, so the negation gives us a clean exit edge.
        let neg = self.fresh_temp();
        self.emit(Instr::UnOp {
            dst: neg,
            op: UnaryOpKind::Not,
            operand: cv,
        });
        self.emit(Instr::JumpIf {
            condition: Value::Temp(neg),
            label: end_label.clone(),
        });
        let _ = self.emit_expr(body);
        self.emit(Instr::Jump(loop_label));
        self.emit(Instr::Label(end_label));
        Value::ConstNull
    }

    fn emit_vec_literal(&mut self, elems: &[Expr]) -> Value {
        let n = elems.len() as f64;
        let t = self.fresh_temp();
        self.emit(Instr::Call {
            dst: t,
            callee: Value::Global("__vec_new".to_string()),
            args: vec![Value::ConstNum(n)],
        });
        for elem in elems {
            let ev = self.emit_expr(elem);
            let push_dst = self.fresh_temp();
            self.emit(Instr::Call {
                dst: push_dst,
                callee: Value::Global("__vec_push".to_string()),
                args: vec![Value::Temp(t), ev],
            });
        }
        Value::Temp(t)
    }
}
