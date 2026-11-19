use crate::diagnostic::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub functions: Vec<Function>,
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub return_type: ReturnType,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Byte,
    Int,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntLiteralKind {
    Decimal,
    BitPattern,
}

impl ValueType {
    pub const fn cell_width(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Int | Self::Fixed => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnType {
    Void,
    Byte,
    Int,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSpec {
    Decimal,
    Byte,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
    pub value_type: ValueType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Const {
        name: String,
        name_span: Span,
        expr: Expr,
        span: Span,
    },
    Let {
        name: String,
        name_span: Span,
        value_type: Option<ValueType>,
        init: Option<Expr>,
        span: Span,
    },
    LetArray {
        name: String,
        name_span: Span,
        value_type: Option<ValueType>,
        len: ArrayLen,
        second_len: Option<ArrayLen>,
        init: Option<ArrayInit>,
        span: Span,
    },
    Assign {
        name: String,
        name_span: Span,
        expr: Expr,
        span: Span,
    },
    ArrayAssign {
        name: String,
        name_span: Span,
        index: Expr,
        second: Option<Expr>,
        expr: Expr,
        span: Span,
    },
    Put(Expr, Span),
    Puts(Vec<u8>, Span),
    Print(Expr, Span),
    PrintFormat {
        parts: Vec<Vec<u8>>,
        specs: Vec<FormatSpec>,
        args: Vec<Expr>,
        span: Span,
    },
    Println(Expr, Span),
    Read(String, Span),
    ReadArray {
        name: String,
        name_span: Span,
        index: Expr,
        second: Option<Expr>,
        span: Span,
    },
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
    Return(Option<Expr>, Span),
    Break(Span),
    Continue(Span),
    Block(Vec<Stmt>, Span),
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Loop {
        body: Vec<Stmt>,
        span: Span,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Vec<Stmt>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayLen {
    Explicit(Expr),
    Inferred(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayInit {
    Items(Vec<Expr>),
    Rows(Vec<ArrayInit>),
    Bytes(Vec<u8>, Span),
}

impl ArrayInit {
    pub fn len(&self) -> usize {
        match self {
            ArrayInit::Items(items) => items.len(),
            ArrayInit::Rows(rows) => rows.iter().map(ArrayInit::len).sum(),
            ArrayInit::Bytes(bytes, _) => bytes.len(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            ArrayInit::Items(items) => items
                .last()
                .map(Expr::span)
                .unwrap_or_else(|| Span::point(0)),
            ArrayInit::Rows(rows) => rows
                .last()
                .map(ArrayInit::span)
                .unwrap_or_else(|| Span::point(0)),
            ArrayInit::Bytes(_, span) => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Byte(u8, Span),
    Int(u32, IntLiteralKind, Span),
    Fixed(u32, Span),
    Var(String, Span),
    ArrayGet {
        name: String,
        name_span: Span,
        index: Box<Expr>,
        second: Option<Box<Expr>>,
        span: Span,
    },
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
    Input {
        parts: Vec<Vec<u8>>,
        specs: Vec<FormatSpec>,
        targets: Vec<InputTarget>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },
    Cast {
        expr: Box<Expr>,
        target: ValueType,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Byte(_, span)
            | Expr::Int(_, _, span)
            | Expr::Fixed(_, span)
            | Expr::Var(_, span) => *span,
            Expr::ArrayGet { span, .. } | Expr::Call { span, .. } | Expr::Input { span, .. } => {
                *span
            }
            Expr::Unary { span, .. } | Expr::Binary { span, .. } | Expr::Cast { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputTarget {
    Scalar {
        name: String,
        name_span: Span,
    },
    Array {
        name: String,
        name_span: Span,
        index: Box<Expr>,
        second: Option<Box<Expr>>,
        span: Span,
    },
}

impl InputTarget {
    pub fn span(&self) -> Span {
        match self {
            Self::Scalar { name_span, .. } => *name_span,
            Self::Array { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    BitNot,
    Neg,
}
