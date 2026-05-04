# Split Large Files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce every `.rs` file in the workspace to ≤ 500 lines by migrating hand-written AST walkers to the existing `hulk-ast::VisitorMut` trait and splitting oversized modules and test files by responsibility.

**Architecture:** Three-phase refactor on branch `feature/Desugaring`. Phase 1 replaces duplicated `ExprKind` walks with the existing visitor trait. Phase 2 splits large modules into submodules by responsibility. Phase 3 splits large integration test files by feature. All existing tests pass unchanged at every commit.

**Tech Stack:** Rust 2021, cargo workspace with multiple crates under `crates/`, `hulk-ast::{Visitor, VisitorMut}` as the shared recursion abstraction.

**Spec:** [docs/superpowers/specs/2026-04-23-split-large-files-design.md](../specs/2026-04-23-split-large-files-design.md)

**Rules for every commit (do NOT forget):**
- Run `cargo test --workspace` → **all green**.
- Run `cargo clippy --workspace --all-targets -- -D warnings` → **no output (clean)**.
- **Never** include `Co-Authored-By: Claude ...` trailers or any AI-attribution line in commit messages. User preference is hard.
- Prefer specific `git add <paths>` over `git add -A`.

---

## Phase 1 — Visitor Migration

Replace hand-written recursive `match &expr.kind` walks with `impl VisitorMut for X` using the existing trait in `crates/hulk-ast/src/visitor.rs`. Each walker becomes a small struct that implements `visit_expr`, delegates recursion to `walk_expr_mut(self, expr)`, and holds any state it needs.

### Task 1.1: Migrate `hulk-macros` walkers to `VisitorMut`

**Files:**
- Modify: `crates/hulk-macros/Cargo.toml` (add `hulk-ast` re-use if not already transitive)
- Modify: `crates/hulk-macros/src/lib.rs` (9 walkers → VisitorMut/Visitor impls)

**Walkers to migrate (7 production + 2 test helpers)**:
1. `MacroExpander::expand_expr_children` (lines ~194-286)
2. `substitute_params` (lines ~987-1119)
3. `bind_placeholder_idents` (lines ~1143-1253)
4. `refresh_node_ids` (lines ~1255-1349)
5. `visit_max_node_id` (lines ~1373-1467)
6. `simplify_algebraic` (lines ~662-792)
7. `LocalSanitizer::visit_expr` (lines ~843-1006)
8. test helper `collect_identifiers`
9. test helper `collect_ident_node_ids`

- [ ] **Step 1: Verify baseline green**

Run: `cargo test -p hulk-macros && cargo clippy -p hulk-macros --all-targets -- -D warnings`
Expected: 11 passed, 0 failed. No clippy output.

- [ ] **Step 2: Add hulk-ast dependency if missing**

Check `crates/hulk-macros/Cargo.toml`. `hulk-macros` already imports `hulk_hir` which re-exports `hulk_ast`. Confirm the path `hulk_hir::{Visitor, VisitorMut, walk_expr_mut, walk_expr}` resolves. If not, import directly via `hulk_ast`.

- [ ] **Step 3: Migrate `refresh_node_ids` first (smallest, purest)**

`refresh_node_ids` is a pure mutable walk that assigns a fresh NodeId to every expression. It has no scope state. Best candidate to validate the pattern.

Replace with:

```rust
use hulk_hir::{walk_expr_mut, VisitorMut};

struct RefreshNodeIds<'a> {
    node_ids: &'a mut NodeIdGen,
}

impl<'a> VisitorMut for RefreshNodeIds<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        expr.id = self.node_ids.next_id();
        walk_expr_mut(self, expr);
    }
}

fn refresh_node_ids(expr: &mut Expr, node_ids: &mut NodeIdGen) {
    RefreshNodeIds { node_ids }.visit_expr_mut(expr);
}
```

Delete the old `match &mut expr.kind { ... }` body of `refresh_node_ids`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p hulk-macros`
Expected: all 11 tests pass. If any fails, the visitor trait does not traverse a variant the old code handled — `walk_expr_mut` in `hulk-ast/src/visitor.rs` is the authoritative walker, cross-check its coverage.

- [ ] **Step 5: Migrate `visit_max_node_id` (immutable walk)**

Use `Visitor` (non-mut):

```rust
use hulk_hir::{walk_expr, Visitor};

struct MaxNodeId {
    max: u32,
}

impl Visitor for MaxNodeId {
    fn visit_expr(&mut self, expr: &Expr) {
        self.max = self.max.max(expr.id.0);
        walk_expr(self, expr);
    }
}

