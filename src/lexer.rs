use crate::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Let,
    Fn,
    Return,
    If,
    Else,
    While,
    Loop,
    For,
    Break,
    Continue,
    ByteTy,
    Put,
    Puts,
    Print,
    Println,
    Read,
    True,
    False,
    Ident(String),
    Number(u16),
    StringLit(Vec<u8>),
    Colon,
    Comma,
    Semi,
    Eq,
    PlusEq,
    MinusEq,
    Arrow,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    EqEq,
    Bang,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Eof,
}

pub fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        match ch {
            ' ' | '\t' | '\r' | '\n' => {}
            '/' if matches!(chars.peek(), Some((_, '/'))) => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::SlashEq, pos, pos + 2));
            }
            '/' => tokens.push(tok(TokenKind::Slash, pos, pos + 1)),
            ':' => tokens.push(tok(TokenKind::Colon, pos, pos + 1)),
            ',' => tokens.push(tok(TokenKind::Comma, pos, pos + 1)),
            ';' => tokens.push(tok(TokenKind::Semi, pos, pos + 1)),
            '=' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::EqEq, pos, pos + 2));
            }
            '=' => tokens.push(tok(TokenKind::Eq, pos, pos + 1)),
            '!' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::BangEq, pos, pos + 2));
            }
            '!' => tokens.push(tok(TokenKind::Bang, pos, pos + 1)),
            '<' if matches!(chars.peek(), Some((_, '<'))) => {
                chars.next();
                if matches!(chars.peek(), Some((_, '='))) {
                    chars.next();
                    tokens.push(tok(TokenKind::ShlEq, pos, pos + 3));
                } else {
                    tokens.push(tok(TokenKind::Shl, pos, pos + 2));
                }
            }
            '<' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::LtEq, pos, pos + 2));
            }
            '<' => tokens.push(tok(TokenKind::Lt, pos, pos + 1)),
            '>' if matches!(chars.peek(), Some((_, '>'))) => {
                chars.next();
                if matches!(chars.peek(), Some((_, '='))) {
                    chars.next();
                    tokens.push(tok(TokenKind::ShrEq, pos, pos + 3));
                } else {
                    tokens.push(tok(TokenKind::Shr, pos, pos + 2));
                }
            }
            '>' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::GtEq, pos, pos + 2));
            }
            '>' => tokens.push(tok(TokenKind::Gt, pos, pos + 1)),
            '&' if matches!(chars.peek(), Some((_, '&'))) => {
                chars.next();
                tokens.push(tok(TokenKind::AndAnd, pos, pos + 2));
            }
            '&' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::AmpEq, pos, pos + 2));
            }
            '&' => tokens.push(tok(TokenKind::Amp, pos, pos + 1)),
            '|' if matches!(chars.peek(), Some((_, '|'))) => {
                chars.next();
                tokens.push(tok(TokenKind::OrOr, pos, pos + 2));
            }
            '|' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::PipeEq, pos, pos + 2));
            }
            '|' => tokens.push(tok(TokenKind::Pipe, pos, pos + 1)),
            '^' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::CaretEq, pos, pos + 2));
            }
            '^' => tokens.push(tok(TokenKind::Caret, pos, pos + 1)),
            '~' => tokens.push(tok(TokenKind::Tilde, pos, pos + 1)),
            '+' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::PlusEq, pos, pos + 2));
            }
            '+' => tokens.push(tok(TokenKind::Plus, pos, pos + 1)),
            '-' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::MinusEq, pos, pos + 2));
            }
            '-' if matches!(chars.peek(), Some((_, '>'))) => {
                chars.next();
                tokens.push(tok(TokenKind::Arrow, pos, pos + 2));
            }
            '-' => tokens.push(tok(TokenKind::Minus, pos, pos + 1)),
            '*' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::StarEq, pos, pos + 2));
            }
            '*' => tokens.push(tok(TokenKind::Star, pos, pos + 1)),
            '%' if matches!(chars.peek(), Some((_, '='))) => {
                chars.next();
                tokens.push(tok(TokenKind::PercentEq, pos, pos + 2));
            }
            '%' => tokens.push(tok(TokenKind::Percent, pos, pos + 1)),
            '(' => tokens.push(tok(TokenKind::LParen, pos, pos + 1)),
            ')' => tokens.push(tok(TokenKind::RParen, pos, pos + 1)),
            '[' => tokens.push(tok(TokenKind::LBracket, pos, pos + 1)),
            ']' => tokens.push(tok(TokenKind::RBracket, pos, pos + 1)),
            '{' => tokens.push(tok(TokenKind::LBrace, pos, pos + 1)),
            '}' => tokens.push(tok(TokenKind::RBrace, pos, pos + 1)),
            '0'..='9' => tokens.push(read_number(src, &mut chars, pos, ch)?),
            '\'' => {
                let value = read_char_escape(&mut chars, pos)?;
                match chars.next() {
                    Some((end, '\'')) => {
                        tokens.push(tok(TokenKind::Number(value as u16), pos, end + 1))
                    }
                    _ => {
                        return Err(Diagnostic::new(
                            "unterminated character literal",
                            Span::point(pos),
                        ));
                    }
                }
            }
            '"' => {
                let mut bytes = Vec::new();
                let end = loop {
                    match chars.next() {
                        Some((end, '"')) => break end + 1,
                        Some((_, '\\')) => bytes.push(read_escape(&mut chars, pos)?),
                        Some((_, ch)) if ch.is_ascii() => bytes.push(ch as u8),
                        Some((p, _)) => {
                            return Err(Diagnostic::new("non-ASCII string byte", Span::point(p)));
                        }
                        None => {
                            return Err(Diagnostic::new(
                                "unterminated string literal",
                                Span::point(pos),
                            ));
                        }
                    }
                };
                tokens.push(tok(TokenKind::StringLit(bytes), pos, end));
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let mut text = String::from(ch);
                let mut end = pos + ch.len_utf8();
                while let Some((next_pos, next)) = chars.peek().copied() {
                    if next.is_ascii_alphanumeric() || next == '_' {
                        text.push(next);
                        end = next_pos + next.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                let kind = match text.as_str() {
                    "let" => TokenKind::Let,
                    "fn" => TokenKind::Fn,
                    "return" => TokenKind::Return,
                    "if" => TokenKind::If,
                    "else" => TokenKind::Else,
                    "while" => TokenKind::While,
                    "loop" => TokenKind::Loop,
                    "for" => TokenKind::For,
                    "break" => TokenKind::Break,
                    "continue" => TokenKind::Continue,
                    "byte" => TokenKind::ByteTy,
                    "put" => TokenKind::Put,
                    "puts" => TokenKind::Puts,
                    "print" => TokenKind::Print,
                    "println" => TokenKind::Println,
                    "read" => TokenKind::Read,
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    _ => TokenKind::Ident(text),
                };
                tokens.push(tok(kind, pos, end));
            }
            _ => {
                return Err(Diagnostic::new(
                    format!("unexpected character {ch:?}"),
                    Span::point(pos),
                ));
            }
        }
    }

    tokens.push(tok(TokenKind::Eof, src.len(), src.len()));
    Ok(tokens)
}

