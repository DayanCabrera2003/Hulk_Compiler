use hulk_hir::{BinOpKind, UnaryOpKind};

/// Opaque handle for an IR temporary variable. Temporaries are created by the
/// lowerer and serve as the left-hand side of every instruction that produces a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempId(pub u32);

/// An operand in a BANNER instruction — either a temporary, a compile-time
/// constant, or a reference to a global symbol (function name, type name).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Temp(TempId),
    ConstNum(f64),
    ConstStr(String),
    ConstBool(bool),
    ConstNull,
    Global(String),
}

/// A single three-address instruction in the BANNER IR.
///
/// Every instruction that produces a value writes it to a fresh `dst` temporary.
/// Control-flow instructions (`Label`, `Jump`, `JumpIf`, `Return`) do not produce values.
/// GC instructions (`ShadowPush`, `ShadowPop`) manage the conservative shadow stack.
#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    Copy {
        dst: TempId,
        src: Value,
    },
    BinOp {
        dst: TempId,
        op: BinOpKind,
        left: Value,
        right: Value,
    },
    UnOp {
        dst: TempId,
        op: UnaryOpKind,
        operand: Value,
    },
    Call {
        dst: TempId,
        callee: Value,
        args: Vec<Value>,
    },
    MethodCall {
        dst: TempId,
        receiver: Value,
        method: String,
        args: Vec<Value>,
    },
    StaticCall {
        dst: TempId,
        type_name: String,
        method: String,
        args: Vec<Value>,
    },
    New {
        dst: TempId,
        type_name: String,
        args: Vec<Value>,
    },
    GetField {
        dst: TempId,
        object: Value,
        field: String,
    },
    SetField {
        object: Value,
        field: String,
        value: Value,
    },
    GetIndex {
        dst: TempId,
        target: Value,
        index: Value,
    },
    SetIndex {
        target: Value,
        index: Value,
        value: Value,
    },
    Label(String),
    Jump(String),
    JumpIf {
        condition: Value,
        label: String,
    },
    Return(Value),
    ShadowPush(Value),
    ShadowPop,
    // Reserved; the lowerer never emits this. Codegen may use it to decompose New into alloc + init.
    Alloc {
        dst: TempId,
        type_name: String,
    },
}

/// A function in the BANNER program, identified by a fully-qualified name.
///
/// For type methods the name is `"TypeName.method"`. For the constructor it is
/// `"TypeName.__init__"`. Standalone functions use their unqualified HULK name.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerFunction {
    pub name: String,
    pub params: Vec<TempId>,
    pub param_names: Vec<String>,
    pub body: Vec<Instr>,
}

/// LLVM-level kind of a struct field, used by the codegen to pick the right
/// alloca/load/store width. `pointer_map` retains only the boolean "needs GC
/// tracing?" answer; `field_kinds` carries the full distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Number — stored as 8-byte f64.
    Number,
    /// Boolean — stored as i1 (1-byte slot with the rest padded).
    Boolean,
    /// Reference (string, vector, user type, Object).
    Reference,
}

/// Describes a user-defined type as needed by the codegen and GC layers.
///
/// `pointer_map` has one entry per field; `true` means the field holds a
/// heap-allocated reference that the GC must trace. `field_kinds` mirrors
/// the same indices but carries the full LLVM kind (so Number and Boolean
/// are not collapsed into the same "not a pointer" bucket).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDescriptor {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<String>,
    pub pointer_map: Vec<bool>,
    pub field_kinds: Vec<FieldKind>,
    pub methods: Vec<BannerFunction>,
}

/// The complete output of the BANNER lowering stage.
///
/// Contains the ordered list of type descriptors, standalone functions, and
/// the implicit main function that holds the top-level expression sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerProgram {
    pub types: Vec<TypeDescriptor>,
    pub functions: Vec<BannerFunction>,
    pub main: BannerFunction,
}
