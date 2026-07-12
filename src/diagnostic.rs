#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn point(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos.saturating_add(1),
        }
    }

    pub fn join(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: impl Into<Option<Span>>) -> Self {
        Self {
            message: message.into(),
            span: span.into(),
        }
    }

    pub fn bare(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn render(&self, path: &str, src: &str) -> String {
        let Some(span) = self.span else {
            return self.message.clone();
        };
        let (line_no, col_no, line_start, line_end) = locate(src, span.start);
        let line = &src[line_start..line_end];
        let caret_start = span.start.saturating_sub(line_start);
        let caret_end = span.end.min(line_end).max(span.start + 1) - line_start;
        let caret_len = caret_end.saturating_sub(caret_start).max(1);
        format!(
            "{}\n  --> {}:{}:{}\n   |\n{:>2} | {}\n   | {}{}",
            self.message,
            path,
            line_no,
            col_no,
            line_no,
            line,
            " ".repeat(caret_start),
            "^".repeat(caret_len)
        )
    }
}

impl From<String> for Diagnostic {
    fn from(message: String) -> Self {
        Self::bare(message)
    }
}

impl From<&str> for Diagnostic {
    fn from(message: &str) -> Self {
        Self::bare(message)
    }
}

fn locate(src: &str, pos: usize) -> (usize, usize, usize, usize) {
    let pos = pos.min(src.len());
    let mut line_no = 1usize;
    let mut line_start = 0usize;
    for (idx, ch) in src.char_indices() {
        if idx >= pos {
            break;
        }
        if ch == '\n' {
            line_no += 1;
            line_start = idx + 1;
        }
    }

    let line_end = src[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(src.len());
    let col_no = src[line_start..pos].chars().count() + 1;
    (line_no, col_no, line_start, line_end)
}