fn tok(kind: TokenKind, start: usize, end: usize) -> Token {
    Token {
        kind,
        span: Span::new(start, end),
    }
}

fn read_number<I>(
    src: &str,
    chars: &mut std::iter::Peekable<I>,
    pos: usize,
    first: char,
) -> Result<Token, Diagnostic>
where
    I: Iterator<Item = (usize, char)>,
{
    let mut text = String::from(first);
    let mut end = pos + 1;
    let radix = if first == '0' && matches!(chars.peek(), Some((_, 'b' | 'B'))) {
        let (_, prefix) = chars.next().expect("peeked prefix");
        text.push(prefix);
        end += 1;
        2
    } else if first == '0' && matches!(chars.peek(), Some((_, 'x' | 'X'))) {
        let (_, prefix) = chars.next().expect("peeked prefix");
        text.push(prefix);
        end += 1;
        16
    } else {
        10
    };

    while let Some((next_pos, next)) = chars.peek().copied() {
        let valid = match radix {
            2 => matches!(next, '0' | '1'),
            10 => next.is_ascii_digit(),
            16 => next.is_ascii_hexdigit(),
            _ => unreachable!(),
        };
        if valid {
            text.push(next);
            end = next_pos + next.len_utf8();
            chars.next();
        } else {
            break;
        }
    }

    let digits = match radix {
        2 | 16 => &text[2..],
        10 => text.as_str(),
        _ => unreachable!(),
    };
    if digits.is_empty() {
        return Err(Diagnostic::new(
            "expected digits after numeric literal prefix",
            Span::new(pos, end),
        ));
    }

    if let Some((bad_pos, bad)) = chars.peek().copied() {
        if bad.is_ascii_alphanumeric() || bad == '_' {
            return Err(Diagnostic::new(
                format!("invalid digit {bad:?} for base {radix} literal"),
                Span::point(bad_pos),
            ));
        }
    }

    let value = u16::from_str_radix(digits, radix).map_err(|_| {
        Diagnostic::new(
            "numeric literal is too large",
            Span::new(
                pos,
                src[pos..]
                    .find(|c: char| !c.is_ascii_alphanumeric())
                    .map_or(src.len(), |n| pos + n),
            ),
        )
    })?;
    Ok(tok(TokenKind::Number(value), pos, end))
}

fn read_char_escape<I>(chars: &mut std::iter::Peekable<I>, pos: usize) -> Result<u8, Diagnostic>
where
    I: Iterator<Item = (usize, char)>,
{
    match chars.next() {
        Some((_, '\\')) => read_escape(chars, pos),
        Some((_, ch)) if ch.is_ascii() => Ok(ch as u8),
        Some((p, _)) => Err(Diagnostic::new(
            "non-ASCII character literal",
            Span::point(p),
        )),
        None => Err(Diagnostic::new(
            "unterminated character literal",
            Span::point(pos),
        )),
    }
}

fn read_escape<I>(chars: &mut std::iter::Peekable<I>, pos: usize) -> Result<u8, Diagnostic>
where
    I: Iterator<Item = (usize, char)>,
{
    match chars.next() {
        Some((_, 'n')) => Ok(b'\n'),
        Some((_, 'r')) => Ok(b'\r'),
        Some((_, 't')) => Ok(b'\t'),
        Some((_, '0')) => Ok(0),
        Some((_, '\\')) => Ok(b'\\'),
        Some((_, '\'')) => Ok(b'\''),
        Some((_, '"')) => Ok(b'"'),
        Some((p, ch)) => Err(Diagnostic::new(
            format!("unknown escape \\{ch}"),
            Span::point(p),
        )),
        None => Err(Diagnostic::new("unterminated escape", Span::point(pos))),
    }
}