fn visit_max_node_id(expr: &Expr, max_id: &mut u32) {
    let mut v = MaxNodeId { max: *max_id };
    v.visit_expr(expr);
    *max_id = v.max;
}
```

- [ ] **Step 6: Run tests, commit intermediate progress**

```bash
cargo test -p hulk-macros
cargo clippy -p hulk-macros --all-targets -- -D warnings
git add crates/hulk-macros/src/lib.rs
git commit -m "refactor(hulk-macros): migrate refresh_node_ids and visit_max_node_id to VisitorMut/Visitor"
```

- [ ] **Step 7: Migrate `simplify_algebraic`**

Pure mutable walk with a local rewrite at `BinOp` nodes. Override `visit_expr_mut` to do the rewrite, then call `walk_expr_mut`:

```rust
struct Simplify;

impl VisitorMut for Simplify {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        walk_expr_mut(self, expr);  // simplify children first (post-order)
        if let ExprKind::BinOp { op, left, right } = &expr.kind {
            // existing rewrite rules for +0, *1, *0, -0
            // (copy verbatim from current simplify_algebraic body)
        }
    }
}

fn simplify_algebraic(expr: &mut Expr) {
    Simplify.visit_expr_mut(expr);
}
```

**Critical naming**: the `VisitorMut` trait method is `visit_expr_mut`, NOT `visit_expr`. Using the wrong name adds an inherent method that never gets called by `walk_expr_mut` — the default (empty) `visit_expr_mut` runs instead, silently producing a no-op walk.

- [ ] **Step 8: Migrate `bind_placeholder_idents`**

Immutable walk with a side channel (the `Resolver`):

```rust
struct BindPlaceholders<'a> {
    placeholders: &'a HashMap<String, SymbolId>,
    resolver: &'a mut Resolver,
}

impl<'a> Visitor for BindPlaceholders<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let ExprKind::Ident(name) = &expr.kind {
            if let Some(&symbol) = self.placeholders.get(name) {
                self.resolver.record_expr_symbol(expr.id, symbol);
            }
        }
        walk_expr(self, expr);
    }
}

fn bind_placeholder_idents(
    expr: &Expr,
    placeholders: &HashMap<String, SymbolId>,
    resolver: &mut Resolver,
) {
    BindPlaceholders { placeholders, resolver }.visit_expr(expr);
}
```

- [ ] **Step 9: Migrate `substitute_params`**

Has a mutable substitution map and pushes diagnostics. Pattern:

```rust
struct Substituter<'a> {
    substitutions: &'a HashMap<String, Substitution>,
    bag: &'a mut DiagnosticBag,
}

impl<'a> VisitorMut for Substituter<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        // handle ExprKind::Ident replacements inline
        if let ExprKind::Ident(name) = &expr.kind {
            if self.substitutions.contains_key(name) {
                // do the substitution (existing logic)
                return;  // don't recurse; new subtree replaces
            }
        }
        walk_expr_mut(self, expr);
    }

    fn visit_assign_target_mut(&mut self, target: &mut AssignTarget) {
        // AssignTarget::Ident replacements + diagnostics for Expr-as-target
        // (copy the AssignTarget logic from the current substitute_params here)
        hulk_hir::walk_assign_target_mut(self, target);
    }
}

fn substitute_params(expr: &mut Expr, substitutions: &HashMap<String, Substitution>, bag: &mut DiagnosticBag) {
    Substituter { substitutions, bag }.visit_expr_mut(expr);
}
```

**Watch out**: the original `substitute_params` handles `AssignTarget::Ident` specially (pushes a diagnostic if an Expr param is used as assignment target). Put that logic in `visit_assign_target_mut`, not in `visit_expr_mut` — the default `walk_expr_mut` routes assignment targets through `visit_assign_target_mut`, so overriding that method is the clean split.

- [ ] **Step 10: Migrate `MacroExpander::expand_expr_children`**

This one is not a free function — it lives on `MacroExpander`. **Critical cycle hazard**: `expand_expr` calls `expand_expr_children` internally, and `walk_expr_mut` dispatches through `visit_expr_mut`. If `visit_expr_mut` calls `expand_expr` again, you get infinite recursion: `walk → visit_expr_mut → expand_expr → expand_expr_children → walk → …`.

**Correct pattern**: `visit_expr_mut` is the **child walker** only; it should NOT call `expand_expr`. The driver code (`for function in &mut program.functions { expander.expand_expr(&mut function.body); }`) keeps calling `expand_expr`, and `expand_expr` internally calls `expand_expr_children` which delegates to `walk_expr_mut`.

```rust
impl<'a> VisitorMut for MacroExpander<'a> {
    // visit_expr_mut is used ONLY for traversal of children; it must NOT
    // re-enter expand_expr or we loop. Call expand_expr on each child
    // directly here.
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        self.expand_expr(expr);
    }
}

