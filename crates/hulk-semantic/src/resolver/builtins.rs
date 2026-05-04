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

        for name in ["PI", "E"] {
            self.define(name, SymbolKind::BuiltinValue, span.clone());
        }

        for name in ["Object", "Number", "String", "Boolean"] {
            self.define(name, SymbolKind::BuiltinType, span.clone());
        }
    }
}
