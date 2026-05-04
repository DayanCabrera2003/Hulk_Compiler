# Split Large Files — Design

**Date**: 2026-04-23
**Status**: Approved, pending implementation plan
**Branch**: `feature/Desugaring`

## Problem

Nine Rust source files in the workspace exceed 500 lines. Several are well beyond that threshold and show symptoms of missing abstraction (duplicated AST walkers) or mixed concerns (one `impl` block covering multiple unrelated responsibilities).

| Lines | File | Nature |
|-------:|------|--------|
| 2793 | `crates/hulk-macros/src/lib.rs` | Production + tests (13 hand-written ExprKind walkers) |
| 1410 | `crates/hulk-ast/tests/coverage.rs` | Tests |
| 1344 | `crates/hulk-semantic/src/lib.rs` | Production + tests (single 900-line `impl Resolver`) |
| 1242 | `crates/hulk-desugar/src/lib.rs` | Production + tests (541-line `impl Desugarer`) |
| 1226 | `crates/hulk-parser/tests/declarations.rs` | Tests |
| 874  | `crates/hulk-types/src/lib.rs` | Production (TypeInferer ~350 lines) |
| 695  | `crates/hulk-ast/src/visitor.rs` | Production (Visitor + VisitorMut traits + walk fns) |
| 577  | `crates/hulk-lexer/src/lib.rs` | Production (single `impl Lexer`) |
| 505  | `crates/hulk-parser/src/decl.rs` | Production (single `impl Parser`) |

**Impact of current state**:
- Adding a new `ExprKind` variant requires touching ~7 locations in `hulk-macros` alone; the compiler does not uniformly catch missed updates because each walker uses its own exhaustive match.
- Large `impl` blocks mix responsibilities that should be separately readable and testable (name resolution vs. inheritance vs. protocols in `hulk-semantic`).
- Files over 1000 lines do not fit in a single reviewer's working memory, increasing the cost of future changes.

## Goal

Reduce every `.rs` file to below 500 lines by:
1. Replacing hand-rolled AST walkers with the existing `hulk-ast::Visitor`/`VisitorMut` trait.
2. Splitting large modules by responsibility into submodules.
3. Splitting large test files by feature.

All existing tests must continue to pass without modification at every commit.

## Non-Goals

- Introducing `derive` macros or procedural macros for visitor generation.
- Changing public APIs of any crate.
- Refactoring logic unrelated to the readability/scalability problems above.
- Performance changes.

## Existing Asset: `hulk-ast/src/visitor.rs`

The crate already ships a complete visitor:

```rust
pub trait Visitor { /* default methods delegate to walk_* */ }
pub trait VisitorMut { /* default methods delegate to walk_*_mut */ }

pub fn walk_expr<V: Visitor + ?Sized>(visitor: &mut V, expr: &Expr) { /* ... */ }
pub fn walk_expr_mut<V: VisitorMut + ?Sized>(visitor: &mut V, expr: &mut Expr) { /* ... */ }
// plus walk_program, walk_function_decl, walk_type_decl, walk_member, etc.
```

It is currently only consumed by parser integration tests. `hulk-macros`, `hulk-desugar`, and `hulk-semantic` each re-implemented their own hand-written walks. Migrating those crates to use the existing visitor is the primary lever for eliminating duplication.

## Strategy — Three Phases

Execution order: Phase 1 → Phase 2 → Phase 3. Each phase produces one or more self-contained commits that can be reverted independently.

### Phase 1 — Visitor Migration

Replace hand-rolled `match &expr.kind` walks with `impl VisitorMut for X` (or `impl Visitor for X`) using the existing trait.

**Target walkers** in `hulk-macros`:
- `MacroExpander::expand_expr_children`
- `substitute_params`
- `bind_placeholder_idents`
- `refresh_node_ids`
- `visit_max_node_id`
- `simplify_algebraic`
- `LocalSanitizer::visit_expr`
- Test helpers `collect_identifiers`, `collect_ident_node_ids`