fn expand_expr_children(&mut self, expr: &mut Expr) {
    // walk_expr_mut calls self.visit_expr_mut on each child of `expr`.
    // It does NOT call visit_expr_mut on `expr` itself — that would loop.
    walk_expr_mut(self, expr);
}
```

Why this works: `walk_expr_mut(visitor, parent)` descends into each *child* of `parent` and calls `visitor.visit_expr_mut(child)`. It never calls `visit_expr_mut` on the parent. So `expand_expr(parent)` → `expand_expr_children(parent)` → `walk_expr_mut(self, parent)` → `self.visit_expr_mut(each_child)` → `self.expand_expr(each_child)` — one level of recursion per level of the AST, which is exactly what we want.

**Verify in the visitor source**: `crates/hulk-ast/src/visitor.rs` line 423 (`walk_expr_mut`). Confirm it calls `self.visit_expr_mut(child)` on children, not on the parent. If it does call on the parent, back out and keep the hand-written walker.

- [ ] **Step 11: Migrate `LocalSanitizer::visit_expr`**

This one has per-scope state. The visitor trait does not help here directly because `LocalSanitizer` already has scopes — it just re-implements the walk. Simplification:

```rust
impl<'a> VisitorMut for LocalSanitizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            // only keep variants that introduce scopes: Block, Let, For, VecGenerator, Lambda
            // plus Ident rename on lookup
            ExprKind::Ident(name) => {
                if let Some(renamed) = self.lookup(name) {
                    *name = renamed;
                }
            }
            ExprKind::Let { .. } | ExprKind::For { .. } | ExprKind::Block(_)
            | ExprKind::VecGenerator { .. } | ExprKind::Lambda { .. } => {
                // scope-specific handling (copy verbatim from current LocalSanitizer::visit_expr)
            }
            _ => walk_expr_mut(self, expr),
        }
    }

    fn visit_assign_target_mut(&mut self, target: &mut AssignTarget) {
        if let AssignTarget::Ident(name) = target {
            if let Some(renamed) = self.lookup(name) {
                *name = renamed;
            }
        }
        hulk_hir::walk_assign_target_mut(self, target);
    }
}
```

- [ ] **Step 12: Migrate test helpers `collect_identifiers` and `collect_ident_node_ids`**

In the `#[cfg(test)] mod tests` block:

```rust
struct CollectIdents<'a>(&'a mut Vec<String>);
impl<'a> Visitor for CollectIdents<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if let ExprKind::Ident(name) = &expr.kind {
            self.0.push(name.clone());
        }
        walk_expr(self, expr);
    }

    fn visit_assign_target(&mut self, target: &AssignTarget) {
        if let AssignTarget::Ident(name) = target {
            self.0.push(name.clone());
        }
        hulk_hir::walk_assign_target(self, target);
    }
}

fn collect_identifiers(expr: &Expr, out: &mut Vec<String>) {
    CollectIdents(out).visit_expr(expr);
}
```

Mirror for `collect_ident_node_ids`.

- [ ] **Step 13: Final test + clippy run**

Run: `cargo test -p hulk-macros && cargo clippy -p hulk-macros --all-targets -- -D warnings`
Expected: 11 passed. No warnings.

- [ ] **Step 14: Commit**

```bash
git add crates/hulk-macros/src/lib.rs
git commit -m "refactor(hulk-macros): migrate remaining walkers to VisitorMut/Visitor

Replaces hand-written match &expr.kind recursion in substitute_params,
bind_placeholder_idents, simplify_algebraic, expand_expr_children,
LocalSanitizer, and test helpers with impls of VisitorMut/Visitor from
hulk-ast. Line count drops from ~2800 to ~2100."
```

---

### Task 1.2: Migrate `hulk-desugar` walkers

**Files:**
- Modify: `crates/hulk-desugar/src/lib.rs`

**API mismatch warning**: the current `Desugarer::desugar_expr` has signature `fn desugar_expr(&mut self, expr: Expr) -> Expr` (takes by value, returns new Expr), defined at `crates/hulk-desugar/src/lib.rs:77`. It does NOT fit `VisitorMut::visit_expr_mut(&mut self, expr: &mut Expr)` without a structural rewrite.

