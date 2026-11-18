use std::collections::BTreeMap;
use std::io::{Read, Write};

const TAPE_SIZE: usize = 30_000;
pub const DISPATCH_MARKER: &str = "SANE PROFILE DISPATCH";
pub(crate) const ARRAY_GET_BF: &str = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<";
pub(crate) const ARRAY_SET_BF: &str =
    ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<";
pub(crate) const WIDE_GET_BF: &str = ">>>>[>>>>>>>>>>>>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]<<<<<<<<<<<[->>>>+<<<<]<<<<[->>>>+<<<<]>>>>-]>>>>>>>>>>>>[-<<<<+<<<<<<<<+>>>>>>>>>>>>]>[-<<<<+<<<<<<<<+>>>>>>>>>>>>]>[-<<<<+<<<<<<<<+>>>>>>>>>>>>]>[-<<<<+<<<<<<<<+>>>>>>>>>>>>]<<<><<<<<<<<<<<<[->>>>>>>>>>>>+<<<<<<<<<<<<]>[->>>>>>>>>>>>+<<<<<<<<<<<<]>[->>>>>>>>>>>>+<<<<<<<<<<<<]>>>>>>>>><<<<<<<<<<<<[->>>>>>>>>>>>+<<<<<<<<<<<<]>>>>[[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]<<<>>>>[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]<<<<<<<<<<<<<<<<<<<[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]<<<>>>>>>>>-]<<<<<<<<";
pub(crate) const WIDE_SET_BF: &str = ">>>>[>>>>>>>>>>>>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]>[-<<<<<<<<<<<<<<<<+>>>>>>>>>>>>>>>>]<<<<<<<[->>>>+<<<<]>[->>>>+<<<<]>[->>>>+<<<<]>[->>>>+<<<<]<<<<<<<[->>>>+<<<<]<<<<[->>>>+<<<<]>>>>-]>>>>>>>>>>>>[-]>[-]>[-]>[-]<<<<<<<[->>>>+<<<<]>[->>>>+<<<<]>[->>>>+<<<<]>[->>>>+<<<<]<<<<<<<[[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]>[-<<<<+>>>>]<<<<<<<<<<<<<<<[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]>[->>>>>>>>>>>>>>>>+<<<<<<<<<<<<<<<<]<<<>>>>>>>>-]<<<<<<<<";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    edits: Box<[(i32, i8)]>,
    advance: i32,
    min_offset: i32,
    max_offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AddMul {
    effects: Box<[(i32, i8)]>,
    min_offset: i32,
    max_offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Instr {
    Add(i8),
    Move(i32),
    Set(u8),
    Block(Box<Block>),
    AddMul(Box<AddMul>),
    ScanZero { delta: i32 },
    Input,
    Output,
    Dispatch,
    JumpIfZero(u32),
    JumpIfNonZero(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Add(i8),
    Move(i32),
    Set(u8),
    Block(Block),
    AddMul(AddMul),
    ScanZero { delta: i32 },
    Input,
    Output,
    Dispatch,
    Loop(Vec<Node>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecutionStats {
    pub executed_ops: u64,
    pub dispatches: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    code: Box<[Instr]>,
}

impl Program {
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut parser = Parser {
            bytes: parse_chars(src),
            pos: 0,
        };
        let nodes = parser.parse_ops(false)?;
        if parser.pos != parser.bytes.len() {
            return Err("internal error: parser did not consume input".to_string());
        }
        let mut code = Vec::new();
        flatten(peephole(fuse_blocks(nodes)), &mut code)?;
        Ok(Self {
            code: code.into_boxed_slice(),
        })
    }

    pub fn run<R: Read, W: Write>(&self, input: R, output: W) -> Result<(), String> {
        let mut runtime = Runtime {
            tape: vec![0; TAPE_SIZE],
            ptr: 0,
            input,
            output,
            stats: ExecutionStats::default(),
        };
        runtime.execute::<false>(&self.code)
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
        runtime.execute::<true>(&self.code)?;
        Ok(runtime.stats)
    }

    pub fn operation_count(&self) -> usize {
        self.code.len()
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

fn parse_chars(src: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(src.len());
    let mut remaining = src;
    while let Some(marker_start) = remaining.find(DISPATCH_MARKER) {
        append_filtered(remaining[..marker_start].as_bytes(), &mut bytes);
        bytes.push(0);
        remaining = &remaining[marker_start + DISPATCH_MARKER.len()..];
    }
    append_filtered(remaining.as_bytes(), &mut bytes);
    bytes
}

fn append_filtered(input: &[u8], output: &mut Vec<u8>) {
    for &byte in input {
        if is_bf_byte(byte) || byte.is_ascii_digit() {
            output.push(byte);
        } else {
            // Keep non-BF text from joining digits from separate comment lines
            // into one repeat count.
            output.push(b' ');
        }
    }
}

fn is_bf_byte(byte: u8) -> bool {
    matches!(byte, b'>' | b'<' | b'+' | b'-' | b'.' | b',' | b'[' | b']')
}

struct Parser {
    bytes: Vec<u8>,
    pos: usize,
}

impl Parser {
    fn parse_ops(&mut self, in_loop: bool) -> Result<Vec<Node>, String> {
        let mut nodes = Vec::new();
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'+' | b'-' => self.parse_add(&mut nodes)?,
                b'>' | b'<' => self.parse_move(&mut nodes)?,
                b'.' => {
                    self.pos += 1;
                    nodes.push(Node::Output);
                }
                b',' => {
                    self.pos += 1;
                    nodes.push(Node::Input);
                }
                b'[' => {
                    self.pos += 1;
                    let body = self.parse_ops(true)?;
                    nodes.push(optimize_loop(body));
                }
                b']' if in_loop => {
                    self.pos += 1;
                    return Ok(nodes);
                }
                b']' => return Err(format!("unmatched `]` at instruction {}", self.pos)),
                0 => {
                    self.pos += 1;
                    nodes.push(Node::Dispatch);
                }
                b'0'..=b'9' => match self.peek_repeat()? {
                    Some((_, command, _)) if matches!(command, b'+' | b'-') => {
                        self.parse_add(&mut nodes)?
                    }
                    Some((_, command, _)) if matches!(command, b'>' | b'<') => {
                        self.parse_move(&mut nodes)?
                    }
                    _ => self.skip_digits(),
                },
                _ => self.pos += 1,
            }
        }

        if in_loop {
            Err("unmatched `[`".to_string())
        } else {
            Ok(nodes)
        }
    }

    fn peek_repeat(&self) -> Result<Option<(u64, u8, usize)>, String> {
        let mut index = self.pos;
        if index >= self.bytes.len() || !self.bytes[index].is_ascii_digit() {
            return Ok(None);
        }
        let mut count: u64 = 0;
        while index < self.bytes.len() && self.bytes[index].is_ascii_digit() {
            let digit = u64::from(self.bytes[index] - b'0');
            count = count
                .checked_mul(10)
                .and_then(|value| value.checked_add(digit))
                .ok_or_else(|| "repeat count is too large".to_string())?;
            index += 1;
        }
        if index < self.bytes.len() && matches!(self.bytes[index], b'+' | b'-' | b'>' | b'<') {
            Ok(Some((count, self.bytes[index], index + 1)))
        } else {
            Ok(None)
        }
    }

    fn skip_digits(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
    }

    fn parse_add(&mut self, nodes: &mut Vec<Node>) -> Result<(), String> {
        let mut delta: i32 = 0;
        loop {
            if let Some((count, command, after)) = self.peek_repeat()? {
                if !matches!(command, b'+' | b'-') {
                    break;
                }
                self.pos = after;
                let amount = (count % 256) as i32;
                delta = if command == b'+' {
                    delta.wrapping_add(amount)
                } else {
                    delta.wrapping_sub(amount)
                };
                continue;
            }
            if self.pos >= self.bytes.len() {
                break;
            }
            match self.bytes[self.pos] {
                b'+' => {
                    self.pos += 1;
                    delta = delta.wrapping_add(1);
                }
                b'-' => {
                    self.pos += 1;
                    delta = delta.wrapping_sub(1);
                }
                _ => break,
            }
        }
        if delta % 256 != 0 {
            push_add(nodes, (delta as u8) as i8);
        }
        Ok(())
    }

    fn parse_move(&mut self, nodes: &mut Vec<Node>) -> Result<(), String> {
        loop {
            let mut amount: u64 = 0;
            let mut direction = None;
            loop {
                let Some((count, command, after)) = self.peek_move()? else {
                    break;
                };
                let towards_right = command == b'>';
                if let Some(previous) = direction {
                    if previous != towards_right {
                        break;
                    }
                } else {
                    direction = Some(towards_right);
                }
                self.pos = after;
                amount = amount
                    .checked_add(count)
                    .ok_or_else(|| "move count is too large".to_string())?;
            }

            let Some(towards_right) = direction else {
                break;
            };
            let amount =
                i64::try_from(amount).map_err(|_| "move count is too large".to_string())?;
            let delta = if towards_right { amount } else { -amount };
            let delta = i32::try_from(delta).map_err(|_| "move count is too large".to_string())?;
            push_move(nodes, delta);
        }
        Ok(())
    }

    fn peek_move(&self) -> Result<Option<(u64, u8, usize)>, String> {
        if let Some((count, command, after)) = self.peek_repeat()? {
            return Ok(matches!(command, b'>' | b'<').then_some((count, command, after)));
        }
        if self.pos < self.bytes.len() && matches!(self.bytes[self.pos], b'>' | b'<') {
            return Ok(Some((1, self.bytes[self.pos], self.pos + 1)));
        }
        Ok(None)
    }
}

fn push_add(nodes: &mut Vec<Node>, delta: i8) {
    if delta == 0 {
        return;
    }
    let mut merged = false;
    let mut remove = false;
    if let Some(Node::Add(previous)) = nodes.last_mut() {
        let sum = ((*previous as i16) + (delta as i16)) as u8 as i8;
        if sum == 0 {
            remove = true;
        } else {
            *previous = sum;
        }
        merged = true;
    }
    if remove {
        nodes.pop();
    } else if !merged {
        nodes.push(Node::Add(delta));
    }
}

fn push_move(nodes: &mut Vec<Node>, delta: i32) {
    if delta != 0 {
        nodes.push(Node::Move(delta));
    }
}

fn optimize_loop(body: Vec<Node>) -> Node {
    if matches!(body.as_slice(), [Node::Add(-1)] | [Node::Add(1)]) {
        return Node::Set(0);
    }
    if matches!(body.as_slice(), [Node::Set(0)]) {
        return Node::Set(0);
    }
    if let [Node::Move(delta)] = body.as_slice()
        && *delta != 0
    {
        return Node::ScanZero { delta: *delta };
    }
    if let Some(candidate) = add_mul(&body) {
        if candidate.effects.is_empty() && candidate.min_offset == 0 && candidate.max_offset == 0 {
            return Node::Set(0);
        }
        return Node::AddMul(AddMul {
            effects: candidate.effects.into_boxed_slice(),
            min_offset: candidate.min_offset,
            max_offset: candidate.max_offset,
        });
    }
    Node::Loop(peephole(fuse_blocks(body)))
}

struct AddMulCandidate {
    effects: Vec<(i32, i8)>,
    min_offset: i32,
    max_offset: i32,
}

fn add_mul(body: &[Node]) -> Option<AddMulCandidate> {
    let mut offset = 0i32;
    let mut min_offset = 0i32;
    let mut max_offset = 0i32;
    let mut effects = BTreeMap::<i32, i32>::new();

    for node in body {
        match node {
            Node::Add(delta) => {
                if *delta != 0 {
                    *effects.entry(offset).or_insert(0) += *delta as i32;
                }
                min_offset = min_offset.min(offset);
                max_offset = max_offset.max(offset);
            }
            Node::Move(delta) => {
                offset = offset.checked_add(*delta)?;
                min_offset = min_offset.min(offset);
                max_offset = max_offset.max(offset);
            }
            _ => return None,
        }
    }

    if offset != 0 {
        return None;
    }
    let origin = (effects.get(&0).copied().unwrap_or(0) as u8) as i8;
    if origin != -1 {
        return None;
    }
    effects.remove(&0);

    let mut normalized = Vec::new();
    for (offset, sum) in effects {
        let delta = (sum as u8) as i8;
        if delta != 0 {
            normalized.push((offset, delta));
        }
    }
    Some(AddMulCandidate {
        effects: normalized,
        min_offset,
        max_offset,
    })
}

fn fuse_blocks(nodes: Vec<Node>) -> Vec<Node> {
    let mut fused = Vec::with_capacity(nodes.len());
    let mut run = Vec::new();
    for node in nodes {
        match node {
            Node::Add(_) | Node::Move(_) => run.push(node),
            other => {
                flush_run(&mut run, &mut fused);
                fused.push(other);
            }
        }
    }
    flush_run(&mut run, &mut fused);
    fused
}

fn flush_run(run: &mut Vec<Node>, output: &mut Vec<Node>) {
    if run.is_empty() {
        return;
    }
    if run.len() == 1 {
        output.push(run.pop().expect("checked non-empty"));
        return;
    }
    let nodes = std::mem::take(run);
    match build_block(&nodes) {
        Some(block) => output.push(Node::Block(block)),
        None => output.extend(nodes),
    }
}

fn build_block(run: &[Node]) -> Option<Block> {
    let mut offset = 0i32;
    let mut min_offset = 0i32;
    let mut max_offset = 0i32;
    let mut edits = BTreeMap::<i32, i32>::new();

    for node in run {
        match node {
            Node::Add(delta) => {
                if *delta != 0 {
                    *edits.entry(offset).or_insert(0) += *delta as i32;
                }
                min_offset = min_offset.min(offset);
                max_offset = max_offset.max(offset);
            }
            Node::Move(delta) => {
                offset = offset.checked_add(*delta)?;
                min_offset = min_offset.min(offset);
                max_offset = max_offset.max(offset);
            }
            _ => return None,
        }
    }

    let mut pairs = Vec::new();
    for (offset, sum) in edits {
        let delta = (sum as u8) as i8;
        if delta != 0 {
            pairs.push((offset, delta));
        }
    }
    Some(Block {
        edits: pairs.into_boxed_slice(),
        advance: offset,
        min_offset,
        max_offset,
    })
}

fn peephole(nodes: Vec<Node>) -> Vec<Node> {
    let mut output = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            Node::Add(delta) => {
                if let Some(Node::Set(value)) = output.last_mut() {
                    *value = value.wrapping_add(delta as u8);
                } else {
                    output.push(Node::Add(delta));
                }
            }
            Node::Set(value) => {
                if matches!(output.last(), Some(Node::Add(_))) {
                    output.pop();
                }
                if let Some(Node::Set(previous)) = output.last_mut() {
                    *previous = value;
                } else {
                    output.push(Node::Set(value));
                }
            }
            other => output.push(other),
        }
    }
    output
}

fn flatten(nodes: Vec<Node>, code: &mut Vec<Instr>) -> Result<(), String> {
    for node in nodes {
        match node {
            Node::Add(delta) => code.push(Instr::Add(delta)),
            Node::Move(delta) => code.push(Instr::Move(delta)),
            Node::Set(value) => code.push(Instr::Set(value)),
            Node::Block(block) => code.push(Instr::Block(Box::new(block))),
            Node::AddMul(add_mul) => code.push(Instr::AddMul(Box::new(add_mul))),
            Node::ScanZero { delta } => code.push(Instr::ScanZero { delta }),
            Node::Input => code.push(Instr::Input),
            Node::Output => code.push(Instr::Output),
            Node::Dispatch => code.push(Instr::Dispatch),
            Node::Loop(body) => {
                let start = code.len();
                code.push(Instr::JumpIfZero(0));
                flatten(body, code)?;
                let after = u32::try_from(code.len() + 1)
                    .map_err(|_| "program is too large".to_string())?;
                code[start] = Instr::JumpIfZero(after);
                let target =
                    u32::try_from(start + 1).map_err(|_| "program is too large".to_string())?;
                code.push(Instr::JumpIfNonZero(target));
            }
        }
    }
    Ok(())
}

struct Runtime<R, W> {
    tape: Vec<u8>,
    ptr: usize,
    input: R,
    output: W,
    stats: ExecutionStats,
}

impl<R: Read, W: Write> Runtime<R, W> {
    fn execute<const STATS: bool>(&mut self, code: &[Instr]) -> Result<(), String> {
        let mut pc = 0usize;
        while pc < code.len() {
            if STATS {
                self.stats.executed_ops += 1;
            }
            match &code[pc] {
                Instr::Add(delta) => {
                    self.tape[self.ptr] = self.tape[self.ptr].wrapping_add(*delta as u8);
                }
                Instr::Move(delta) => {
                    self.ptr = checked_index(self.ptr, *delta)?;
                }
                Instr::Set(value) => {
                    self.tape[self.ptr] = *value;
                }
                Instr::Block(block) => {
                    self.apply_block(block)?;
                }
                Instr::AddMul(add_mul) => {
                    self.apply_add_mul(add_mul)?;
                }
                Instr::ScanZero { delta } => {
                    self.scan_zero(*delta)?;
                }
                Instr::Input => {
                    let mut byte = [0];
                    self.tape[self.ptr] = match self.input.read(&mut byte) {
                        Ok(0) => 0,
                        Ok(_) => byte[0],
                        Err(err) => return Err(format!("failed to read input: {err}")),
                    };
                }
                Instr::Output => self
                    .output
                    .write_all(&[self.tape[self.ptr]])
                    .map_err(|err| format!("failed to write output: {err}"))
                    .and_then(|_| {
                        self.output
                            .flush()
                            .map_err(|err| format!("failed to flush output: {err}"))
                    })?,
                Instr::Dispatch => {
                    if STATS {
                        self.stats.dispatches += 1;
                    }
                }
                Instr::JumpIfZero(target) => {
                    if self.tape[self.ptr] == 0 {
                        pc = *target as usize;
                        continue;
                    }
                }
                Instr::JumpIfNonZero(target) => {
                    if self.tape[self.ptr] != 0 {
                        pc = *target as usize;
                        continue;
                    }
                }
            }
            pc += 1;
        }
        Ok(())
    }

    fn apply_block(&mut self, block: &Block) -> Result<(), String> {
        self.check_span(block.min_offset, block.max_offset)?;
        for &(offset, delta) in block.edits.iter() {
            let cell = self.offset_cell(offset);
            self.tape[cell] = self.tape[cell].wrapping_add(delta as u8);
        }
        self.ptr = self.offset_cell(block.advance);
        Ok(())
    }

    fn apply_add_mul(&mut self, add_mul: &AddMul) -> Result<(), String> {
        let value = self.tape[self.ptr];
        if value == 0 {
            return Ok(());
        }
        self.check_span(add_mul.min_offset, add_mul.max_offset)?;
        for &(offset, coefficient) in add_mul.effects.iter() {
            let cell = self.offset_cell(offset);
            self.tape[cell] = self.tape[cell].wrapping_add(value.wrapping_mul(coefficient as u8));
        }
        self.tape[self.ptr] = 0;
        Ok(())
    }

    fn scan_zero(&mut self, delta: i32) -> Result<(), String> {
        if self.tape[self.ptr] == 0 {
            return Ok(());
        }
        if delta == 0 {
            return Err("internal error: zero scan delta".to_string());
        }
        let mut index = self.ptr as i64;
        loop {
            index = index
                .checked_add(delta as i64)
                .ok_or_else(|| "data pointer moved past the tape".to_string())?;
            let cell = checked_cell(index)?;
            if self.tape[cell] == 0 {
                self.ptr = cell;
                return Ok(());
            }
        }
    }

    fn check_span(&self, min_offset: i32, max_offset: i32) -> Result<(), String> {
        checked_cell(self.ptr as i64 + min_offset as i64)?;
        checked_cell(self.ptr as i64 + max_offset as i64)?;
        Ok(())
    }

    fn offset_cell(&self, offset: i32) -> usize {
        (self.ptr as i64 + offset as i64) as usize
    }
}

fn checked_index(ptr: usize, delta: i32) -> Result<usize, String> {
    checked_cell(ptr as i64 + delta as i64)
}

fn checked_cell(index: i64) -> Result<usize, String> {
    if index < 0 {
        Err("data pointer moved before cell 0".to_string())
    } else if index >= TAPE_SIZE as i64 {
        Err(format!(
            "data pointer moved past cell {}",
            TAPE_SIZE.saturating_sub(1)
        ))
    } else {
        Ok(index as usize)
    }
}
