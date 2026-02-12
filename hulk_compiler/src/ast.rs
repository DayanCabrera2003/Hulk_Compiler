// src/ast.rs

#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl Expr {
    pub fn eval(&self) -> f64 {
        match self {
            Expr::Number(n) => *n,
            Expr::Binary(left, op, right) => {
                let l = left.eval();
                let r = right.eval();
                match op {
                    BinaryOp::Add => l + r,
                    BinaryOp::Sub => l - r,
                    BinaryOp::Mul => l * r,
                    BinaryOp::Div => l / r,
                }
            }
        }
    }
}