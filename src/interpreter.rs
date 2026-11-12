use std::io::{Read, Write};

const TAPE_SIZE: usize = 30_000;
pub const DISPATCH_MARKER: &str = "SANE PROFILE DISPATCH";
pub(crate) const ARRAY_GET_BF: &str = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<";
pub(crate) const ARRAY_SET_BF: &str =
    ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Add(i16),
    Move(isize),
    Input,
    Output,
    Clear,
    Loop(Vec<Op>),
    Dispatch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    pub executed_ops: u64,
    pub dispatches: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    ops: Vec<Op>,
}

impl Program {
    pub fn parse(src: &str) -> Result<Self, String> {
        let expanded = expand_repeated_commands(src)?;
        let mut parser = Parser {
            chars: parse_chars(&expanded),
            pos: 0,
        };
        let ops = parser.parse_ops(false)?;
        if parser.pos != parser.chars.len() {
            return Err("internal error: parser did not consume input".to_string());
        }
        Ok(Self { ops })
    }

    pub fn run<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), String> {
        self.run_with_stats(input, output).map(|_| ())
    }

    pub fn run_with_stats<R: Read, W: Write>(
        &self,
        input: R,
        output: W,
    ) -> Result<ExecutionStats, String> {
        let mut runtime = Runtime {
            tape: vec![0; TAPE_SIZE],
            ptr: 0,
            input,
            output,
            stats: ExecutionStats::default(),
        };
        runtime.run_ops(&self.ops)?;
        Ok(runtime.stats)
    }

    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub fn operation_count(&self) -> usize {
        count_ops(&self.ops)
    }
}

pub fn run_source<R: Read, W: Write>(src: &str, input: R, output: W) -> Result<(), String> {
    Program::parse(src)?.run(input, output)
}

pub fn expand_repeated_commands(src: &str) -> Result<String, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut expanded = String::with_capacity(src.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_ascii_digit() {
            let mut count: usize = 0;
            while index < chars.len() && chars[index].is_ascii_digit() {
                let digit = usize::from(chars[index] as u8 - b'0');
                count = count
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(digit))
                    .ok_or_else(|| "repeat count is too large".to_string())?;
                index += 1;
            }
            if index < chars.len() && matches!(chars[index], '+' | '-' | '<' | '>') {
                let command = chars[index];
                index += 1;
                expanded.extend(std::iter::repeat(command).take(count));
            }
            continue;
        }
        expanded.push(chars[index]);
        index += 1;
    }
    Ok(expanded)
}

fn is_bf(ch: char) -> bool {
    matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']')
}

fn parse_chars(src: &str) -> Vec<char> {
    let mut chars = Vec::new();
    let mut remaining = src;
    while let Some(marker_start) = remaining.find(DISPATCH_MARKER) {
        chars.extend(remaining[..marker_start].chars().filter(|ch| is_bf(*ch)));
        chars.push('\0');
        remaining = &remaining[marker_start + DISPATCH_MARKER.len()..];
    }
    chars.extend(remaining.chars().filter(|ch| is_bf(*ch)));
    chars
}