**Decision tree for this task**:
- **Option A** (preferred): change `desugar_expr` to `fn desugar_expr(&mut self, expr: &mut Expr)` using `std::mem::replace` or `std::mem::take` (with a dummy expr) to swap children in place, then migrate to `VisitorMut`. Larger diff but unifies with the rest of the codebase.
- **Option B** (safer): skip the recursion migration — keep `desugar_expr(Expr) -> Expr` as-is. Only migrate the immutable walker `visit_max_node_id` to `Visitor`. Document in commit message.

This task's default path is **Option B**. Attempt Option A only if there is time and it does not disturb existing tests.

- [ ] **Step 1: Verify baseline green**

Run: `cargo test -p hulk-desugar && cargo clippy -p hulk-desugar --all-targets -- -D warnings`

- [ ] **Step 2: Migrate `visit_max_node_id` to `Visitor`** (same pattern as Task 1.1 Step 5)

- [ ] **Step 3: Decide on Option A vs B for `desugar_expr`**

Read `crates/hulk-desugar/src/lib.rs:77-600`. If the by-value signature is load-bearing (transforms construct brand-new trees and never mutate children in place), choose **Option B** and skip to Step 5.

- [ ] **Step 4 (Option A only): Rewrite `desugar_expr` to in-place form**

Change signature to `fn desugar_expr(&mut self, expr: &mut Expr)`. Use `std::mem::replace(expr, Expr::dummy(...))` where child trees are consumed to reconstruct, then assign the result back.

Only attempt this after reading the full current function body. If the conversion introduces more than ~50 lines of bookkeeping, abandon and fall back to Option B (revert the attempt).

- [ ] **Step 5: Tests and commit**

```bash
cargo test -p hulk-desugar
cargo clippy -p hulk-desugar --all-targets -- -D warnings
git add crates/hulk-desugar/src/lib.rs
# Option A chosen:
git commit -m "refactor(hulk-desugar): migrate walks to VisitorMut with in-place desugar_expr"
# Option B chosen (default):
git commit -m "refactor(hulk-desugar): migrate visit_max_node_id to Visitor (desugar_expr kept by-value)"
```

---

### Task 1.3: Evaluate `hulk-semantic::Resolver::resolve_expr` migration (optional)

**Files:**
- Potentially modify: `crates/hulk-semantic/src/lib.rs`

- [ ] **Step 1: Inspect `resolve_expr`**

Read `resolve_expr` in `crates/hulk-semantic/src/lib.rs`. If it threads per-scope state that does not fit cleanly into `VisitorMut` (e.g. pushing scopes around nested expressions), **skip this task** and document in the commit message or a code comment the reason.

- [ ] **Step 2: Decision**

- Skip → mark task 1.3 as completed with "skipped (documented)".
- Migrate → follow Task 1.1 pattern. Commit:

```bash
git commit -m "refactor(hulk-semantic): migrate resolve_expr to Visitor"
```

---

## Phase 2 — Module Splits

Each task moves code out of a single oversized `lib.rs` (or `decl.rs`) into submodules. Public API is unchanged (use `pub use` re-exports where needed). Tests unchanged.

### Task 2.1: Split `hulk-ast/src/visitor.rs`

**Files:**
- Create: `crates/hulk-ast/src/visitor/mod.rs`
- Create: `crates/hulk-ast/src/visitor/immut.rs`
- Create: `crates/hulk-ast/src/visitor/mutate.rs`
- Delete: `crates/hulk-ast/src/visitor.rs` (replace with directory)

- [ ] **Step 1: Create `crates/hulk-ast/src/visitor/mod.rs`**

```rust
//! AST visitor traits and walk functions.

mod immut;
mod mutate;

pub use immut::*;
pub use mutate::*;
```

- [ ] **Step 2: Move immutable visitor code to `visitor/immut.rs`**

Move `Visitor` trait (lines 8-56 of current visitor.rs) and all `walk_*` free functions (non-mut) to the new file. Keep `use super::*` or exact imports at the top of the new file.

- [ ] **Step 3: Move mutable visitor code to `visitor/mutate.rs`**

Move `VisitorMut` trait and all `walk_*_mut` free functions there.

- [ ] **Step 4: Delete the old `visitor.rs` file**

```bash
rm crates/hulk-ast/src/visitor.rs
```

`mod visitor;` in `crates/hulk-ast/src/lib.rs` will now resolve to the directory.

