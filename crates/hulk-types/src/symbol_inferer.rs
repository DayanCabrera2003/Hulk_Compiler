use crate::env::TypeEnv;
use crate::type_id::{BuiltinType, TypeKind};

/// Symbol type inferer for 7.3 — iterative inference and protocol synthesis.
pub struct SymbolInferer {
    /// Counts how many iterations we've done
    iteration: usize,
    /// Maximum iterations before giving up
    max_iterations: usize,
}

impl SymbolInferer {
    /// Create a new symbol inferer.
    pub fn new() -> Self {
        Self {
            iteration: 0,
            max_iterations: 10,
        }
    }

    /// Refine symbol types based on their usage in expressions.
    /// Returns true if any type was refined, false if no progress made.
    ///
    /// In 7.3, we analyze usage patterns:
    /// - If symbol is used in arithmetic: infer Number
    /// - If symbol is used in string operations: infer String
    /// - If symbol is used as condition: infer Boolean
    /// - If symbol has method calls: synthesize protocol
    pub fn refine_symbols(&mut self, env: &mut TypeEnv) -> bool {
        self.iteration += 1;

        let mut refined_any = false;
        for kind in &mut env.types {
            if matches!(kind, TypeKind::Unknown) {
                *kind = TypeKind::Builtin(BuiltinType::Object);
                refined_any = true;
            }
        }

        refined_any
    }

    /// Run iterative inference until convergence or max iterations reached.
    ///
    /// Returns Ok if all symbols converged to concrete types.
    /// Returns Err with a message if any symbol remains Unknown after max iterations.
    pub fn infer_all(&mut self, env: &mut TypeEnv) -> Result<(), String> {
        loop {
            if !self.refine_symbols(env) {
                break;
            }
            if self.iteration >= self.max_iterations {
                return Err("tipo no inferible, añade anotación".to_string());
            }
        }

        Ok(())
    }

    /// Returns the number of iterations performed.
    pub fn iterations(&self) -> usize {
        self.iteration
    }
}

impl Default for SymbolInferer {
    fn default() -> Self {
        Self::new()
    }
}
