use std::sync::Arc;

use hulk_ast::{SourceFile, Span};

use crate::symbols::SymbolKind;
use crate::Resolver;

impl Resolver {
    pub(crate) fn register_builtins(&mut self) {
        let file = Arc::new(SourceFile::new("<builtins>", ""));
        let span = Span::dummy(file);

        for name in ["print", "sqrt", "sin", "cos", "exp", "log", "rand", "range"] {
            self.define(name, SymbolKind::BuiltinFunction, span.clone());
        }

        // Pattern-matching intrinsics produced by the parser for the
        // `match` / `case` / `default` syntax. They are not real functions:
        // the macro expander consumes them at expansion time. Registering
        // them as builtin functions keeps the resolver from rejecting them
        // and silences the type inferer's parameter checks.
        for name in [
            "__hulk_match",
            "__hulk_case_lit",
            "__hulk_case_var",
            "__hulk_case_binop",
            "__hulk_case_binop_right_lit",
            "__hulk_default",
        ] {
            self.define(name, SymbolKind::BuiltinFunction, span.clone());
        }

        for name in ["PI", "E"] {
            self.define(name, SymbolKind::BuiltinValue, span.clone());
        }

        for name in ["Object", "Number", "String", "Boolean"] {
            self.define(name, SymbolKind::BuiltinType, span.clone());
        }
    }
}