- [ ] **Step 5: Tests and commit**

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
git add crates/hulk-ast/src/visitor crates/hulk-ast/src/lib.rs
git rm crates/hulk-ast/src/visitor.rs
git commit -m "refactor(hulk-ast): split visitor.rs into immut.rs + mutate.rs submodules"
```

---

### Task 2.2: Split `hulk-lexer/src/lib.rs`

**Files:**
- Create: `crates/hulk-lexer/src/cursor.rs`
- Create: `crates/hulk-lexer/src/tokens/mod.rs`
- Create: `crates/hulk-lexer/src/tokens/numbers.rs`
- Create: `crates/hulk-lexer/src/tokens/strings.rs`
- Create: `crates/hulk-lexer/src/tokens/idents.rs`
- Create: `crates/hulk-lexer/src/tokens/operators.rs`
- Modify: `crates/hulk-lexer/src/lib.rs`

- [ ] **Step 1: Inspect `impl Lexer` block in current lib.rs**

Read `crates/hulk-lexer/src/lib.rs` lines 26-314 (the `impl Lexer<'a>` block). Identify method groups:
- cursor helpers: `peek`, `advance`, `pos`, `at_end`, `starts_with`
- number lexing: `lex_number`, `read_digits`
- string lexing: `lex_string`, `read_escape`
- identifier lexing: `lex_ident_or_keyword`, `is_ident_start`, `is_ident_cont`
- operator lexing: `lex_operator`, `single_char`

- [ ] **Step 2: Create `cursor.rs` with `impl Lexer` for cursor helpers**

```rust
use super::Lexer;

impl<'a> Lexer<'a> {
    pub(super) fn peek(&self) -> Option<char> { /* copy */ }
    // ... all cursor helpers
}
```

- [ ] **Step 3: Create `tokens/numbers.rs` with `impl Lexer` for number methods**

Same pattern. Use `pub(super)` visibility so the top-level `lex` function still sees them.

- [ ] **Step 4: Repeat for `tokens/strings.rs`, `tokens/idents.rs`, `tokens/operators.rs`**

- [ ] **Step 5: Create `tokens/mod.rs`**

```rust
mod numbers;
mod strings;
mod idents;
mod operators;
```

- [ ] **Step 6: Shrink `lib.rs` to just the `Lexer` struct, `pub fn lex`, and module declarations**

```rust
mod cursor;
mod tokens;

pub fn lex(...) -> Vec<SpannedToken> { /* unchanged */ }

struct Lexer<'a> { /* unchanged */ }

impl<'a> Lexer<'a> {
    fn new(...) -> Self { /* unchanged */ }
    fn lex(&mut self) -> Vec<SpannedToken> { /* unchanged */ }
}
```

The individual method impls live in the submodules.

- [ ] **Step 7: Tests and commit**

```bash
cargo test -p hulk-lexer
cargo clippy -p hulk-lexer --all-targets -- -D warnings
git add crates/hulk-lexer/src
git commit -m "refactor(hulk-lexer): split lib.rs into cursor + tokens/* submodules"
```

---

### Task 2.3: Split `hulk-parser/src/decl.rs`

**Files:**
- Create: `crates/hulk-parser/src/decl/mod.rs`
- Create: `crates/hulk-parser/src/decl/function.rs`
- Create: `crates/hulk-parser/src/decl/type_decl.rs`
- Create: `crates/hulk-parser/src/decl/protocol.rs`
- Create: `crates/hulk-parser/src/decl/macro_decl.rs`
- Delete: `crates/hulk-parser/src/decl.rs`

- [ ] **Step 1: Read current `decl.rs`, identify method groups**

All methods belong to one `impl Parser` block. Group by declaration type:
- `parse_function_decl` and helpers (params, return type, body) → `function.rs`
- `parse_type_decl`, member parsing, attribute/method parsing → `type_decl.rs`
- `parse_protocol_decl`, protocol method sig parsing → `protocol.rs`
- `parse_macro_decl`, macro param parsing → `macro_decl.rs`
- Free helper `_span_from_tokens` stays in `mod.rs`

- [ ] **Step 2-5: Create each submodule with `impl Parser` block carrying the methods**

Use `pub(super)` or `pub(crate)` to keep them callable from the parent module.

- [ ] **Step 6: Create `decl/mod.rs`**

```rust
mod function;
mod type_decl;
mod protocol;
mod macro_decl;

fn _span_from_tokens(...) -> Span { /* unchanged */ }
```

- [ ] **Step 7: Delete `decl.rs`**

- [ ] **Step 8: Tests and commit**