**Target walkers** in `hulk-desugar`:
- `Desugarer` recursion over `Expr`
- `visit_max_node_id`

**`hulk-semantic::Resolver::resolve_expr`**: **optional**. Keep hand-written if migration increases complexity (the resolver threads substantial state through scopes and diagnostics; forcing it into a visitor may hurt readability more than it helps).

**Migration pattern**:
```rust
struct Sanitizer<'a> { /* state */ }

impl<'a> VisitorMut for Sanitizer<'a> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        // specific logic that runs before/instead of the default walk
        match &mut expr.kind {
            ExprKind::Let { .. } => { /* scope-specific handling */ }
            _ => walk_expr_mut(self, expr),
        }
    }
}
```

Specific logic lives in `visit_expr`/`visit_*`; recursion delegates to `walk_expr_mut`.

**Commits**: 2-3 (one per crate, `hulk-macros` first).

**Expected line reduction**: `hulk-macros` drops ~600 lines; `hulk-desugar` drops ~150.

### Phase 2 — Module Splits

Split large modules into submodules grouped by responsibility. Pure code movement plus `use` and visibility adjustments. No behavior change.

#### `hulk-macros/src/`

```
lib.rs          re-exports + pub fn expand_macros
expander.rs     MacroExpander struct, expand_macro_call core
pattern.rs      PatternExpr, MatchCase, match_pattern, parse_match_case, simplify_algebraic
substitution.rs Substitution enum, substitute_params, build_substitution
sanitize.rs     LocalSanitizer
symbols.rs      bind_placeholder_idents, allocate_placeholder_symbol
node_ids.rs     refresh_node_ids, max_node_id_in_program, visit_max_node_id
```

#### `hulk-semantic/src/`

```
lib.rs                    public API + Resolver struct
symbols.rs                SymbolId, SymbolKind, Symbol, SymbolTable, Scope
resolver/mod.rs           Resolver methods glue
resolver/names.rs         resolve_expr, define_params, lookup
resolver/protocols.rs     register_protocol_details, protocol_methods, type_conforms_protocol
resolver/inheritance.rs   type_parents, detect_inheritance_cycles, resolve_parent_spec
resolver/builtins.rs      register_builtins
validation.rs             (already extracted)
```

#### `hulk-desugar/src/`

```
lib.rs                       pub fn desugar + Desugarer core
transforms/for_loop.rs       For → While transformation
transforms/string_concat.rs  @@ → @ " " @
transforms/lambda.rs         Lambda desugar
signatures.rs                FunctionSignature, collect_function_signatures
node_ids.rs                  visit_max_node_id
```

#### `hulk-types/src/`

```
lib.rs            re-exports
type_id.rs        TypeId, TypeKind, BuiltinType
env.rs            TypeEnv
inferer.rs        TypeInferer (still the largest module at ~350 lines)
symbol_inferer.rs SymbolInferer
```

#### `hulk-lexer/src/`

```
lib.rs              pub fn lex + Lexer struct
cursor.rs           position, peek, advance, read helpers
tokens/numbers.rs   number lexing
tokens/strings.rs   string lexing + escape handling
tokens/idents.rs    identifiers and keywords
tokens/operators.rs operators and symbols
```

#### `hulk-parser/src/decl.rs`

```
decl/mod.rs        re-exports
decl/function.rs   parse_function_decl
decl/type_decl.rs  parse_type_decl + members
decl/protocol.rs   parse_protocol_decl
decl/macro_decl.rs parse_macro_decl
```

#### `hulk-ast/src/visitor.rs`

Borderline at 695 lines. Split for consistency:

```
visitor/mod.rs    re-exports
visitor/immut.rs  Visitor trait + walk_* free functions
visitor/mutate.rs VisitorMut trait + walk_*_mut free functions
```

**Commits**: 7 (one per crate).

### Phase 3 — Test File Splits

