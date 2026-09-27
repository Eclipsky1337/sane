use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::OnceLock;

const TAPE_SIZE: usize = 30_000;
pub const DISPATCH_MARKER: &str = "SANE PROFILE DISPATCH";
pub const ARRAY_GET_MARKER: &str = "SANE ARRAY GET BYTE";
pub const ARRAY_SET_MARKER: &str = "SANE ARRAY SET BYTE";
pub const ARRAY_GET_WIDE_MARKER: &str = "SANE ARRAY GET STRIDE4";
pub const ARRAY_SET_WIDE_MARKER: &str = "SANE ARRAY SET STRIDE4";
pub(crate) const ARRAY_GET_BF: &str = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<";
pub(crate) const ARRAY_SET_BF: &str =
    ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<";
const ARRAY_GET_TOKEN: char = '\x01';
const ARRAY_SET_TOKEN: char = '\x02';
const ARRAY_GET_WIDE_TOKEN: char = '\x03';
const ARRAY_SET_WIDE_TOKEN: char = '\x04';

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Add(i16),
    Move(isize),
    Input,
    Output,
    Clear,
    AddMul(Vec<(isize, i16)>),
    Block(Box<Block>),
    Loop(Vec<Op>),
    Dispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Linear,
    ArrayGet,
    ArraySet,
    ArrayGetWide,
    ArraySetWide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub edits: Vec<(isize, i16)>,
    pub moves: Vec<isize>,
    pub advance: isize,
    pub min_offset: isize,
    pub max_offset: isize,
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
        let mut parser = Parser {
            chars: parse_chars(src),
            pos: 0,
        };
        let ops = parser.parse_ops(false)?;
        if parser.pos != parser.chars.len() {
            return Err("internal error: parser did not consume input".to_string());
        }
        Ok(Self { ops: fuse_ops(ops) })
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

fn parse_chars(src: &str) -> Vec<char> {
    let mut chars = Vec::new();
    let mut remaining = src;
    let markers = [
        (DISPATCH_MARKER, '\0'),
        (ARRAY_GET_MARKER, ARRAY_GET_TOKEN),
        (ARRAY_SET_MARKER, ARRAY_SET_TOKEN),
        (ARRAY_GET_WIDE_MARKER, ARRAY_GET_WIDE_TOKEN),
        (ARRAY_SET_WIDE_MARKER, ARRAY_SET_WIDE_TOKEN),
    ];

    while let Some((marker_start, marker, token)) = markers
        .iter()
        .filter_map(|&(marker, token)| remaining.find(marker).map(|start| (start, marker, token)))
        .min_by_key(|&(start, _, _)| start)
    {
        chars.extend(
            remaining[..marker_start]
                .chars()
                .filter(|ch| matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']')),
        );
        chars.push(token);
        remaining = &remaining[marker_start + marker.len()..];
    }

    chars.extend(
        remaining
            .chars()
            .filter(|ch| matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']')),
    );
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
                ARRAY_GET_TOKEN | ARRAY_SET_TOKEN | ARRAY_GET_WIDE_TOKEN | ARRAY_SET_WIDE_TOKEN => {
                    let (get, stride, kind) = match self.chars[self.pos] {
                        ARRAY_GET_TOKEN => (true, 1, BlockKind::ArrayGet),
                        ARRAY_SET_TOKEN => (false, 1, BlockKind::ArraySet),
                        ARRAY_GET_WIDE_TOKEN => (true, 4, BlockKind::ArrayGetWide),
                        _ => (false, 4, BlockKind::ArraySetWide),
                    };
                    self.pos += 1;
                    let template = if stride == 1 {
                        if get { ARRAY_GET_BF } else { ARRAY_SET_BF }
                    } else {
                        strided_array_bf(get)
                    };
                    if self.chars[self.pos..]
                        .iter()
                        .copied()
                        .take(template.len())
                        .eq(template.chars())
                    {
                        self.pos += template.len();
                        ops.push(Op::Block(Box::new(Block {
                            kind,
                            edits: Vec::new(),
                            moves: Vec::new(),
                            advance: 0,
                            min_offset: 0,
                            max_offset: 0,
                        })));
                    }
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

    if let Some(effects) = add_mul_effects(&body) {
        return Op::AddMul(effects);
    }

    Op::Loop(fuse_ops(body))
}

