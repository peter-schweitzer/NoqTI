use std::fmt;
use std::collections::HashMap;

use super::lexer::*;
use super::diagnostics::*;

pub type Bindings = HashMap<String, Expr>;

macro_rules! fun_args {
    () => { vec![] };
    ($name:ident) => { vec![expr!($name)] };
    ($name:ident,$($rest:tt)*) => {
        {
            let mut t = vec![expr!($name)];
            t.append(&mut fun_args!($($rest)*));
            t
        }
    };
    ($name:ident($($args:tt)*)) => {
        vec![expr!($name($($args)*))]
    };
    ($name:ident($($args:tt)*),$($rest:tt)*) => {
        {
            let mut t = vec![expr!($name($($args)*))];
            t.append(&mut fun_args!($($rest)*));
            t
        }
    }
}

macro_rules! expr {
    ($name:ident) => {
        Expr::make_ident(stringify!($name), loc_here!())
    };
    ($name:ident($($args:tt)*)) => {
        Expr::Fun(Box::new(Expr::make_ident(stringify!($name), loc_here!())), fun_args!($($args)*))
    };
}

// TODO: unary minus
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Mod,
    Eql,
    And,
    Or,
    Xor,
    Not,
}

impl Op {
    fn from_token_kind(kind: TokenKind) -> Option<Self> {
        match kind {
            TokenKind::Plus => Some(Op::Add),
            TokenKind::Dash => Some(Op::Sub),
            TokenKind::Asterisk => Some(Op::Mul),
            TokenKind::Slash => Some(Op::Div),
            TokenKind::Caret => Some(Op::Pow),
            TokenKind::Percent => Some(Op::Mod),
            TokenKind::EqualsEquals => Some(Op::Eql),
            TokenKind::LogicalAnd => Some(Op::And),
            TokenKind::LogicalOr => Some(Op::Or),
            TokenKind::LogicalXor => Some(Op::Xor),
            TokenKind::LogicalNot => Some(Op::Not),
            _ => None
        }
    }

    pub fn precedence(&self) -> usize {
        use Op::*;
        match self {
            Eql             => 0,
            Add | Sub       => 1,
            Mul | Div | Mod => 2,
            Pow             => 3,
            And | Or | Xor  => 4,
            Not => 5, // unary expressions should have the highest precedence I guess :man_shrugging:
        }
    }

