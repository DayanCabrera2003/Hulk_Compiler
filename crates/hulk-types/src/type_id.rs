/// Stable, opaque identifier for a type in the program.
/// Reserved IDs for builtins: Object=0, Number=1, String=2, Boolean=3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeId(pub(crate) u32);

impl TypeId {
    pub const OBJECT: TypeId = TypeId(0);
    pub const NUMBER: TypeId = TypeId(1);
    pub const STRING: TypeId = TypeId(2);
    pub const BOOLEAN: TypeId = TypeId(3);

    /// Returns the raw numeric value for diagnostics/debugging.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    #[must_use]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// The different kinds of types in HULK.
#[derive(Debug, Clone)]
pub enum TypeKind {
    /// Builtin type (Object, Number, String, Boolean).
    Builtin(BuiltinType),
    /// User-defined type (class).
    UserDefined {
        name: String,
        parent: Option<TypeId>,
    },
    /// Protocol type.
    Protocol { name: String },
    /// Iterable type: T*.
    Iterable(TypeId),
    /// Vector type: T[].
    Vector(TypeId),
    /// Function type: (A, B, ...) -> ReturnType.
    Functor {
        params: Vec<TypeId>,
        ret: Box<TypeId>,
    },
    /// Unknown type (inference error or TBD).
    Unknown,
}

/// Builtin type categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinType {
    /// The top type, parent of all types.
    Object,
    /// Numeric literal type.
    Number,
    /// String literal type.
    String,
    /// Boolean literal type.
    Boolean,
}
