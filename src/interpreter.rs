use std::collections::BTreeMap;
use std::io::{Read, Write};

const TAPE_SIZE: usize = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Add(i16),
    Move(isize),
    Input,
    Output,
    Clear,
    AddMul(Vec<(isize, i16)>),
    Loop(Vec<Op>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    ops: Vec<Op>,
}

impl Program {
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut parser = Parser {
            chars: src
                .chars()
                .filter(|ch| matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']'))
                .collect(),
            pos: 0,
        };
        let ops = parser.parse_ops(false)?;
        if parser.pos != parser.chars.len() {
            return Err("internal error: parser did not consume input".to_string());
        }
        Ok(Self { ops })
    }

    pub fn run<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), String> {
        let mut runtime = Runtime {
            tape: vec![0; TAPE_SIZE],
            ptr: 0,
            input,
            output,
        };
        runtime.run_ops(&self.ops)
    }

    pub fn ops(&self) -> &[Op] {
        &self.ops
    }
}

pub fn run_source<R: Read, W: Write>(src: &str, input: R, output: W) -> Result<(), String> {
    Program::parse(src)?.run(input, output)
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

    if let Some(effects) = add_mul_effects(&body) {
        return Op::AddMul(effects);
    }

    Op::Loop(body)
}

fn add_mul_effects(body: &[Op]) -> Option<Vec<(isize, i16)>> {
    let mut offset = 0isize;
    let mut effects = BTreeMap::<isize, i16>::new();

    for op in body {
        match *op {
            Op::Add(delta) => {
                *effects.entry(offset).or_default() += delta;
            }
            Op::Move(delta) => offset += delta,
            _ => return None,
        }
    }

    if offset != 0 || effects.get(&0).copied() != Some(-1) {
        return None;
    }

    effects.remove(&0);
    let effects = effects
        .into_iter()
        .filter(|(_, delta)| *delta != 0)
        .collect::<Vec<_>>();
    if effects.is_empty() {
        Some(Vec::new())
    } else {
        Some(effects)
    }
}

struct Runtime<R, W> {
    tape: Vec<u8>,
    ptr: usize,
    input: R,
    output: W,
}

impl<R: Read, W: Write> Runtime<R, W> {
    fn run_ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::Add(delta) => {
                    self.tape[self.ptr] = add_wrapping(self.tape[self.ptr], *delta);
                }
                Op::Move(delta) => self.move_ptr(*delta)?,
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
                Op::AddMul(effects) => {
                    let value = self.tape[self.ptr];
                    for (offset, factor) in effects {
                        let index = checked_index(self.ptr, *offset)?;
                        let delta = value.wrapping_mul(*factor as u8);
                        self.tape[index] = self.tape[index].wrapping_add(delta);
                    }
                    self.tape[self.ptr] = 0;
                }
                Op::Loop(body) => {
                    while self.tape[self.ptr] != 0 {
                        self.run_ops(body)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn move_ptr(&mut self, delta: isize) -> Result<(), String> {
        self.ptr = checked_index(self.ptr, delta)?;
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