    const MAX_PRECEDENCE: usize = 4;
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Op::Eql => write!(f, "=="),
            Op::Add => write!(f, "+"),
            Op::Sub => write!(f, "-"),
            Op::Mul => write!(f, "*"),
            Op::Div => write!(f, "/"),
            Op::Mod => write!(f, "%"),
            Op::Pow => write!(f, "^"),
            Op::And => write!(f, "∧"),
            Op::Or => write!(f, "∨"),
            Op::Xor => write!(f, "⊕"),
            Op::Not => write!(f, "¬"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Sym(Token),
    Var(Token),
    Fun(Box<Expr>, Vec<Expr>),
    UniOp(Op, Box<Expr>),
    BinOp(Op, Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn replace_head() -> Expr {
        expr!(apply_rule(Strategy, Head, Body, Expr))
    }

    pub fn substitute(&mut self, bindings: &Bindings) {
        match self {
            Self::Sym(_) => {},

            Self::Var(name) => if let Some(value) = bindings.get(&name.text) {
                *self = value.clone()
            }

            Self::UniOp(_, rhs) => {
                rhs.substitute(bindings);
            }

            Self::BinOp(_, lhs, rhs) => {
                lhs.substitute(bindings);
                rhs.substitute(bindings);
            },

            Self::Fun(head, args) => {
                head.substitute(bindings);
                for arg in args {
                    arg.substitute(bindings)
                }
            }
        }
    }

    pub fn make_ident(name: &str, loc: Loc) -> Self {
        Self::parse_ident(Token {
            kind: TokenKind::Ident,
            text: name.to_string(),
            loc,
        })
    }

    pub fn parse_ident(token: Token) -> Self {
        assert!(token.kind == TokenKind::Ident);
        let x = token.text.chars().next().expect("Empty names are not allowed. This might be a bug in the lexer.");
        if x.is_uppercase() || x == '_' {
            Self::Var(token)
        } else {
            Self::Sym(token)
        }
    }


    pub fn human_name(&self) -> &'static str {
        match self {
            Self::Sym(_) => "a symbol",
            Self::Var(_) => "a variable",
            Self::Fun(_, _) => "a functor",
            Self::UniOp(_, _) => "a unary operator",
            Self::BinOp(_, _, _) => "a binary operator",
        }
    }

    fn parse_fun_args(lexer: &mut Lexer, diag: &mut impl Diagnoster) -> Option<Vec<Self>> {
        use TokenKind::*;
        let mut args = Vec::new();
        let open_paren_token = lexer.expect_token(OpenParen).map_err(|(expected_kind, actual_token)| {
            diag.report(&actual_token.loc, Severity::Error, &format!("Functor argument list must start with {}, but we got {} instead", expected_kind, actual_token.report()))
        }).ok()?;
        if lexer.peek_token().kind == CloseParen {
            lexer.next_token();
            return Some(args)
        }
        args.push(Self::parse(lexer, diag)?);
        while lexer.peek_token().kind == Comma {
            lexer.next_token();
            args.push(Self::parse(lexer, diag)?);
        }
        lexer.expect_token(CloseParen).map_err(|(expected_kind, actual_token)| {
            diag.report(&actual_token.loc, Severity::Error, &format!("Functor argument list must end with {}, but we got {} instead", expected_kind, actual_token.report()));
            diag.report(&open_paren_token.loc, Severity::Info, &format!("The corresponding {} is here.", open_paren_token.kind));
        }).ok()?;
        Some(args)
    }

    fn parse_unary_operation_or_primary(lexer: &mut Lexer, diag: &mut impl Diagnoster) -> Option<Self> {
        let mut head = {
            let token = lexer.next_token();
            match token.kind {
                TokenKind::OpenParen => {
                    let result = Self::parse(lexer, diag)?;
                    lexer.expect_token(TokenKind::CloseParen).map_err(|(expected_kind, actual_token)| {
                        diag.report(&actual_token.loc, Severity::Error, &format!("Expected {} at the end of the expression, but we got {} instead.", expected_kind, actual_token.report()));
                        diag.report(&token.loc, Severity::Info, &format!("The corresponding {} is here.", token.kind));
                    }).ok()?;
                    result
                }

                TokenKind::Ident => {
                    Self::parse_ident(token)
                },

                TokenKind::LogicalNot => {
                    return Some(Self::UniOp(Op::Not, Box::new(Self::parse_unary_operation_or_primary(lexer, diag)?)));
                },

                _ => {
                    diag.report(&token.loc, Severity::Error, &format!("Expected start of a primary expression. Primary expressions start with {} or {}.", TokenKind::Ident, TokenKind::OpenParen));
                    return None;
                }
            }
        };

        while lexer.peek_token().kind == TokenKind::OpenParen {
            head = Expr::Fun(Box::new(head), Self::parse_fun_args(lexer, diag)?)
        }
        Some(head)
    }

    fn parse_binary_operator(lexer: &mut Lexer, current_precedence: usize, diag: &mut impl Diagnoster) -> Option<Self> {
        if current_precedence > Op::MAX_PRECEDENCE {
            return Self::parse_unary_operation_or_primary(lexer, diag)
        }

        let mut result = Self::parse_binary_operator(lexer, current_precedence + 1, diag)?;

        while let Some(op) = Op::from_token_kind(lexer.peek_token().kind) {
            if current_precedence != op.precedence() {
                break
            }

            lexer.next_token();

            result = match op {
                Op::Not => Expr::UniOp(
                    op,
                    Box::new(Self::parse_binary_operator(lexer, current_precedence, diag)?),
                ),
                _ => Expr::BinOp(
                    op,
                    Box::new(result),
                    Box::new(Self::parse_binary_operator(lexer, current_precedence, diag)?)
                ),
            };
        }

        Some(result)
    }

    pub fn parse(lexer: &mut Lexer, diag: &mut impl Diagnoster) -> Option<Self> {
        Self::parse_binary_operator(lexer, 0, diag)
    }

    pub fn pattern_match(&self, value: &Expr) -> Option<Bindings> {
        fn pattern_match_impl(pattern: &Expr, value: &Expr, bindings: &mut Bindings) -> bool {
            use Expr::*;
            match (pattern, value) {
                (Sym(name1), Sym(name2)) => {
                    name1 == name2
                }
                (Var(name), _) => {
                    if name.text == "_" {
                        true
                    } else if let Some(bound_value) = bindings.get(&name.text) {
                        bound_value == value
                    } else {
                        bindings.insert(name.text.clone(), value.clone());
                        true
                    }
                }
                (UniOp(op1, rhs1), UniOp(op2, rhs2)) => {
                    *op1 == *op2 && pattern_match_impl(rhs1, rhs2, bindings)
                }
                (BinOp(op1, lhs1, rhs1), BinOp(op2, lhs2, rhs2)) => {
                    *op1 == *op2 && pattern_match_impl(lhs1, lhs2, bindings) && pattern_match_impl(rhs1, rhs2, bindings)
                }
                (Fun(name1, args1), Fun(name2, args2)) => {
                    if pattern_match_impl(name1, name2, bindings) && args1.len() == args2.len() {
                        for i in 0..args1.len() {
                            if !pattern_match_impl(&args1[i], &args2[i], bindings) {
                                return false;
                            }
                        }
                        true
                    } else {
                        false
                    }
                },
                _ => false,
            }
        }

        let mut bindings = HashMap::new();

        if pattern_match_impl(self, value, &mut bindings) {
            Some(bindings)
        } else {
            None
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Sym(name) | Expr::Var(name) => write!(f, "{}", name.text),
            Expr::Fun(head, args) => {
                match &**head {
                    Expr::Sym(name) | Expr::Var(name) => write!(f, "{}", name.text)?,
                    other => write!(f, "({})", other)?,
                }
                write!(f, "(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 { write!(f, ", ")? }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            },
            Expr::UniOp(op, rhs) => {
                if op.precedence() <= 1 {
                    write!(f, " {} ", op)?;
                } else {
                    write!(f, "{}", op)?;
                }
                match **rhs {
                    Expr::UniOp(sub_op, _) | Expr::BinOp(sub_op, _, _) => {
                        if sub_op.precedence() <= op.precedence() {
                            write!(f, "({})", rhs)
                        } else {
                            write!(f, "{}", rhs)
                        }
                    }
                    _ => write!(f, "{}", rhs)
                }
            }
            Expr::BinOp(op, lhs, rhs) => {
                match **lhs {
                    Expr::UniOp(sub_op, _) | Expr::BinOp(sub_op, _, _) => if sub_op.precedence() <= op.precedence() {
                        write!(f, "({})", lhs)?
                    } else {
                        write!(f, "{}", lhs)?
                    }
                    _ => write!(f, "{}", lhs)?
                }
                if op.precedence() <= 1 {
                    write!(f, " {} ", op)?;
                } else {
                    write!(f, "{}", op)?;
                }
                match **rhs {
                    Expr::UniOp(sub_op, _) | Expr::BinOp(sub_op, _, _) => {
                        if sub_op.precedence() <= op.precedence() {
                            write!(f, "({})", rhs)
                        } else {
                            write!(f, "{}", rhs)
                        }
                    }
                    _ => write!(f, "{}", rhs)
                }
            }
        }
    }
}

pub fn matches_at_least_one<'a>(pattern: &'a Expr, expr: &'a Expr) -> bool {
    if pattern.pattern_match(expr).is_some() {
        return true;
    }

    match expr {
        Expr::Fun(head, args) => {
            if matches_at_least_one(pattern, head) {
                return true;
            }
            for arg in args {
                if matches_at_least_one(pattern, arg) {
                    return true;
                }
            }
        }
        Expr::UniOp(_, rhs) => {
            if matches_at_least_one(pattern, rhs) {
                return true;
            }
        }
        Expr::BinOp(_, lhs, rhs) => {
            if matches_at_least_one(pattern, lhs) {
                return true;
            }
            if matches_at_least_one(pattern, rhs) {
                return true;
            }
        }
        Expr::Sym(_) | Expr::Var(_) => {},
    }
    false
}

pub fn find_all_subexprs<'a>(pattern: &'a Expr, expr: &'a Expr) -> Vec<&'a Expr> {
    let mut subexprs = Vec::new();

    fn find_all_subexprs_impl<'a>(pattern: &'a Expr, expr: &'a Expr, subexprs: &mut Vec<&'a Expr>) {
        if pattern.pattern_match(expr).is_some() {
            subexprs.push(expr);
        }

        match expr {
            Expr::Fun(head, args) => {
                find_all_subexprs_impl(pattern, head, subexprs);
                for arg in args {
                    find_all_subexprs_impl(pattern, arg, subexprs);
                }
            }
            Expr::UniOp(_, rhs) => {
                find_all_subexprs_impl(pattern, rhs, subexprs);
            }
            Expr::BinOp(_, lhs, rhs) => {
                find_all_subexprs_impl(pattern, lhs, subexprs);
                find_all_subexprs_impl(pattern, rhs, subexprs);
            }
            Expr::Sym(_) | Expr::Var(_) => {}
        }
    }

    find_all_subexprs_impl(pattern, expr, &mut subexprs);
    subexprs
}
