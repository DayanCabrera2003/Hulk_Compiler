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
    // Maps a parameter name to the temporary assigned in the current frame.
    // Param names are used (instead of SymbolId) because Param nodes have no
    // NodeId and cannot be resolved through `hir.resolved_symbol`.
    param_temps: HashMap<String, TempId>,
    // Number of `ShadowPush` emissions made inside the current `let` frame.
    // Saved/restored around each `Let` so nested lets pop only their own.
    shadow_count: usize,
    self_temp: Option<TempId>,
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
            attrs: Vec<(String, Expr)>,
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
                        if let MemberKind::Attribute { name, value, .. } = &m.kind {
                            Some((name.clone(), value.clone()))
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

        // Lower each type into a `TypeDescriptor` containing an `__init__`
        // function plus one `BannerFunction` per declared method.
        let mut types = Vec::new();
        for entry in type_entries {
            let init_fn =
                self.lower_type_init(&entry.name, entry.parent.as_deref(), &entry.parent_args,
                                     &entry.type_params, &entry.attrs);

            let mut methods: Vec<BannerFunction> = vec![init_fn];
            for method in &entry.methods {
                let lowered = self.lower_method(&entry.name, entry.parent.as_deref(), method);
                methods.push(lowered);
            }

            // Field metadata is filled by a later pass (Task 5+); the
            // descriptor owns the names and pointer flags but the lowerer
            // emits attribute initialization through __init__ instead.
            let fields: Vec<String> = entry.attrs.iter().map(|(name, _)| name.clone()).collect();
            let pointer_map: Vec<bool> = entry
                .attrs
                .iter()
                .map(|(_, value)| {
                    let ty = self.hir.expr_type(value.id).unwrap_or(TypeId::OBJECT);
                    Self::is_reference(ty)
                })
                .collect();

            types.push(TypeDescriptor {
                name: entry.name,
                parent: entry.parent,
                fields,
                pointer_map,
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
            body: std::mem::take(&mut self.instrs),
        };

        BannerProgram {
            types,
            functions,
            main,
        }
    }

    // -------- Per-function frame setup --------

    fn reset_for_function(&mut self) {
        self.instrs.clear();
        self.next_temp = 0;
        self.next_label = 0;
        self.locals.clear();
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
        let body_val = self.emit_expr(&fd.body);
        self.emit(Instr::Return(body_val));
        BannerFunction {
            name: fd.name.clone(),
            params: param_temps,
            param_names,
            body: std::mem::take(&mut self.instrs),
        }
    }

    fn lower_type_init(
        &mut self,
        type_name: &str,
        parent_name: Option<&str>,
        parent_args: &[Expr],
        type_params: &[hulk_hir::Param],
        attrs: &[(String, Expr)],
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
        if let Some(parent) = parent_name {
            let mut args: Vec<Value> = vec![Value::Temp(t_self)];
            for arg in parent_args {
                let v = self.emit_expr(arg);
                args.push(v);
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
        for (name, value) in attrs {
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

        BannerFunction {
            name: "__init__".to_string(),
            params,
            param_names: names,
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

        BannerFunction {
            name: method.name.clone(),
            params,
            param_names: names,
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

    // -------- Expression dispatcher --------

    fn emit_expr(&mut self, expr: &Expr) -> Value {
        match &expr.kind {
            ExprKind::Number(v) => Value::ConstNum(*v),
            ExprKind::StringLit(s) => Value::ConstStr(s.clone()),
            ExprKind::Bool(b) => Value::ConstBool(*b),
            ExprKind::Ident(_name) => self.emit_ident(expr),
            ExprKind::Self_ => Value::Temp(
                self.self_temp
                    .expect("Self_ outside of a method body"),
            ),
            ExprKind::Base => Value::Temp(
                self.self_temp
                    .expect("Base outside of a method body"),
            ),
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
        let sym = self
            .hir
            .resolved_symbol(expr.id)
            .expect("Ident has no resolved symbol");
        let table = self.hir.symbols.table();
        let symbol: &Symbol = table.get(sym).expect("symbol not in table");
        match &symbol.kind {
            SymbolKind::Variable => {
                let t = *self
                    .locals
                    .get(&sym)
                    .expect("variable not in locals");
                Value::Temp(t)
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
    }

    fn emit_binop(&mut self, left: &Expr, op: BinOpKind, right: &Expr) -> Value {
        let lv = self.emit_expr(left);
        let rv = self.emit_expr(right);
        let dst = self.fresh_temp();
        match op {
            BinOpKind::Concat => {
                // String concatenation lowers to a runtime helper call so
                // backends only need to deal with primitive arithmetic in
                // BinOp instructions.
                self.emit(Instr::Call {
                    dst,
                    callee: Value::Global("__hulk_concat".to_string()),
                    args: vec![lv, rv],
                });
            }
            BinOpKind::ConcatSpaced => {
                panic!("ConcatSpaced should have been desugared before BANNER lowering");
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

    fn emit_call(&mut self, callee: &Expr, args: &[Expr]) -> Value {
        // `base(args)` becomes a static dispatch to the parent's
        // implementation of the current method, prepending `self`.
        if let ExprKind::Base = &callee.kind {
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
        let sym_id = self
            .hir
            .resolved_symbol(expr.id)
            .expect("LetBinding has no resolved symbol — resolver extension missing");
        let val = self.emit_expr(&lb.value);
        let dst = self.fresh_temp();
        self.emit(Instr::Copy { dst, src: val });
        self.locals.insert(sym_id, dst);
        // Conservative classification: if the type is unknown, treat the
        // value as a reference so the GC can still find it.
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
            AssignTarget::Ident(_) => {
                let sym = self
                    .hir
                    .resolved_symbol(target.id)
                    .expect("AssignTarget::Ident has no resolved symbol");
                let table = self.hir.symbols.table();
                let symbol = table.get(sym).expect("symbol not in table");
                match &symbol.kind {
                    SymbolKind::Variable => {
                        let dst = *self
                            .locals
                            .get(&sym)
                            .expect("variable not in locals");
                        self.emit(Instr::Copy { dst, src: v.clone() });
                        v
                    }
                    SymbolKind::Parameter => {
                        let name = table.name_of(sym).expect("param has no name").to_string();
                        let dst = *self
                            .param_temps
                            .get(&name)
                            .expect("param not in param_temps");
                        self.emit(Instr::Copy { dst, src: v.clone() });
                        v
                    }
                    other => panic!("AssignTarget::Ident resolved to unexpected SymbolKind: {other:?}"),
                }
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
            self.emit(Instr::Copy { dst: t_res, src: bv });
            self.emit(Instr::Jump(end_label.clone()));
        }

        // The `then` block is laid out last so it falls through to `end`,
        // saving one explicit jump.
        self.emit(Instr::Label(then_label));
        let tv = self.emit_expr(then_branch);
        self.emit(Instr::Copy { dst: t_res, src: tv });

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