fn fuse_ops(ops: Vec<Op>) -> Vec<Op> {
    if !ops
        .windows(3)
        .any(|run| run.iter().all(|op| matches!(op, Op::Add(_) | Op::Move(_))))
    {
        return ops;
    }

    let mut fused = Vec::with_capacity(ops.len());
    let mut remaining = ops.into_iter().peekable();
    while let Some(op) = remaining.next() {
        if !matches!(op, Op::Add(_) | Op::Move(_)) {
            fused.push(op);
            continue;
        }

        if !matches!(remaining.peek(), Some(Op::Add(_) | Op::Move(_))) {
            fused.push(op);
            continue;
        }
        let next = remaining.next().unwrap();
        if !matches!(remaining.peek(), Some(Op::Add(_) | Op::Move(_))) {
            fused.extend([op, next]);
            continue;
        }

        let mut run = vec![op, next];
        while matches!(remaining.peek(), Some(Op::Add(_) | Op::Move(_))) {
            run.push(remaining.next().unwrap());
        }
        if run.len() < 3
            || !run.iter().any(|op| matches!(op, Op::Add(_)))
            || !run.iter().any(|op| matches!(op, Op::Move(_)))
        {
            fused.extend(run);
            continue;
        }

        let mut edits = Vec::new();
        let mut moves = Vec::new();
        let mut offset = 0isize;
        let mut min_offset = 0isize;
        let mut max_offset = 0isize;
        for op in run {
            match op {
                Op::Add(delta) => edits.push((offset, delta)),
                Op::Move(delta) => {
                    offset += delta;
                    moves.push(offset);
                    min_offset = min_offset.min(offset);
                    max_offset = max_offset.max(offset);
                }
                _ => unreachable!(),
            }
        }
        fused.push(Op::Block(Box::new(Block {
            kind: BlockKind::Linear,
            edits,
            moves,
            advance: offset,
            min_offset,
            max_offset,
        })));
    }
    fused
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

fn strided_array_bf(get: bool) -> &'static str {
    static GET: OnceLock<String> = OnceLock::new();
    static SET: OnceLock<String> = OnceLock::new();
    let code = if get { ARRAY_GET_BF } else { ARRAY_SET_BF };
    let cache = if get { &GET } else { &SET };
    cache.get_or_init(|| {
        let mut strided = String::with_capacity(code.len() * 4);
        for instruction in code.bytes() {
            match instruction {
                b'>' => strided.push_str(">>>>"),
                b'<' => strided.push_str("<<<<"),
                _ => strided.push(instruction as char),
            }
        }
        strided
    })
}

fn array_fallback(get: bool, stride: usize) -> &'static [Op] {
    static GET: OnceLock<Vec<Op>> = OnceLock::new();
    static SET: OnceLock<Vec<Op>> = OnceLock::new();
    static WIDE_GET: OnceLock<Vec<Op>> = OnceLock::new();
    static WIDE_SET: OnceLock<Vec<Op>> = OnceLock::new();
    match (get, stride) {
        (true, 1) => GET.get_or_init(|| Program::parse(ARRAY_GET_BF).unwrap().ops),
        (false, 1) => SET.get_or_init(|| Program::parse(ARRAY_SET_BF).unwrap().ops),
        (true, 4) => WIDE_GET.get_or_init(|| Program::parse(strided_array_bf(true)).unwrap().ops),
        (false, 4) => WIDE_SET.get_or_init(|| Program::parse(strided_array_bf(false)).unwrap().ops),
        _ => unreachable!("unsupported array stride"),
    }
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
                Op::Block(block) => {
                    match block.kind {
                        BlockKind::ArrayGet => {
                            self.run_array(true, 1)?;
                            continue;
                        }
                        BlockKind::ArraySet => {
                            self.run_array(false, 1)?;
                            continue;
                        }
                        BlockKind::ArrayGetWide => {
                            self.run_array(true, 4)?;
                            continue;
                        }
                        BlockKind::ArraySetWide => {
                            self.run_array(false, 4)?;
                            continue;
                        }
                        BlockKind::Linear => {}
                    }
                    if checked_index(self.ptr, block.min_offset).is_err()
                        || checked_index(self.ptr, block.max_offset).is_err()
                    {
                        for offset in &block.moves {
                            checked_index(self.ptr, *offset)?;
                        }
                    }
                    for (offset, delta) in &block.edits {
                        let index = self.ptr.wrapping_add_signed(*offset);
                        self.tape[index] = add_wrapping(self.tape[index], *delta);
                    }
                    self.ptr = self.ptr.wrapping_add_signed(block.advance);
                }
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

    fn move_ptr(&mut self, delta: isize) -> Result<(), String> {
        self.ptr = checked_index(self.ptr, delta)?;
        Ok(())
    }

    #[inline(never)]
    fn run_array(&mut self, get: bool, stride: usize) -> Result<(), String> {
        if let Some(index) = self.array_target(get, stride) {
            if get {
                self.tape[self.ptr + 3 * stride] = self.tape[index];
            } else {
                self.tape[index] = self.tape[self.ptr + 3 * stride];
                self.tape[self.ptr + 3 * stride] = 0;
            }
            self.tape[self.ptr + stride] = 0;
            self.tape[self.ptr + 2 * stride] = 0;
            Ok(())
        } else {
            self.run_ops(array_fallback(get, stride))
        }
    }

    fn array_target(&self, get: bool, stride: usize) -> Option<usize> {
        let space = *self.tape.get(self.ptr)?;
        let first = *self.tape.get(self.ptr + stride)?;
        let second = *self.tape.get(self.ptr + 2 * stride)?;
        let transfer = *self.tape.get(self.ptr + 3 * stride)?;
        if space != 0 || first != second || get && transfer != 0 {
            return None;
        }
        let index = self.ptr + (4 + usize::from(first)) * stride;
        (index + 4 * stride < self.tape.len()).then_some(index)
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
