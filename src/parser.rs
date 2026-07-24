use crate::ast::{
    ArrayInit, ArrayLen, BinOp, Expr, Function, Param, Program, ReturnType, Stmt, UnOp,
};
use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Program, Diagnostic> {
        let mut functions = Vec::new();
        let mut stmts = Vec::new();
        while !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Fn) {
                functions.push(self.parse_function()?);
            } else {
                stmts.push(self.parse_stmt()?);
            }
        }
        Ok(Program { functions, stmts })
    }

    fn parse_function(&mut self) -> Result<Function, Diagnostic> {
        let start = self.expect(TokenKind::Fn)?.span;
        let (name, name_span) = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.match_kind(TokenKind::RParen) {
            loop {
                let (param_name, param_span) = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                self.expect(TokenKind::ByteTy)?;
                params.push(Param {
                    name: param_name,
                    name_span: param_span,
                });
                if self.match_kind(TokenKind::Comma) {
                    continue;
                }
                self.expect(TokenKind::RParen)?;
                break;
            }
        }
        let return_type = if self.match_kind(TokenKind::Arrow) {
            self.expect(TokenKind::ByteTy)?;
            ReturnType::Byte
        } else {
            ReturnType::Void
        };
        let (body, end) = self.parse_block_with_span()?;
        Ok(Function {
            name,
            name_span,
            params,
            return_type,
            body,
            span: start.join(end),
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        match self.peek() {
            TokenKind::Const => self.parse_const(),
            TokenKind::Let => self.parse_let(),
            TokenKind::Put => self.parse_put(),
            TokenKind::Puts => self.parse_puts(),
            TokenKind::Print => self.parse_print(false),
            TokenKind::Println => self.parse_print(true),
            TokenKind::Read => self.parse_read(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Continue => self.parse_continue(),
            TokenKind::LBrace => {
                let (stmts, span) = self.parse_block_with_span()?;
                Ok(Stmt::Block(stmts, span))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::For => self.parse_for(),
            TokenKind::Ident(_) if self.at_next(TokenKind::LParen) => self.parse_call_stmt(),
            TokenKind::Ident(_) => self.parse_assign(),
            kind => Err(self.error_here(format!("expected statement, found {kind:?}"))),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, Diagnostic> {
        self.parse_let_with_semi(true)
    }

    fn parse_const(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Const)?.span;
        let (name, name_span) = self.expect_ident()?;
        self.expect(TokenKind::Eq)?;
        let expr = self.parse_expr()?;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Const {
            name,
            name_span,
            expr,
            span: start.join(end),
        })
    }

    fn parse_let_with_semi(&mut self, semi: bool) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Let)?.span;
        let (name, name_span) = self.expect_ident()?;
        let typed = if self.match_kind(TokenKind::Colon) {
            self.expect(TokenKind::ByteTy)?;
            true
        } else {
            false
        };
        if typed && self.match_kind(TokenKind::LBracket) {
            let len_expr = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            let init = if self.match_kind(TokenKind::Eq) {
                Some(self.parse_array_initializer()?)
            } else {
                None
            };
            let end = if semi {
                self.expect(TokenKind::Semi)?.span
            } else {
                init.as_ref()
                    .map_or_else(|| len_expr.span(), ArrayInit::span)
            };
            return Ok(Stmt::LetArray {
                name,
                name_span,
                len: ArrayLen::Explicit(len_expr),
                init,
                span: start.join(end),
            });
        }
        let init = if self.match_kind(TokenKind::Eq) {
            if !typed
                && (self.at(TokenKind::LBracket) || matches!(self.peek(), TokenKind::StringLit(_)))
            {
                let init = self.parse_array_initializer()?;
                if matches!(&init, ArrayInit::Items(items) if items.is_empty()) {
                    return Err(Diagnostic::new(
                        "cannot infer the element type of an empty array",
                        init.span(),
                    ));
                }
                let len = init.len();
                let end = if semi {
                    self.expect(TokenKind::Semi)?.span
                } else {
                    init.span()
                };
                return Ok(Stmt::LetArray {
                    name,
                    name_span,
                    len: ArrayLen::Inferred(len),
                    init: Some(init),
                    span: start.join(end),
                });
            }
            Some(self.parse_expr()?)
        } else if typed {
            None
        } else {
            return Err(Diagnostic::new(
                "inferred byte declaration requires an initializer",
                name_span,
            ));
        };
        let end = if semi {
            self.expect(TokenKind::Semi)?.span
        } else {
            init.as_ref().map_or(name_span, Expr::span)
        };
        Ok(Stmt::Let {
            name,
            name_span,
            init,
            span: start.join(end),
        })
    }

    fn parse_array_initializer(&mut self) -> Result<ArrayInit, Diagnostic> {
        if let TokenKind::StringLit(_) = self.peek() {
            let token = self.advance_token().clone();
            let TokenKind::StringLit(bytes) = token.kind else {
                unreachable!();
            };
            return Ok(ArrayInit::Bytes(bytes, token.span));
        }

        self.expect(TokenKind::LBracket)?;
        let mut items = Vec::new();
        if self.match_kind(TokenKind::RBracket) {
            return Ok(ArrayInit::Items(items));
        }
        loop {
            items.push(self.parse_expr()?);
            if self.match_kind(TokenKind::Comma) {
                if self.match_kind(TokenKind::RBracket) {
                    break;
                }
            } else {
                self.expect(TokenKind::RBracket)?;
                break;
            }
        }
        Ok(ArrayInit::Items(items))
    }

    fn parse_assign(&mut self) -> Result<Stmt, Diagnostic> {
        self.parse_assign_with_semi(true)
    }

    fn parse_assign_with_semi(&mut self, semi: bool) -> Result<Stmt, Diagnostic> {
        let (name, name_span) = self.expect_ident()?;
        let array_index = if self.match_kind(TokenKind::LBracket) {
            let index = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            Some(index)
        } else {
            None
        };
        let compound = if self.match_kind(TokenKind::Eq) {
            None
        } else if self.match_kind(TokenKind::PlusEq) {
            Some(BinOp::Add)
        } else if self.match_kind(TokenKind::MinusEq) {
            Some(BinOp::Sub)
        } else if self.match_kind(TokenKind::StarEq) {
            Some(BinOp::Mul)
        } else if self.match_kind(TokenKind::SlashEq) {
            Some(BinOp::Div)
        } else if self.match_kind(TokenKind::PercentEq) {
            Some(BinOp::Mod)
        } else if self.match_kind(TokenKind::AmpEq) {
            Some(BinOp::BitAnd)
        } else if self.match_kind(TokenKind::PipeEq) {
            Some(BinOp::BitOr)
        } else if self.match_kind(TokenKind::CaretEq) {
            Some(BinOp::BitXor)
        } else if self.match_kind(TokenKind::ShlEq) {
            Some(BinOp::Shl)
        } else if self.match_kind(TokenKind::ShrEq) {
            Some(BinOp::Shr)
        } else {
            return Err(self.error_here(format!(
                "expected assignment operator, found {:?}",
                self.peek()
            )));
        };
        let rhs = self.parse_expr()?;
        let end = if semi {
            self.expect(TokenKind::Semi)?.span
        } else {
            rhs.span()
        };
        if let Some(index) = array_index {
            let expr = if let Some(op) = compound {
                let span = name_span.join(rhs.span());
                Expr::Binary {
                    left: Box::new(Expr::ArrayGet {
                        name: name.clone(),
                        name_span,
                        index: Box::new(index.clone()),
                        span: name_span.join(index.span()),
                    }),
                    op,
                    right: Box::new(rhs),
                    span,
                }
            } else {
                rhs
            };
            return Ok(Stmt::ArrayAssign {
                name,
                name_span,
                index,
                expr,
                span: name_span.join(end),
            });
        }
        let expr = if let Some(op) = compound {
            let span = name_span.join(rhs.span());
            Expr::Binary {
                left: Box::new(Expr::Var(name.clone(), name_span)),
                op,
                right: Box::new(rhs),
                span,
            }
        } else {
            rhs
        };
        Ok(Stmt::Assign {
            name,
            name_span,
            expr,
            span: name_span.join(end),
        })
    }

    fn parse_put(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Put)?.span;
        let expr = self.parse_expr()?;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Put(expr, start.join(end)))
    }

    fn parse_puts(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Puts)?.span;
        let token = self.advance_token().clone();
        let bytes = match token.kind {
            TokenKind::StringLit(bytes) => bytes,
            kind => {
                return Err(Diagnostic::new(
                    format!("expected string literal, found {kind:?}"),
                    token.span,
                ));
            }
        };
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Puts(bytes, start.join(end)))
    }

    fn parse_print(&mut self, newline: bool) -> Result<Stmt, Diagnostic> {
        let start = if newline {
            self.expect(TokenKind::Println)?.span
        } else {
            self.expect(TokenKind::Print)?.span
        };
        if !newline && matches!(self.peek(), TokenKind::StringLit(_)) {
            let token = self.advance_token().clone();
            let TokenKind::StringLit(bytes) = token.kind else {
                unreachable!("formatted print is selected by token kind");
            };
            let parts = parse_format_parts(&bytes, token.span)?;
            let mut args = Vec::new();
            while self.match_kind(TokenKind::Comma) {
                args.push(self.parse_expr()?);
            }
            let end = self.expect(TokenKind::Semi)?.span;
            let placeholders = parts.len() - 1;
            if placeholders != args.len() {
                return Err(Diagnostic::new(
                    format!(
                        "format string has {placeholders} placeholders, but {} arguments were provided",
                        args.len()
                    ),
                    token.span,
                ));
            }
            return Ok(Stmt::PrintFormat {
                parts,
                args,
                span: start.join(end),
            });
        }
        let expr = self.parse_expr()?;
        let end = self.expect(TokenKind::Semi)?.span;
        if newline {
            Ok(Stmt::Println(expr, start.join(end)))
        } else {
            Ok(Stmt::Print(expr, start.join(end)))
        }
    }

    fn parse_read(&mut self) -> Result<Stmt, Diagnostic> {
        self.expect(TokenKind::Read)?;
        let (name, name_span) = self.expect_ident()?;
        if self.match_kind(TokenKind::LBracket) {
            let index = self.parse_expr()?;
            let end = self.expect(TokenKind::RBracket)?.span;
            self.expect(TokenKind::Semi)?;
            Ok(Stmt::ReadArray {
                name,
                name_span,
                index,
                span: name_span.join(end),
            })
        } else {
            self.expect(TokenKind::Semi)?;
            Ok(Stmt::Read(name, name_span))
        }
    }

    fn parse_return(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Return)?.span;
        let expr = if self.at(TokenKind::Semi) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Return(expr, start.join(end)))
    }

    fn parse_call_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let expr = self.parse_primary()?;
        let end = self.expect(TokenKind::Semi)?.span;
        let Expr::Call {
            name,
            name_span,
            args,
            span,
        } = expr
        else {
            unreachable!("call statements are selected by lookahead");
        };
        Ok(Stmt::Call {
            name,
            name_span,
            args,
            span: span.join(end),
        })
    }

    fn parse_break(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Break)?.span;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Break(start.join(end)))
    }

    fn parse_continue(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Continue)?.span;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(Stmt::Continue(start.join(end)))
    }

    fn parse_if(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::If)?.span;
        let cond = self.parse_expr()?;
        let (then_branch, then_span) = self.parse_block_with_span()?;
        let (else_branch, end) = if self.match_kind(TokenKind::Else) {
            if self.at(TokenKind::If) {
                let stmt = self.parse_if()?;
                let span = stmt_span(&stmt);
                (vec![stmt], span)
            } else {
                self.parse_block_with_span()?
            }
        } else {
            (Vec::new(), then_span)
        };
        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
            span: start.join(end),
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::While)?.span;
        let cond = self.parse_expr()?;
        let (body, end) = self.parse_block_with_span()?;
        Ok(Stmt::While {
            cond,
            body,
            span: start.join(end),
        })
    }

    fn parse_loop(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::Loop)?.span;
        let (body, end) = self.parse_block_with_span()?;
        Ok(Stmt::Loop {
            body,
            span: start.join(end),
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, Diagnostic> {
        let start = self.expect(TokenKind::For)?.span;

        let init = if self.match_kind(TokenKind::Semi) {
            None
        } else {
            let init = match self.peek() {
                TokenKind::Let => self.parse_let_with_semi(false)?,
                TokenKind::Ident(_) => self.parse_assign_with_semi(false)?,
                kind => {
                    return Err(
                        self.error_here(format!("expected for initializer, found {kind:?}"))
                    );
                }
            };
            self.expect(TokenKind::Semi)?;
            Some(Box::new(init))
        };

        let cond = if self.match_kind(TokenKind::Semi) {
            None
        } else {
            let cond = self.parse_expr()?;
            self.expect(TokenKind::Semi)?;
            Some(cond)
        };

        let step = if self.at(TokenKind::LBrace) {
            None
        } else {
            let step = match self.peek() {
                TokenKind::Ident(_) => self.parse_assign_with_semi(false)?,
                kind => {
                    return Err(
                        self.error_here(format!("expected for step assignment, found {kind:?}"))
                    );
                }
            };
            Some(Box::new(step))
        };

        let (body, end) = self.parse_block_with_span()?;
        Ok(Stmt::For {
            init,
            cond,
            step,
            body,
            span: start.join(end),
        })
    }

    fn parse_block_with_span(&mut self) -> Result<(Vec<Stmt>, Span), Diagnostic> {
        let start = self.expect(TokenKind::LBrace)?.span;
        let mut stmts = Vec::new();
        while !self.at(TokenKind::RBrace) {
            if self.at(TokenKind::Eof) {
                return Err(Diagnostic::new("unterminated block", start));
            }
            stmts.push(self.parse_stmt()?);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok((stmts, start.join(end)))
    }

    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_and()?;
        while self.match_kind(TokenKind::OrOr) {
            let right = self.parse_and()?;
            expr = binary(expr, BinOp::Or, right);
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_bitor()?;
        while self.match_kind(TokenKind::AndAnd) {
            let right = self.parse_bitor()?;
            expr = binary(expr, BinOp::And, right);
        }
        Ok(expr)
    }

    fn parse_bitor(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_bitxor()?;
        while self.match_kind(TokenKind::Pipe) {
            let right = self.parse_bitxor()?;
            expr = binary(expr, BinOp::BitOr, right);
        }
        Ok(expr)
    }

    fn parse_bitxor(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_bitand()?;
        while self.match_kind(TokenKind::Caret) {
            let right = self.parse_bitand()?;
            expr = binary(expr, BinOp::BitXor, right);
        }
        Ok(expr)
    }

    fn parse_bitand(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_compare()?;
        while self.match_kind(TokenKind::Amp) {
            let right = self.parse_compare()?;
            expr = binary(expr, BinOp::BitAnd, right);
        }
        Ok(expr)
    }

    fn parse_compare(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_shift()?;
        loop {
            let op = if self.match_kind(TokenKind::EqEq) {
                Some(BinOp::Eq)
            } else if self.match_kind(TokenKind::BangEq) {
                Some(BinOp::Ne)
            } else if self.match_kind(TokenKind::Lt) {
                Some(BinOp::Lt)
            } else if self.match_kind(TokenKind::LtEq) {
                Some(BinOp::Le)
            } else if self.match_kind(TokenKind::Gt) {
                Some(BinOp::Gt)
            } else if self.match_kind(TokenKind::GtEq) {
                Some(BinOp::Ge)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_shift()?;
            expr = binary(expr, op, right);
        }
        Ok(expr)
    }

    fn parse_shift(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_add()?;
        loop {
            let op = if self.match_kind(TokenKind::Shl) {
                Some(BinOp::Shl)
            } else if self.match_kind(TokenKind::Shr) {
                Some(BinOp::Shr)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_add()?;
            expr = binary(expr, op, right);
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_mul()?;
        loop {
            let op = if self.match_kind(TokenKind::Plus) {
                Some(BinOp::Add)
            } else if self.match_kind(TokenKind::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_mul()?;
            expr = binary(expr, op, right);
        }
        Ok(expr)
    }

    fn parse_mul(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = if self.match_kind(TokenKind::Star) {
                Some(BinOp::Mul)
            } else if self.match_kind(TokenKind::Slash) {
                Some(BinOp::Div)
            } else if self.match_kind(TokenKind::Percent) {
                Some(BinOp::Mod)
            } else {
                None
            };
            let Some(op) = op else { break };
            let right = self.parse_unary()?;
            expr = binary(expr, op, right);
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        if let Some(op_span) = self.match_token(TokenKind::Bang) {
            let expr = self.parse_unary()?;
            let span = op_span.join(expr.span());
            return Ok(Expr::Unary {
                op: UnOp::Not,
                expr: Box::new(expr),
                span,
            });
        }
        if let Some(op_span) = self.match_token(TokenKind::Tilde) {
            let expr = self.parse_unary()?;
            let span = op_span.join(expr.span());
            return Ok(Expr::Unary {
                op: UnOp::BitNot,
                expr: Box::new(expr),
                span,
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.advance_token().clone();
        match token.kind {
            TokenKind::Number(n) => {
                let value = u8::try_from(n)
                    .map_err(|_| Diagnostic::new("byte literal out of range", token.span))?;
                Ok(Expr::Byte(value, token.span))
            }
            TokenKind::True => Ok(Expr::Byte(1, token.span)),
            TokenKind::False => Ok(Expr::Byte(0, token.span)),
            TokenKind::Ident(name) => {
                if self.match_kind(TokenKind::LParen) {
                    let mut args = Vec::new();
                    let end = if self.match_kind(TokenKind::RParen) {
                        token.span
                    } else {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.match_kind(TokenKind::Comma) {
                                continue;
                            }
                            break self.expect(TokenKind::RParen)?.span;
                        }
                    };
                    Ok(Expr::Call {
                        name,
                        name_span: token.span,
                        args,
                        span: token.span.join(end),
                    })
                } else if self.match_kind(TokenKind::LBracket) {
                    let index = self.parse_expr()?;
                    let end = self.expect(TokenKind::RBracket)?.span;
                    Ok(Expr::ArrayGet {
                        name,
                        name_span: token.span,
                        index: Box::new(index),
                        span: token.span.join(end),
                    })
                } else {
                    Ok(Expr::Var(name, token.span))
                }
            }
            TokenKind::LParen => {
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(expr)
            }
            kind => Err(Diagnostic::new(
                format!("expected expression, found {kind:?}"),
                token.span,
            )),
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let token = self.advance_token().clone();
        match token.kind {
            TokenKind::Ident(name) => Ok((name, token.span)),
            kind => Err(Diagnostic::new(
                format!("expected identifier, found {kind:?}"),
                token.span,
            )),
        }
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        self.match_token(kind).is_some()
    }

    fn match_token(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            Some(self.advance_token().span)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, Diagnostic> {
        if self.at(kind.clone()) {
            Ok(self.advance_token().clone())
        } else {
            Err(self.error_here(format!("expected {kind:?}, found {:?}", self.peek())))
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(&kind)
    }

    fn at_next(&self, kind: TokenKind) -> bool {
        self.tokens.get(self.current + 1).is_some_and(|token| {
            std::mem::discriminant(&token.kind) == std::mem::discriminant(&kind)
        })
    }

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.current].kind
    }

    fn advance_token(&mut self) -> &Token {
        let current = self.current;
        if self.current + 1 < self.tokens.len() {
            self.current += 1;
        }
        &self.tokens[current]
    }

    fn error_here(&self, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(message, self.tokens[self.current].span)
    }
}

fn binary(left: Expr, op: BinOp, right: Expr) -> Expr {
    let span = left.span().join(right.span());
    Expr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
        span,
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Const { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::LetArray { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::ArrayAssign { span, .. }
        | Stmt::Put(_, span)
        | Stmt::Puts(_, span)
        | Stmt::Print(_, span)
        | Stmt::PrintFormat { span, .. }
        | Stmt::Println(_, span)
        | Stmt::Read(_, span)
        | Stmt::ReadArray { span, .. }
        | Stmt::Call { span, .. }
        | Stmt::Return(_, span)
        | Stmt::Break(span)
        | Stmt::Continue(span)
        | Stmt::Block(_, span)
        | Stmt::If { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::For { span, .. } => *span,
    }
}

fn parse_format_parts(bytes: &[u8], span: Span) -> Result<Vec<Vec<u8>>, Diagnostic> {
    let mut parts = Vec::new();
    let mut literal = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => {
                literal.push(b'{');
                index += 2;
            }
            b'}' if bytes.get(index + 1) == Some(&b'}') => {
                literal.push(b'}');
                index += 2;
            }
            b'{' if bytes.get(index + 1) == Some(&b'}') => {
                parts.push(std::mem::take(&mut literal));
                index += 2;
            }
            b'{' | b'}' => {
                return Err(Diagnostic::new(
                    "unmatched brace in format string; use `{{`, `}}`, or `{}`",
                    span,
                ));
            }
            byte => {
                literal.push(byte);
                index += 1;
            }
        }
    }
    parts.push(literal);
    Ok(parts)
}
