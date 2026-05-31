use std::collections::HashMap;

use hulk_hir::visitor::walk_expr_mut;
use hulk_hir::{Expr, ExprKind, VisitorMut};

use crate::substitution::Substitution;

pub(crate) const CASE_LITERAL_INTRINSIC: &str = "__hulk_case_lit";
pub(crate) const CASE_VARIABLE_INTRINSIC: &str = "__hulk_case_var";
pub(crate) const CASE_BINOP_INTRINSIC: &str = "__hulk_case_binop";
pub(crate) const CASE_BINOP_RIGHT_LITERAL_INTRINSIC: &str = "__hulk_case_binop_right_lit";
pub(crate) const DEFAULT_CASE_INTRINSIC: &str = "__hulk_default";

pub(crate) enum MatchCase {
    Pattern { pattern: PatternExpr, body: Expr },
    Default(Expr),
}

pub(crate) enum PatternExpr {
    Literal(Expr),
    Variable {
        name: String,
        ty_name: String,
    },
    BinOp {
        op: hulk_hir::BinOpKind,
        left_name: String,
        left_ty: String,
        right_name: String,
        right_ty: String,
    },
    BinOpRightLiteral {
        op: hulk_hir::BinOpKind,
        left_name: String,
        left_ty: String,
        right_literal: Expr,
    },
}

pub(crate) fn parse_match_case(expr: &Expr) -> Option<MatchCase> {
    let ExprKind::Call { callee, args } = &expr.kind else {
        return None;
    };
    let ExprKind::Ident(case_kind) = &callee.kind else {
        return None;
    };

    match case_kind.as_str() {
        CASE_LITERAL_INTRINSIC => {
            if args.len() != 2 {
                return None;
            }
            Some(MatchCase::Pattern {
                pattern: PatternExpr::Literal(args[0].clone()),
                body: args[1].clone(),
            })
        }
        CASE_VARIABLE_INTRINSIC => {
            if args.len() != 3 {
                return None;
            }
            let ExprKind::Ident(var_name) = &args[0].kind else {
                return None;
            };
            let ExprKind::StringLit(ty_name) = &args[1].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::Variable {
                    name: var_name.clone(),
                    ty_name: ty_name.clone(),
                },
                body: args[2].clone(),
            })
        }
        CASE_BINOP_INTRINSIC => {
            if args.len() != 6 {
                return None;
            }

            let ExprKind::StringLit(op_name) = &args[0].kind else {
                return None;
            };
            let op = parse_binop_name(op_name)?;

            let ExprKind::Ident(left_name) = &args[1].kind else {
                return None;
            };
            let ExprKind::StringLit(left_ty) = &args[2].kind else {
                return None;
            };
            let ExprKind::Ident(right_name) = &args[3].kind else {
                return None;
            };
            let ExprKind::StringLit(right_ty) = &args[4].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::BinOp {
                    op,
                    left_name: left_name.clone(),
                    left_ty: left_ty.clone(),
                    right_name: right_name.clone(),
                    right_ty: right_ty.clone(),
                },
                body: args[5].clone(),
            })
        }
        CASE_BINOP_RIGHT_LITERAL_INTRINSIC => {
            if args.len() != 5 {
                return None;
            }

            let ExprKind::StringLit(op_name) = &args[0].kind else {
                return None;
            };
            let op = parse_binop_name(op_name)?;

            let ExprKind::Ident(left_name) = &args[1].kind else {
                return None;
            };
            let ExprKind::StringLit(left_ty) = &args[2].kind else {
                return None;
            };

            Some(MatchCase::Pattern {
                pattern: PatternExpr::BinOpRightLiteral {
                    op,
                    left_name: left_name.clone(),
                    left_ty: left_ty.clone(),
                    right_literal: args[3].clone(),
                },
                body: args[4].clone(),
            })
        }
        DEFAULT_CASE_INTRINSIC => {
            if args.len() != 1 {
                return None;
            }
            Some(MatchCase::Default(args[0].clone()))
        }
        _ => None,
    }
}

fn parse_binop_name(op_name: &str) -> Option<hulk_hir::BinOpKind> {
    match op_name {
        "+" => Some(hulk_hir::BinOpKind::Add),
        "-" => Some(hulk_hir::BinOpKind::Sub),
        "*" => Some(hulk_hir::BinOpKind::Mul),
        "\\" => Some(hulk_hir::BinOpKind::Div),
        "==" => Some(hulk_hir::BinOpKind::Eq),
        "!=" => Some(hulk_hir::BinOpKind::Ne),
        "<" => Some(hulk_hir::BinOpKind::Lt),
        "<=" => Some(hulk_hir::BinOpKind::Le),
        ">" => Some(hulk_hir::BinOpKind::Gt),
        ">=" => Some(hulk_hir::BinOpKind::Ge),
        _ => None,
    }
}

