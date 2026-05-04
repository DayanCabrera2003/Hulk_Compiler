use crate::ir::{BannerFunction, BannerProgram, Instr, TempId, TypeDescriptor, Value};
use hulk_hir::{BinOpKind, UnaryOpKind};
use std::fmt;

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

impl fmt::Display for TempId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "t{}", self.0)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Temp(t) => write!(f, "{t}"),
            Value::ConstNum(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Value::ConstStr(s) => write!(f, "\"{}\"", escape_str(s)),
            Value::ConstBool(b) => write!(f, "{b}"),
            Value::ConstNull => write!(f, "null"),
            Value::Global(name) => write!(f, "{name}"),
        }
    }
}

fn fmt_args(args: &[Value]) -> String {
    args.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn binop_sym(op: &BinOpKind) -> &'static str {
    use BinOpKind::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Mod => "%",
        Pow => "**",
        Eq => "==",
        Ne => "!=",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        And => "&",
        Or => "|",
        Concat => "@",
        ConcatSpaced => {
            unreachable!("ConcatSpaced should have been desugared before BANNER lowering")
        }
    }
}

fn unop_sym(op: &UnaryOpKind) -> &'static str {
    use UnaryOpKind::*;
    match op {
        Neg => "-",
        Not => "not",
    }
}

fn fmt_instr(instr: &Instr) -> String {
    match instr {
        Instr::Copy { dst, src } => format!("    {dst} = copy {src}"),
        Instr::BinOp {
            dst,
            op,
            left,
            right,
        } => {
            format!("    {dst} = {left} {} {right}", binop_sym(op))
        }
        Instr::UnOp { dst, op, operand } => {
            format!("    {dst} = {} {operand}", unop_sym(op))
        }
        Instr::Call { dst, callee, args } => {
            format!("    {dst} = call {callee}({})", fmt_args(args))
        }
        Instr::MethodCall {
            dst,
            receiver,
            method,
            args,
        } => {
            format!("    {dst} = {receiver}.{method}({})", fmt_args(args))
        }
        Instr::StaticCall {
            dst,
            type_name,
            method,
            args,
        } => {
            format!(
                "    {dst} = static {type_name}.{method}({})",
                fmt_args(args)
            )
        }
        Instr::New {
            dst,
            type_name,
            args,
        } => {
            format!("    {dst} = new {type_name}({})", fmt_args(args))
        }
        Instr::GetField { dst, object, field } => {
            format!("    {dst} = {object}.{field}")
        }
        Instr::SetField {
            object,
            field,
            value,
        } => {
            format!("    setfield {object}.{field} = {value}")
        }
        Instr::GetIndex { dst, target, index } => {
            format!("    {dst} = {target}[{index}]")
        }
        Instr::SetIndex {
            target,
            index,
            value,
        } => {
            format!("    {target}[{index}] = {value}")
        }
        Instr::Label(name) => format!("  {name}:"),
        Instr::Jump(label) => format!("    jump {label}"),
        Instr::JumpIf { condition, label } => format!("    jumpif {condition} {label}"),
        Instr::Return(v) => format!("    return {v}"),
        Instr::ShadowPush(v) => format!("    shadowpush {v}"),
        Instr::ShadowPop => "    shadowpop".to_string(),
        Instr::Alloc { dst, type_name } => format!("    {dst} = alloc {type_name}"),
    }
}

fn fmt_params(params: &[TempId], names: &[String]) -> String {
    params
        .iter()
        .zip(names.iter())
        .map(|(t, n)| format!("{t} /* {n} */"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for BannerFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "fn {}({}) {{",
            self.name,
            fmt_params(&self.params, &self.param_names)
        )?;
        for instr in &self.body {
            writeln!(f, "{}", fmt_instr(instr))?;
        }
        write!(f, "}}")
    }
}

impl fmt::Display for TypeDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "type {} {{", self.name)?;
        let parent = self.parent.as_deref().unwrap_or("none");
        writeln!(f, "  parent: {parent}")?;
        let fields_str: Vec<String> = self
            .fields
            .iter()
            .zip(self.pointer_map.iter())
            .map(|(name, is_ptr)| format!("{name} ({})", if *is_ptr { "ptr" } else { "val" }))
            .collect();
        writeln!(f, "  fields: [{}]", fields_str.join(", "))?;
        for method in &self.methods {
            for line in method.to_string().lines() {
                writeln!(f, "  {line}")?;
            }
        }
        write!(f, "}}")
    }
}

impl fmt::Display for BannerProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for td in &self.types {
            writeln!(f, "{td}")?;
            writeln!(f)?;
        }
        for func in &self.functions {
            writeln!(f, "{func}")?;
            writeln!(f)?;
        }
        write!(f, "{}", self.main)
    }
}