```bash
cargo test -p hulk-parser
cargo clippy -p hulk-parser --all-targets -- -D warnings
git add crates/hulk-parser/src/decl
git rm crates/hulk-parser/src/decl.rs
git commit -m "refactor(hulk-parser): split decl.rs into per-declaration submodules"
```

---

### Task 2.4: Split `hulk-types/src/lib.rs`

**Files:**
- Create: `crates/hulk-types/src/type_id.rs`
- Create: `crates/hulk-types/src/env.rs`
- Create: `crates/hulk-types/src/inferer.rs`
- Create: `crates/hulk-types/src/symbol_inferer.rs`
- Modify: `crates/hulk-types/src/lib.rs`

- [ ] **Step 1: Move `TypeId`, `TypeKind`, `BuiltinType` to `type_id.rs`**

- [ ] **Step 2: Move `TypeEnv` struct + impl to `env.rs`**

- [ ] **Step 3: Move `TypeInferer` struct + impl to `inferer.rs`**

- [ ] **Step 4: Move `SymbolInferer` struct + impl to `symbol_inferer.rs`**

- [ ] **Step 5: Shrink `lib.rs` to re-exports**

```rust
mod type_id;
mod env;
mod inferer;
mod symbol_inferer;

pub use type_id::*;
pub use env::*;
pub use inferer::*;
pub use symbol_inferer::*;

#[cfg(test)]
mod tests;
```

Move tests block to `crates/hulk-types/src/tests.rs` (single file is fine).

- [ ] **Step 6: Tests and commit**

```bash
cargo test -p hulk-types
cargo clippy -p hulk-types --all-targets -- -D warnings
git add crates/hulk-types/src
git commit -m "refactor(hulk-types): split lib.rs into type_id/env/inferer/symbol_inferer modules"
```

---

### Task 2.5: Split `hulk-semantic/src/lib.rs`

**Files:**
- Create: `crates/hulk-semantic/src/symbols.rs`
- Create: `crates/hulk-semantic/src/resolver/mod.rs`
- Create: `crates/hulk-semantic/src/resolver/names.rs`
- Create: `crates/hulk-semantic/src/resolver/protocols.rs`
- Create: `crates/hulk-semantic/src/resolver/inheritance.rs`
- Create: `crates/hulk-semantic/src/resolver/builtins.rs`
- Modify: `crates/hulk-semantic/src/lib.rs`

- [ ] **Step 1: Move symbol table types to `symbols.rs`**

`SymbolId`, `SymbolKind`, `Symbol`, `SymbolTable`, `Scope`.

- [ ] **Step 2: Create `resolver/mod.rs` with the `Resolver` struct**

Keep the struct definition + basic methods (`new`, `push_scope`, `pop_scope`, `define`, `lookup`, `table`, `diagnostics`, `expr_symbol`, `has_expr_symbol`, `record_expr_symbol`, `allocate_symbol`).

- [ ] **Step 3: Move name resolution methods to `resolver/names.rs`**

`resolve_program`, `resolve_expr`, `resolve_function_decl`, `resolve_type_decl`, `resolve_member`, `resolve_macro_decl`, `define_params`, `register_global_declarations`, `resolve_type_ann_option`, `resolve_type_ann`. Implemented as `impl Resolver { ... }` in this file.

- [ ] **Step 4: Move protocol handling to `resolver/protocols.rs`**

`register_protocol_details`, `collect_protocol_methods`, `type_conforms_protocol`, `is_protocol_symbol`, `validate_call_argument_protocol_conformance`.

- [ ] **Step 5: Move inheritance handling to `resolver/inheritance.rs`**

`type_parents`, `resolve_parent_spec`, `detect_inheritance_cycles`, `type_has_method`, `resolve_concrete_type_symbol`.

- [ ] **Step 6: Move builtin registration to `resolver/builtins.rs`**

`register_builtins`.

- [ ] **Step 7: Shrink `lib.rs` to re-exports**

```rust
mod symbols;
mod resolver;
mod validation;  // already exists as crates/hulk-semantic/src/validation.rs

pub use symbols::*;
pub use resolver::*;

#[cfg(test)]
mod tests;  // extract the inline `mod tests { ... }` (starts ~line 902, ~440 lines) into tests.rs
```

The current `crates/hulk-semantic/src/` contains only `lib.rs` and `validation.rs` — do NOT assume a `support` module exists. The inline `mod tests { ... }` in lib.rs at line 902 onward should be extracted into a sibling `tests.rs` to keep `lib.rs` small.

- [ ] **Step 8: Tests and commit**