Move tests into submodules grouped by feature. Test helpers (fresh_span, parse_ok, etc.) move to the submodule's `mod.rs`.

#### `hulk-ast/tests/coverage.rs`

```
coverage/mod.rs       helpers (fresh_span, expr, num, ident)
coverage/node_id.rs   NodeIdGen tests (lines 47-117)
coverage/expr.rs      literals, binop, unaryop, call, method, field, index, block, vec, let
coverage/control.rs   if, while, for, assign
coverage/type_ann.rs  TypeAnn, Functor
coverage/decl.rs      FunctionDecl, TypeDecl, Member, attributes
```

#### `hulk-parser/tests/declarations.rs`

```
declarations/mod.rs       helpers (parse_ok, parse_with_errors, body)
declarations/let_decl.rs  let_* tests
declarations/control.rs   if_*, while_*, for_* tests
declarations/assign.rs    assign_* tests
declarations/access.rs    field_access, method_call, index_* tests
declarations/ops.rs       is, as, new, base_call
```

Each submodule holds 5-15 related tests.

**Rust integration test wiring**: `cargo` picks up each `tests/<name>.rs` as a separate integration test crate. To split one into submodules, keep a top-level `tests/coverage.rs` (or `tests/declarations.rs`) as the entry point and declare submodules from it:

```rust
// tests/coverage.rs
mod coverage {
    mod node_id;
    mod expr;
    // ...
}
```

with the submodule files at `tests/coverage/node_id.rs`, etc. Helpers live in `tests/coverage/mod.rs` or `tests/coverage/helpers.rs`.

**Commits**: 2 (one per test file).

## Behavior Preservation — TDD Strategy

**Invariant**: `cargo test --workspace` passes at the end of every commit. No test is modified unless the refactor intentionally changes observable behavior (it shouldn't).

**Per-commit workflow**:
1. Make the change.
2. `cargo test --workspace` → all green.
3. `cargo clippy --workspace --all-targets -- -D warnings` → clean.
4. Commit.

**Phase 1 (visitor migration)**: if grep reveals a walker has no edge-case tests, write 1-2 characterization tests before migrating. Existing tests must pass unchanged after migration.

**Phase 2 (module splits)**: pure code motion plus `use`/`pub(crate)` adjustments. No TDD step (no behavior change).

**Phase 3 (test splits)**: test functions move verbatim from one file to another. Helpers move to submodule `mod.rs`.

## Execution Order

1. **Baseline commit** — commit the pending Session 10 fixes so the starting point is green.
2. **Phase 1** — 2-3 commits: `hulk-macros` visitor, `hulk-desugar` visitor, (optional) `hulk-semantic` visitor.
3. **Phase 2** — 7 commits: `hulk-ast/visitor`, `hulk-lexer`, `hulk-parser/decl`, `hulk-types`, `hulk-semantic`, `hulk-desugar`, `hulk-macros`.
4. **Phase 3** — 2 commits: `coverage.rs` split, `declarations.rs` split.

**Total**: 12-13 commits on branch `feature/Desugaring`.

## Rollback

Each commit is self-contained. `git revert <sha>` for any single commit leaves the workspace compilable and tested.

## Abandonment Criterion

If a Phase 1 walker migration increases complexity — e.g. a walker needs state the visitor trait does not expose cleanly — leave that walker hand-written and document the reason in a code comment (`// Visitor migration abandoned: threads per-scope state`). Do not force the abstraction.

## Success Criteria

- Every `.rs` file in `crates/` has ≤ 500 lines (production or test file).
- No existing test was modified.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass at the final commit of every phase.
- Each phase is a coherent commit sequence that can be reviewed on its own.

## Out of Scope

- Performance tuning.
- Public API changes.
- New features or bug fixes beyond what the refactor inherently produces.
- Introducing new dependencies.
- Promoting `ExprKind::Match` to a real AST node (documented in `doc/seccion-10-macros.md` as a separate deferral).
