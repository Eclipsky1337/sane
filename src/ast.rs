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
pub enum ReturnType {
    Void,
    Byte,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub name_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        name: String,
        name_span: Span,
        init: Option<Expr>,
        span: Span,
    },
    LetArray {
        name: String,
        name_span: Span,
        len: usize,
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
        expr: Expr,
        span: Span,
    },
    Put(Expr, Span),
    Puts(Vec<u8>, Span),
    Print(Expr, Span),
    Println(Expr, Span),
    Read(String, Span),
    ReadArray {
        name: String,
        name_span: Span,
        index: Expr,
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
pub enum ArrayInit {
    Items(Vec<Expr>),
    Bytes(Vec<u8>, Span),
}

impl ArrayInit {
    pub fn span(&self) -> Span {
        match self {
            ArrayInit::Items(items) => items
                .last()
                .map(Expr::span)
                .unwrap_or_else(|| Span::point(0)),
            ArrayInit::Bytes(_, span) => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Byte(u8, Span),
    Var(String, Span),
    ArrayGet {
        name: String,
        name_span: Span,
        index: Box<Expr>,
        span: Span,
    },
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
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
            Expr::Byte(_, span) | Expr::Var(_, span) => *span,
            Expr::ArrayGet { span, .. } | Expr::Call { span, .. } => *span,
            Expr::Unary { span, .. } | Expr::Binary { span, .. } => *span,
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
}