```bash
cargo test -p hulk-semantic
cargo clippy -p hulk-semantic --all-targets -- -D warnings
git add crates/hulk-semantic/src
git commit -m "refactor(hulk-semantic): split lib.rs into symbols + resolver/{names,protocols,inheritance,builtins}"
```

---

### Task 2.6: Split `hulk-desugar/src/lib.rs`

**Files:**
- Create: `crates/hulk-desugar/src/transforms/mod.rs`
- Create: `crates/hulk-desugar/src/transforms/for_loop.rs`
- Create: `crates/hulk-desugar/src/transforms/string_concat.rs`
- Create: `crates/hulk-desugar/src/transforms/lambda.rs`
- Create: `crates/hulk-desugar/src/signatures.rs`
- Create: `crates/hulk-desugar/src/node_ids.rs`
- Modify: `crates/hulk-desugar/src/lib.rs`

- [ ] **Step 1: Inspect current `Desugarer` impl**

Group methods by transformation: `for_loop_to_while`, `concat_space_to_concat_space_concat`, `lambda_*`, plus helpers.

- [ ] **Step 2: Create `transforms/for_loop.rs`** with the For→While transformation methods in an `impl Desugarer`.

- [ ] **Step 3: Create `transforms/string_concat.rs`** for `@@` → `@ " " @`.

- [ ] **Step 4: Create `transforms/lambda.rs`** for lambda-related transforms.

- [ ] **Step 5: Create `signatures.rs`** with `FunctionSignature` and `collect_function_signatures`.

- [ ] **Step 6: Create `node_ids.rs`** with `max_node_id_in_program` and helpers (may already be VisitorMut from Task 1.2).

- [ ] **Step 7: Shrink `lib.rs` to `pub fn desugar` + `Desugarer` struct + module declarations.**

- [ ] **Step 8: Tests and commit**

```bash
cargo test -p hulk-desugar
cargo clippy -p hulk-desugar --all-targets -- -D warnings
git add crates/hulk-desugar/src
git commit -m "refactor(hulk-desugar): split Desugarer impl by transformation into transforms/*"
```

---

### Task 2.7: Split `hulk-macros/src/lib.rs`

**Files:**
- Create: `crates/hulk-macros/src/expander.rs`
- Create: `crates/hulk-macros/src/pattern.rs`
- Create: `crates/hulk-macros/src/substitution.rs`
- Create: `crates/hulk-macros/src/sanitize.rs`
- Create: `crates/hulk-macros/src/symbols.rs`
- Create: `crates/hulk-macros/src/node_ids.rs`
- Modify: `crates/hulk-macros/src/lib.rs`

- [ ] **Step 1: Move `MacroExpander` and `expand_macro_call` to `expander.rs`**

- [ ] **Step 2: Move `PatternExpr`, `MatchCase`, `match_pattern`, `parse_match_case`, `simplify_algebraic`, `same_literal`, `expr_conforms_type_name`, `is_number_expr`, `parse_binop_name` to `pattern.rs`**

- [ ] **Step 3: Move `Substitution` enum, `substitute_params`, `build_substitution`, `map_type_ann_to_type_id` to `substitution.rs`**

- [ ] **Step 4: Move `LocalSanitizer` and `sanitize_locals` to `sanitize.rs`**

- [ ] **Step 5: Move `bind_placeholder_idents`, `allocate_placeholder_symbol` to `symbols.rs`**

- [ ] **Step 6: Move `refresh_node_ids`, `max_node_id_in_program`, `visit_max_node_id` to `node_ids.rs`**

- [ ] **Step 7: Shrink `lib.rs` to:**

```rust
mod expander;
mod pattern;
mod substitution;
mod sanitize;
mod symbols;
mod node_ids;

pub use expander::expand_macros;

#[cfg(test)]
mod tests;  // or keep inline if it fits under 500
```

- [ ] **Step 8: Tests and commit**

```bash
cargo test -p hulk-macros
cargo clippy -p hulk-macros --all-targets -- -D warnings
git add crates/hulk-macros/src
git commit -m "refactor(hulk-macros): split lib.rs into expander/pattern/substitution/sanitize/symbols/node_ids modules"
```

---

## Phase 3 — Test File Splits

### Task 3.1: Split `hulk-ast/tests/coverage.rs`

**Files:**
- Modify: `crates/hulk-ast/tests/coverage.rs` (becomes entry point only)
- Create: `crates/hulk-ast/tests/coverage/mod.rs`
- Create: `crates/hulk-ast/tests/coverage/node_id.rs`
- Create: `crates/hulk-ast/tests/coverage/expr.rs`
- Create: `crates/hulk-ast/tests/coverage/control.rs`
- Create: `crates/hulk-ast/tests/coverage/type_ann.rs`
- Create: `crates/hulk-ast/tests/coverage/decl.rs`