pub(crate) fn match_pattern(
    pattern: &PatternExpr,
    subject: &Expr,
) -> Option<HashMap<String, Substitution>> {
    match pattern {
        PatternExpr::Literal(expected) => {
            if same_literal(expected, subject) {
                Some(HashMap::new())
            } else {
                None
            }
        }
        PatternExpr::Variable { name, ty_name } => {
            if expr_conforms_type_name(subject, ty_name) {
                Some(HashMap::from([(
                    name.clone(),
                    Substitution::Expr(subject.clone()),
                )]))
            } else {
                None
            }
        }
        PatternExpr::BinOp {
            op,
            left_name,
            left_ty,
            right_name,
            right_ty,
        } => {
            let ExprKind::BinOp {
                op: subject_op,
                left,
                right,
            } = &subject.kind
            else {
                return None;
            };

            if subject_op != op {
                return None;
            }
            if !expr_conforms_type_name(left, left_ty) {
                return None;
            }
            if !expr_conforms_type_name(right, right_ty) {
                return None;
            }

            Some(HashMap::from([
                (left_name.clone(), Substitution::Expr((**left).clone())),
                (right_name.clone(), Substitution::Expr((**right).clone())),
            ]))
        }
        PatternExpr::BinOpRightLiteral {
            op,
            left_name,
            left_ty,
            right_literal,
        } => {
            let ExprKind::BinOp {
                op: subject_op,
                left,
                right,
            } = &subject.kind
            else {
                return None;
            };

            if subject_op != op {
                return None;
            }
            if !expr_conforms_type_name(left, left_ty) {
                return None;
            }
            if !same_literal(right_literal, right) {
                return None;
            }

            Some(HashMap::from([(
                left_name.clone(),
                Substitution::Expr((**left).clone()),
            )]))
        }
    }
}

fn same_literal(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::Number(a), ExprKind::Number(b)) => (a - b).abs() < f64::EPSILON,
        (ExprKind::StringLit(a), ExprKind::StringLit(b)) => a == b,
        (ExprKind::Bool(a), ExprKind::Bool(b)) => a == b,
        _ => false,
    }
}

struct Simplify;

impl VisitorMut for Simplify {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        walk_expr_mut(self, expr);

        loop {
            let ExprKind::BinOp { op, left, right } = &expr.kind else {
                return;
            };

            let replacement = match op {
                hulk_hir::BinOpKind::Add => match (&left.kind, &right.kind) {
                    (ExprKind::Number(value), _) if value.abs() < f64::EPSILON => {
                        Some((**right).clone())
                    }
                    (_, ExprKind::Number(value)) if value.abs() < f64::EPSILON => {
                        Some((**left).clone())
                    }
                    _ => None,
                },
                hulk_hir::BinOpKind::Sub => match &right.kind {
                    ExprKind::Number(value) if value.abs() < f64::EPSILON => Some((**left).clone()),
                    _ => None,
                },
                hulk_hir::BinOpKind::Mul => match (&left.kind, &right.kind) {
                    (ExprKind::Number(value), _) if (*value - 1.0).abs() < f64::EPSILON => {
                        Some((**right).clone())
                    }
                    (_, ExprKind::Number(value)) if (*value - 1.0).abs() < f64::EPSILON => {
                        Some((**left).clone())
                    }
                    (ExprKind::Number(value), _) if value.abs() < f64::EPSILON => {
                        Some(Expr::new(ExprKind::Number(0.0), expr.span.clone(), expr.id))
                    }
                    (_, ExprKind::Number(value)) if value.abs() < f64::EPSILON => {
                        Some(Expr::new(ExprKind::Number(0.0), expr.span.clone(), expr.id))
                    }
                    _ => None,
                },
                _ => None,
            };

            let Some(mut replacement) = replacement else {
                return;
            };

            replacement.span = expr.span.clone();
            replacement.id = expr.id;
            *expr = replacement;
        }
    }
}

pub(crate) fn simplify_algebraic(expr: &mut Expr) {
    Simplify.visit_expr_mut(expr);
}

fn expr_conforms_type_name(expr: &Expr, ty_name: &str) -> bool {
    match ty_name {
        "Object" => true,
        "Number" => is_number_expr(expr),
        "String" => matches!(expr.kind, ExprKind::StringLit(_)),
        "Boolean" => matches!(expr.kind, ExprKind::Bool(_)),
        _ => false,
    }
}

fn is_number_expr(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Number(_) => true,
        ExprKind::UnaryOp {
            op: hulk_hir::UnaryOpKind::Neg,
            expr,
        } => is_number_expr(expr),
        ExprKind::BinOp { left, right, .. } => is_number_expr(left) && is_number_expr(right),
        _ => false,
    }
}