fn count_ops(ops: &[Op]) -> usize {
    ops.iter()
        .map(|op| match op {
            Op::Loop(body) => 1 + count_ops(body),
            _ => 1,
        })
        .sum()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn parse_ops(&mut self, in_loop: bool) -> Result<Vec<Op>, String> {
        let mut ops = Vec::new();
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '+' | '-' => self.parse_add(&mut ops),
                '>' | '<' => self.parse_move(&mut ops),
                '.' => {
                    self.pos += 1;
                    ops.push(Op::Output);
                }
                ',' => {
                    self.pos += 1;
                    ops.push(Op::Input);
                }
                '[' => {
                    self.pos += 1;
                    let body = self.parse_ops(true)?;
                    ops.push(optimize_loop(body));
                }
                ']' if in_loop => {
                    self.pos += 1;
                    return Ok(ops);
                }
                ']' => return Err(format!("unmatched `]` at instruction {}", self.pos)),
                '\0' => {
                    self.pos += 1;
                    ops.push(Op::Dispatch);
                }
                _ => unreachable!("non-BF characters are filtered before parsing"),
            }
        }

        if in_loop {
            Err("unmatched `[`".to_string())
        } else {
            Ok(ops)
        }
    }

    fn parse_add(&mut self, ops: &mut Vec<Op>) {
        let mut delta = 0i16;
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '+' => delta += 1,
                '-' => delta -= 1,
                _ => break,
            }
            self.pos += 1;
        }
        push_add(ops, delta);
    }

    fn parse_move(&mut self, ops: &mut Vec<Op>) {
        let mut delta = 0isize;
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '>' => delta += 1,
                '<' => delta -= 1,
                _ => break,
            }
            self.pos += 1;
        }
        push_move(ops, delta);
    }
}

fn push_add(ops: &mut Vec<Op>, delta: i16) {
    if delta == 0 {
        return;
    }
    if let Some(Op::Add(prev)) = ops.last_mut() {
        *prev += delta;
        if *prev == 0 {
            ops.pop();
        }
    } else {
        ops.push(Op::Add(delta));
    }
}

fn push_move(ops: &mut Vec<Op>, delta: isize) {
    if delta == 0 {
        return;
    }
    if let Some(Op::Move(prev)) = ops.last_mut() {
        *prev += delta;
        if *prev == 0 {
            ops.pop();
        }
    } else {
        ops.push(Op::Move(delta));
    }
}

fn optimize_loop(body: Vec<Op>) -> Op {
    if matches!(body.as_slice(), [Op::Add(-1)] | [Op::Add(1)]) {
        return Op::Clear;
    }
    Op::Loop(body)
}

struct Runtime<R, W> {
    tape: Vec<u8>,
    ptr: usize,
    input: R,
    output: W,
    stats: ExecutionStats,
}

impl<R: Read, W: Write> Runtime<R, W> {
    fn run_ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            self.stats.executed_ops += 1;
            match op {
                Op::Add(delta) => {
                    self.tape[self.ptr] = add_wrapping(self.tape[self.ptr], *delta);
                }
                Op::Move(delta) => {
                    self.ptr = checked_index(self.ptr, *delta)?;
                }
                Op::Input => {
                    let mut byte = [0];
                    self.tape[self.ptr] = match self.input.read(&mut byte) {
                        Ok(0) => 0,
                        Ok(_) => byte[0],
                        Err(err) => return Err(format!("failed to read input: {err}")),
                    };
                }
                Op::Output => self
                    .output
                    .write_all(&[self.tape[self.ptr]])
                    .map_err(|err| format!("failed to write output: {err}"))
                    .and_then(|_| {
                        self.output
                            .flush()
                            .map_err(|err| format!("failed to flush output: {err}"))
                    })?,
                Op::Clear => self.tape[self.ptr] = 0,
                Op::Loop(body) => {
                    while self.tape[self.ptr] != 0 {
                        self.run_ops(body)?;
                    }
                }
                Op::Dispatch => self.stats.dispatches += 1,
            }
        }
        Ok(())
    }
}

fn checked_index(ptr: usize, delta: isize) -> Result<usize, String> {
    let next = ptr
        .checked_add_signed(delta)
        .ok_or_else(|| "data pointer moved before cell 0".to_string())?;
    if next >= TAPE_SIZE {
        Err(format!(
            "data pointer moved past cell {}",
            TAPE_SIZE.saturating_sub(1)
        ))
    } else {
        Ok(next)
    }
}

fn add_wrapping(value: u8, delta: i16) -> u8 {
    if delta >= 0 {
        value.wrapping_add(delta as u8)
    } else {
        value.wrapping_sub(delta.unsigned_abs() as u8)
    }
}