- [ ] **Step 1: Create `tests/coverage/mod.rs` with shared helpers**

```rust
use hulk_ast::*;
use hulk_span::*;

pub(crate) fn fresh_span() -> Span { /* copy from current coverage.rs */ }
pub(crate) fn expr(kind: ExprKind, id: u32) -> Expr { /* copy */ }
pub(crate) fn num(n: f64, id: u32) -> Expr { /* copy */ }
pub(crate) fn ident(name: &str, id: u32) -> Expr { /* copy */ }

pub(crate) mod node_id;
pub(crate) mod expr;
pub(crate) mod control;
pub(crate) mod type_ann;
pub(crate) mod decl;
```

- [ ] **Step 2: Move tests by group (per spec section 3)**

Each new file has `use super::*;` at the top to get the helpers, and `use hulk_ast::*;` for the domain types.

- [ ] **Step 3: Replace `tests/coverage.rs` with an entry point that declares the module**

```rust
// tests/coverage.rs  —  entry point for submodule tests
mod coverage {
    mod node_id;
    mod expr;
    mod control;
    mod type_ann;
    mod decl;

    // shared helpers
    use hulk_ast::*;
    use hulk_span::*;

    pub(crate) fn fresh_span() -> Span { /* copy */ }
    pub(crate) fn expr(kind: ExprKind, id: u32) -> Expr { /* copy */ }
    pub(crate) fn num(n: f64, id: u32) -> Expr { /* copy */ }
    pub(crate) fn ident(name: &str, id: u32) -> Expr { /* copy */ }
}
```

Then each `tests/coverage/*.rs` uses `use super::*;` to access helpers.

**Watch out**: cargo integration test submodule wiring is finicky. If the first layout (`tests/coverage/mod.rs` as the helpers file) does not compile, fall back to the flat layout above (everything declared from `tests/coverage.rs`).

- [ ] **Step 4: Tests and commit**

```bash
cargo test -p hulk-ast
git add crates/hulk-ast/tests
git commit -m "refactor(hulk-ast): split coverage.rs into per-feature test submodules"
```

---

### Task 3.2: Split `hulk-parser/tests/declarations.rs`

Same pattern as Task 3.1. Target submodules: `let_decl`, `control`, `assign`, `access`, `ops`.

- [ ] **Step 1: Create submodule layout with shared helpers**

Helpers: `parse_ok`, `parse_with_errors`, `body`.

- [ ] **Step 2-3: Move tests and create entry point**

- [ ] **Step 4: Tests and commit**

```bash
cargo test -p hulk-parser
git add crates/hulk-parser/tests
git commit -m "refactor(hulk-parser): split declarations.rs into per-feature test submodules"
```

---

## Final Verification

- [ ] **Step 1: Full workspace check**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Both must be green.

- [ ] **Step 2: Confirm line count invariant**

```bash
find crates -name "*.rs" -not -path "*/target/*" -exec wc -l {} + | awk '$1 > 500 && $2 != "total"'
```

Expected output: empty (no files over 500).

- [ ] **Step 3: Confirm no Claude in commit trailers**

```bash
git log --format="%h %s%n%b" feature/Desugaring...HEAD~15 | grep -i "claude"
```

Expected: empty or only the pre-existing `.gitignore` subject commit (`0c3061b`) already discussed with the user.

- [ ] **Step 4: Summary commit (optional)**

Nothing to commit. Final status reported verbally.

---

## Rollback

Each task is one commit (sometimes two in Task 1.1 because of mid-migration checkpoint). To revert a single refactor:

```bash
git revert <sha>
```

If a commit exposes a latent bug (tests green locally but integration fails), revert the minimum necessary commits and address the root cause before continuing.

## Abandonment Conditions

- If `walk_expr_mut` does not cover an `ExprKind` variant the hand-written code handled, **do not force the migration**. Keep that walker hand-written, add a code comment explaining why, and move on. Extending `walk_expr_mut` is out of scope for this plan.
- If a Phase 2 split exposes circular dependencies that require API changes, **revert that task's commit** and leave the file monolithic. Document in the commit message.

## Success Criteria

- `find crates -name "*.rs" -exec wc -l {} + | awk '$1 > 500'` → empty.
- `cargo test --workspace` → all green.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean.
- No commit in the phase ranges contains `Co-Authored-By: Claude` or equivalent.
- No existing test's body was modified (moved verbatim is fine).
