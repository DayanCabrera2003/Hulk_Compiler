use hulk_hir::{BinOpKind, UnaryOpKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Temp(TempId),
    ConstNum(f64),
    ConstStr(String),
    ConstBool(bool),
    ConstNull,
    Global(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instr {
    Copy       { dst: TempId, src: Value },
    BinOp      { dst: TempId, op: BinOpKind, left: Value, right: Value },
    UnOp       { dst: TempId, op: UnaryOpKind, operand: Value },
    Call       { dst: TempId, callee: Value, args: Vec<Value> },
    MethodCall { dst: TempId, receiver: Value, method: String, args: Vec<Value> },
    StaticCall { dst: TempId, type_name: String, method: String, args: Vec<Value> },
    New        { dst: TempId, type_name: String, args: Vec<Value> },
    GetField   { dst: TempId, object: Value, field: String },
    SetField   { object: Value, field: String, value: Value },
    GetIndex   { dst: TempId, target: Value, index: Value },
    SetIndex   { target: Value, index: Value, value: Value },
    Label      (String),
    Jump       (String),
    JumpIf     { condition: Value, label: String },
    Return     (Value),
    ShadowPush (Value),
    ShadowPop,
    Alloc      { dst: TempId, type_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BannerFunction {
    pub name: String,
    pub params: Vec<TempId>,
    pub param_names: Vec<String>,
    pub body: Vec<Instr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDescriptor {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<String>,
    pub pointer_map: Vec<bool>,
    pub methods: Vec<BannerFunction>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BannerProgram {
    pub types: Vec<TypeDescriptor>,
    pub functions: Vec<BannerFunction>,
    pub main: BannerFunction,
}
