use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

const TAPE_SIZE: usize = 30_000;
const SIGINT: i32 = 2;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

type SignalHandler = extern "C" fn(i32);

unsafe extern "C" {
    fn signal(sig: i32, handler: SignalHandler) -> SignalHandler;
}

extern "C" fn handle_sigint(_: i32) {
    INTERRUPTED.store(true, Ordering::SeqCst);
}

pub fn install_ctrlc_handler() {
    unsafe {
        signal(SIGINT, handle_sigint);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inst {
    Add,
    Sub,
    Left,
    Right,
    Input,
    Output,
    LoopStart,
    LoopEnd,
}

impl Inst {
    fn from_char(ch: char) -> Option<Self> {
        match ch {
            '+' => Some(Self::Add),
            '-' => Some(Self::Sub),
            '<' => Some(Self::Left),
            '>' => Some(Self::Right),
            ',' => Some(Self::Input),
            '.' => Some(Self::Output),
            '[' => Some(Self::LoopStart),
            ']' => Some(Self::LoopEnd),
            _ => None,
        }
    }

    fn as_char(self) -> char {
        match self {
            Self::Add => '+',
            Self::Sub => '-',
            Self::Left => '<',
            Self::Right => '>',
            Self::Input => ',',
            Self::Output => '.',
            Self::LoopStart => '[',
            Self::LoopEnd => ']',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugProgram {
    insts: Vec<Inst>,
    jumps: Vec<Option<usize>>,
    symbols: Vec<DebugSymbol>,
}

impl DebugProgram {
    pub fn parse(src: &str) -> Result<Self, String> {
        let symbols = parse_symbols(src);
        let insts = src.chars().filter_map(Inst::from_char).collect::<Vec<_>>();
        let jumps = build_jumps(&insts)?;
        Ok(Self {
            insts,
            jumps,
            symbols,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugSymbol {
    Cell {
        name: String,
        cell: usize,
    },
    Array {
        name: String,
        base: usize,
        len: usize,
        first: usize,
        last: usize,
    },
}

impl DebugSymbol {
    fn name(&self) -> &str {
        match self {
            Self::Cell { name, .. } | Self::Array { name, .. } => name,
        }
    }

    fn address(&self) -> Address {
        match self {
            Self::Cell { cell, .. } => Address {
                start: *cell,
                len: 1,
            },
            Self::Array { len, first, .. } => Address {
                start: *first,
                len: *len,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct DebugVm {
    program: DebugProgram,
    tape: Vec<u8>,
    ptr: usize,
    pc: usize,
    halted: bool,
    breakpoints: BTreeSet<usize>,
    watchpoints: BTreeMap<usize, u8>,
}

impl DebugVm {
    pub fn new(program: DebugProgram) -> Self {
        Self {
            program,
            tape: vec![0; TAPE_SIZE],
            ptr: 0,
            pc: 0,
            halted: false,
            breakpoints: BTreeSet::new(),
            watchpoints: BTreeMap::new(),
        }
    }

    fn restart(&mut self) {
        self.tape.fill(0);
        self.ptr = 0;
        self.pc = 0;
        self.halted = false;
    }

    fn step<R: Read, W: Write>(&mut self, input: &mut R, output: &mut W) -> Result<(), String> {
        if self.halted {
            return Ok(());
        }
        let Some(inst) = self.program.insts.get(self.pc).copied() else {
            self.halted = true;
            return Ok(());
        };
        match inst {
            Inst::Add => {
                self.tape[self.ptr] = self.tape[self.ptr].wrapping_add(1);
                self.pc += 1;
            }
            Inst::Sub => {
                self.tape[self.ptr] = self.tape[self.ptr].wrapping_sub(1);
                self.pc += 1;
            }
            Inst::Left => {
                self.ptr = checked_index(self.ptr, -1)?;
                self.pc += 1;
            }
            Inst::Right => {
                self.ptr = checked_index(self.ptr, 1)?;
                self.pc += 1;
            }
            Inst::Input => {
                let mut byte = [0];
                self.tape[self.ptr] = match input.read(&mut byte) {
                    Ok(0) => 0,
                    Ok(_) => byte[0],
                    Err(err)
                        if err.kind() == ErrorKind::Interrupted
                            && INTERRUPTED.load(Ordering::SeqCst) =>
                    {
                        return Ok(());
                    }
                    Err(err) => return Err(format!("failed to read input: {err}")),
                };
                self.pc += 1;
            }
            Inst::Output => {
                output
                    .write_all(&[self.tape[self.ptr]])
                    .map_err(|err| format!("failed to write output: {err}"))?;
                output
                    .flush()
                    .map_err(|err| format!("failed to flush output: {err}"))?;
                self.pc += 1;
            }
            Inst::LoopStart => {
                if self.tape[self.ptr] == 0 {
                    self.pc = self.jump_target(self.pc)? + 1;
                } else {
                    self.pc += 1;
                }
            }
            Inst::LoopEnd => {
                if self.tape[self.ptr] != 0 {
                    self.pc = self.jump_target(self.pc)? + 1;
                } else {
                    self.pc += 1;
                }
            }
        }
        if self.pc >= self.program.insts.len() {
            self.halted = true;
        }
        Ok(())
    }

    fn jump_target(&self, pc: usize) -> Result<usize, String> {
        self.program
            .jumps
            .get(pc)
            .and_then(|target| *target)
            .ok_or_else(|| format!("missing jump target for instruction {pc}"))
    }
}

pub fn run_debug<R: Read, W: Write>(src: &str, mut input: R, mut output: W) -> Result<(), String> {
    let program = DebugProgram::parse(src)?;
    let mut debugger = Debugger {
        vm: DebugVm::new(program),
    };
    debugger.repl(&mut input, &mut output)
}

struct Debugger {
    vm: DebugVm,
}

impl Debugger {
    fn repl<R: Read, W: Write>(&mut self, input: &mut R, output: &mut W) -> Result<(), String> {
        writeln!(
            output,
            "sanei debug mode. Type `help` for commands. Ctrl-C pauses continue."
        )
        .map_err(|err| format!("failed to write output: {err}"))?;
        let mut last_command = String::new();
        loop {
            write!(output, "sane-db> ").map_err(|err| format!("failed to write output: {err}"))?;
            output
                .flush()
                .map_err(|err| format!("failed to flush output: {err}"))?;
            let Some(line) =
                read_line(input).map_err(|err| format!("failed to read input: {err}"))?
            else {
                return Ok(());
            };
            INTERRUPTED.store(false, Ordering::SeqCst);
            let command = line.trim();
            let command = if command.is_empty() {
                last_command.as_str()
            } else {
                last_command = command.to_string();
                command
            };
            if !self.handle_command(command, input, output)? {
                return Ok(());
            }
        }
    }

    fn handle_command<R: Read, W: Write>(
        &mut self,
        command: &str,
        input: &mut R,
        output: &mut W,
    ) -> Result<bool, String> {
        let mut parts = command.split_whitespace();
        let Some(name) = parts.next() else {
            return Ok(true);
        };
        match name {
            "help" => self.help(output)?,
            "r" => {
                self.vm.restart();
                writeln!(output, "restarted")
                    .map_err(|err| format!("failed to write output: {err}"))?;
            }
            "c" => self.continue_run(input, output)?,
            "s" => {
                let count = parse_optional_usize(parts.next(), 1)?;
                self.step_count(count, input, output)?;
            }
            "info" => self.info(output)?,
            "next" => {
                let target = parts
                    .next()
                    .ok_or_else(|| "next requires a BF instruction".to_string())?;
                let inst = parse_inst_arg(target)?;
                self.next_inst(inst, input, output)?;
            }
            "b" => {
                let pc = parse_required_usize(parts.next(), "break requires an instruction index")?;
                self.vm.breakpoints.insert(pc);
                writeln!(output, "breakpoint set at {pc}")
                    .map_err(|err| format!("failed to write output: {err}"))?;
            }
            "delete" => {
                let pc =
                    parse_required_usize(parts.next(), "delete requires an instruction index")?;
                if self.vm.breakpoints.remove(&pc) {
                    writeln!(output, "breakpoint deleted at {pc}")
                } else {
                    writeln!(output, "no breakpoint at {pc}")
                }
                .map_err(|err| format!("failed to write output: {err}"))?;
            }
            "breakpoints" => self.breakpoints(output)?,
            "watch" => {
                let target = parts
                    .next()
                    .ok_or_else(|| "watch requires a cell index or symbol name".to_string())?;
                let address = self.resolve_address(target)?;
                self.watch_address(address, output)?;
            }
            "unwatch" => {
                let target = parts
                    .next()
                    .ok_or_else(|| "unwatch requires a cell index or symbol name".to_string())?;
                let address = self.resolve_address(target)?;
                self.unwatch_address(address, output)?;
            }
            "watchpoints" => self.watchpoints(output)?,
            "set" => {
                let target = parts
                    .next()
                    .ok_or_else(|| "set requires a cell index or symbol name".to_string())?;
                let value = parse_byte_arg(parts.next())?;
                let cell = self.resolve_single_cell(target, "set")?;
                self.set_cell(cell, value, output)?;
            }
            "pc" => writeln!(output, "pc={}", self.vm.pc)
                .map_err(|err| format!("failed to write output: {err}"))?,
            "inst" => self.inst(output)?,
            "code" => {
                let radius = parse_optional_usize(parts.next(), 5)?;
                self.code(radius, output)?;
            }
            command if command == "x" || command.starts_with("x/") => {
                let format = ExamineFormat::parse(command.strip_prefix("x/"))?;
                let target = parts
                    .next()
                    .ok_or_else(|| "x requires a cell index or symbol name".to_string())?;
                self.examine(target, format, output)?;
            }
            "symbols" => self.symbols(output)?,
            "symbol" => {
                let symbol = parts
                    .next()
                    .ok_or_else(|| "symbol requires a name".to_string())?;
                self.symbol(symbol, output)?;
            }
            "q" => return Ok(false),
            other => writeln!(output, "unknown command `{other}`")
                .map_err(|err| format!("failed to write output: {err}"))?,
        }
        Ok(true)
    }

    fn help<W: Write>(&self, output: &mut W) -> Result<(), String> {
        write!(
            output,
            "\
Commands:
  r                  Restart program
  c                  Continue until breakpoint, Ctrl-C, or halt
  s [n]              Step n raw BF instructions
  info               Show pc, instruction, pointer, and breakpoint counts
  next <inst>        Continue until next matching BF instruction
  b <pc>             Set breakpoint at instruction index
  delete <pc>        Delete breakpoint
  breakpoints        List breakpoints
  watch <addr|sym>   Stop when a cell changes
  unwatch <addr|sym> Remove watchpoint by cell or symbol
  watchpoints        List watchpoints
  set <addr|sym> <v> Set one tape cell
  pc                 Show current instruction index
  inst               Show current instruction
  code [n]           Show instructions around pc
  x[/FMT] <addr|sym> Examine tape. FMT is count plus d x c and b h w g
  symbols            Show Sane symbols from sanec -s
  symbol <name>      Show one symbol
  q                  Exit debugger
"
        )
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn continue_run<R: Read, W: Write>(
        &mut self,
        input: &mut R,
        output: &mut W,
    ) -> Result<(), String> {
        let mut first = true;
        loop {
            if self.vm.halted {
                writeln!(output, "halted pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            if INTERRUPTED.swap(false, Ordering::SeqCst) {
                writeln!(output, "paused pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            if !first && self.vm.breakpoints.contains(&self.vm.pc) {
                writeln!(output, "breakpoint pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            first = false;
            self.vm.step(input, output)?;
            if self.check_watchpoints(output)? {
                return Ok(());
            }
        }
    }

    fn step_count<R: Read, W: Write>(
        &mut self,
        count: usize,
        input: &mut R,
        output: &mut W,
    ) -> Result<(), String> {
        for _ in 0..count {
            if self.vm.halted {
                break;
            }
            self.vm.step(input, output)?;
            if self.check_watchpoints(output)? {
                break;
            }
        }
        self.status(output)
    }

    fn next_inst<R: Read, W: Write>(
        &mut self,
        target: Inst,
        input: &mut R,
        output: &mut W,
    ) -> Result<(), String> {
        let mut first = true;
        loop {
            if self.vm.halted {
                writeln!(output, "halted pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            if !first && self.vm.program.insts.get(self.vm.pc).copied() == Some(target) {
                writeln!(output, "next pc={} inst='{}'", self.vm.pc, target.as_char())
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            if INTERRUPTED.swap(false, Ordering::SeqCst) {
                writeln!(output, "paused pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            if !first && self.vm.breakpoints.contains(&self.vm.pc) {
                writeln!(output, "breakpoint pc={}", self.vm.pc)
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(());
            }
            first = false;
            self.vm.step(input, output)?;
            if self.check_watchpoints(output)? {
                return Ok(());
            }
        }
    }

    fn info<W: Write>(&self, output: &mut W) -> Result<(), String> {
        let inst = self
            .vm
            .program
            .insts
            .get(self.vm.pc)
            .copied()
            .map(Inst::as_char)
            .unwrap_or(' ');
        let state = if self.vm.halted { "halted" } else { "paused" };
        writeln!(
            output,
            "pc={} inst='{}' ptr={} cell[{}]={} state={} breakpoints={} watchpoints={}",
            self.vm.pc,
            inst,
            self.vm.ptr,
            self.vm.ptr,
            self.vm.tape[self.vm.ptr],
            state,
            self.vm.breakpoints.len(),
            self.vm.watchpoints.len()
        )
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn status<W: Write>(&self, output: &mut W) -> Result<(), String> {
        let inst = self
            .vm
            .program
            .insts
            .get(self.vm.pc)
            .copied()
            .map(Inst::as_char)
            .unwrap_or(' ');
        let state = if self.vm.halted { "halted" } else { "paused" };
        writeln!(
            output,
            "pc={} inst='{}' ptr={} cell[{}]={} state={}",
            self.vm.pc, inst, self.vm.ptr, self.vm.ptr, self.vm.tape[self.vm.ptr], state
        )
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn breakpoints<W: Write>(&self, output: &mut W) -> Result<(), String> {
        if self.vm.breakpoints.is_empty() {
            writeln!(output, "no breakpoints")
        } else {
            for pc in &self.vm.breakpoints {
                writeln!(output, "breakpoint pc={pc}")
                    .map_err(|err| format!("failed to write output: {err}"))?;
            }
            Ok(())
        }
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn watchpoints<W: Write>(&self, output: &mut W) -> Result<(), String> {
        if self.vm.watchpoints.is_empty() {
            writeln!(output, "no watchpoints")
        } else {
            for (cell, value) in &self.vm.watchpoints {
                writeln!(output, "watchpoint cell={cell} value={value}")
                    .map_err(|err| format!("failed to write output: {err}"))?;
            }
            Ok(())
        }
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn check_watchpoints<W: Write>(&mut self, output: &mut W) -> Result<bool, String> {
        for (cell, old) in self.vm.watchpoints.clone() {
            let new = *self
                .vm
                .tape
                .get(cell)
                .ok_or_else(|| format!("cell {cell} is out of bounds"))?;
            if new != old {
                self.vm.watchpoints.insert(cell, new);
                writeln!(output, "watchpoint cell={cell} old={old} new={new}")
                    .map_err(|err| format!("failed to write output: {err}"))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn inst<W: Write>(&self, output: &mut W) -> Result<(), String> {
        if let Some(inst) = self.vm.program.insts.get(self.vm.pc) {
            writeln!(output, "inst pc={} op='{}'", self.vm.pc, inst.as_char())
        } else {
            writeln!(output, "inst pc={} op=<halt>", self.vm.pc)
        }
        .map_err(|err| format!("failed to write output: {err}"))
    }

    fn code<W: Write>(&self, radius: usize, output: &mut W) -> Result<(), String> {
        let start = self.vm.pc.saturating_sub(radius);
        let end = (self.vm.pc + radius + 1).min(self.vm.program.insts.len());
        for pc in start..end {
            let marker = if pc == self.vm.pc { "=>" } else { "  " };
            let breakpoint = if self.vm.breakpoints.contains(&pc) {
                "B"
            } else {
                " "
            };
            writeln!(
                output,
                "{marker} {breakpoint} {pc:>6} {}",
                self.vm.program.insts[pc].as_char()
            )
            .map_err(|err| format!("failed to write output: {err}"))?;
        }
        Ok(())
    }

    fn examine<W: Write>(
        &self,
        target: &str,
        format: ExamineFormat,
        output: &mut W,
    ) -> Result<(), String> {
        let address = self.resolve_address(target)?;
        let (start, default_len) = address.range();
        let count = format
            .count
            .unwrap_or(default_len.div_ceil(format.size.bytes()));
        for item in 0..count {
            let cell = start
                .checked_add(item * format.size.bytes())
                .ok_or_else(|| "examine range is too large".to_string())?;
            let value = self.read_sized_value(cell, format.size.bytes())?;
            writeln!(
                output,
                "{cell} {}",
                format.kind.format(value, format.size.bytes())
            )
            .map_err(|err| format!("failed to write output: {err}"))?;
        }
        Ok(())
    }

    fn read_sized_value(&self, start: usize, size: usize) -> Result<u64, String> {
        let end = start
            .checked_add(size)
            .ok_or_else(|| "examine range is too large".to_string())?;
        if end > self.vm.tape.len() {
            return Err(format!("cell {start} is out of bounds"));
        }
        let mut value = 0u64;
        for (shift, byte) in self.vm.tape[start..end].iter().copied().enumerate() {
            value |= u64::from(byte) << (shift * 8);
        }
        Ok(value)
    }

    fn set_cell<W: Write>(&mut self, cell: usize, value: u8, output: &mut W) -> Result<(), String> {
        let target = self
            .vm
            .tape
            .get_mut(cell)
            .ok_or_else(|| format!("cell {cell} is out of bounds"))?;
        *target = value;
        if let Some(watched) = self.vm.watchpoints.get_mut(&cell) {
            *watched = value;
        }
        writeln!(output, "cell[{cell}]={value}")
            .map_err(|err| format!("failed to write output: {err}"))
    }

    fn symbols<W: Write>(&self, output: &mut W) -> Result<(), String> {
        if self.vm.program.symbols.is_empty() {
            writeln!(output, "no symbols")
                .map_err(|err| format!("failed to write output: {err}"))?;
            return Ok(());
        }
        for symbol in &self.vm.program.symbols {
            write_symbol(symbol, output)?;
        }
        Ok(())
    }

    fn symbol<W: Write>(&self, name: &str, output: &mut W) -> Result<(), String> {
        let Some(symbol) = self
            .vm
            .program
            .symbols
            .iter()
            .find(|symbol| symbol.name() == name)
        else {
            writeln!(output, "symbol `{name}` not found")
                .map_err(|err| format!("failed to write output: {err}"))?;
            return Ok(());
        };
        write_symbol(symbol, output)
    }

    fn watch_address<W: Write>(&mut self, address: Address, output: &mut W) -> Result<(), String> {
        let (start, len) = address.range();
        for cell in start..start + len {
            let value = *self
                .vm
                .tape
                .get(cell)
                .ok_or_else(|| format!("cell {cell} is out of bounds"))?;
            self.vm.watchpoints.insert(cell, value);
            writeln!(output, "watchpoint cell={cell} value={value}")
                .map_err(|err| format!("failed to write output: {err}"))?;
        }
        Ok(())
    }

    fn unwatch_address<W: Write>(
        &mut self,
        address: Address,
        output: &mut W,
    ) -> Result<(), String> {
        let (start, len) = address.range();
        let mut removed = 0usize;
        for cell in start..start + len {
            if self.vm.watchpoints.remove(&cell).is_some() {
                removed += 1;
            }
        }
        writeln!(output, "watchpoints deleted {removed}")
            .map_err(|err| format!("failed to write output: {err}"))
    }

    fn resolve_single_cell(&self, target: &str, command: &str) -> Result<usize, String> {
        let address = self.resolve_address(target)?;
        if address.len != 1 {
            return Err(format!("{command} requires an indexed array element"));
        }
        Ok(address.start)
    }

    fn resolve_address(&self, target: &str) -> Result<Address, String> {
        if let Ok(cell) = target.parse() {
            return Ok(Address {
                start: cell,
                len: 1,
            });
        }
        if let Some((name, index)) = parse_indexed_symbol(target)? {
            let symbol = self.lookup_symbol(name)?;
            return resolve_symbol_offset(symbol, index);
        }
        if let Some((name, offset)) = parse_offset_symbol(target)? {
            let symbol = self.lookup_symbol(name)?;
            return resolve_symbol_offset(symbol, offset);
        }
        self.lookup_symbol(target).map(DebugSymbol::address)
    }

    fn lookup_symbol(&self, name: &str) -> Result<&DebugSymbol, String> {
        self.vm
            .program
            .symbols
            .iter()
            .find(|symbol| symbol.name() == name)
            .ok_or_else(|| format!("unknown cell or symbol `{name}`"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Address {
    start: usize,
    len: usize,
}

impl Address {
    fn range(self) -> (usize, usize) {
        (self.start, self.len)
    }
}

fn parse_indexed_symbol(target: &str) -> Result<Option<(&str, usize)>, String> {
    let Some(open) = target.find('[') else {
        return Ok(None);
    };
    if !target.ends_with(']') {
        return Err(format!("invalid indexed symbol `{target}`"));
    }
    let name = &target[..open];
    let index = &target[open + 1..target.len() - 1];
    if name.is_empty() || index.is_empty() {
        return Err(format!("invalid indexed symbol `{target}`"));
    }
    let index = index
        .parse()
        .map_err(|_| format!("invalid array index `{index}`"))?;
    Ok(Some((name, index)))
}

fn parse_offset_symbol(target: &str) -> Result<Option<(&str, usize)>, String> {
    let Some((name, offset)) = target.split_once('+') else {
        return Ok(None);
    };
    if name.is_empty() || offset.is_empty() {
        return Err(format!("invalid symbol offset `{target}`"));
    }
    let offset = offset
        .parse()
        .map_err(|_| format!("invalid symbol offset `{offset}`"))?;
    Ok(Some((name, offset)))
}

fn resolve_symbol_offset(symbol: &DebugSymbol, offset: usize) -> Result<Address, String> {
    match symbol {
        DebugSymbol::Cell { name, cell } => {
            if offset == 0 {
                Ok(Address {
                    start: *cell,
                    len: 1,
                })
            } else {
                Err(format!(
                    "scalar symbol `{name}` does not support offset {offset}"
                ))
            }
        }
        DebugSymbol::Array {
            name, first, len, ..
        } => {
            if offset >= *len {
                Err(format!(
                    "array index {offset} is out of bounds for `{name}` length {len}"
                ))
            } else {
                Ok(Address {
                    start: first + offset,
                    len: 1,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExamineFormat {
    count: Option<usize>,
    kind: ExamineKind,
    size: ExamineSize,
}

impl ExamineFormat {
    fn parse(spec: Option<&str>) -> Result<Self, String> {
        let Some(spec) = spec else {
            return Ok(Self::default());
        };
        let mut digits = String::new();
        let mut kind = None;
        let mut size = None;
        for ch in spec.chars() {
            if ch.is_ascii_digit() {
                if kind.is_some() || size.is_some() {
                    return Err(format!("invalid x format `{spec}`"));
                }
                digits.push(ch);
            } else if let Some(parsed) = ExamineKind::parse(ch) {
                kind = Some(parsed);
            } else if let Some(parsed) = ExamineSize::parse(ch) {
                size = Some(parsed);
            } else {
                return Err(format!("invalid x format character `{ch}`"));
            }
        }
        let count = if digits.is_empty() {
            None
        } else {
            Some(
                digits
                    .parse()
                    .map_err(|_| format!("invalid x count `{digits}`"))?,
            )
        };
        Ok(Self {
            count,
            kind: kind.unwrap_or(ExamineKind::Hex),
            size: size.unwrap_or(ExamineSize::Byte),
        })
    }
}

impl Default for ExamineFormat {
    fn default() -> Self {
        Self {
            count: None,
            kind: ExamineKind::Hex,
            size: ExamineSize::Byte,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExamineKind {
    Decimal,
    Hex,
    Char,
}

impl ExamineKind {
    fn parse(ch: char) -> Option<Self> {
        match ch {
            'd' => Some(Self::Decimal),
            'x' => Some(Self::Hex),
            'c' => Some(Self::Char),
            _ => None,
        }
    }

    fn format(self, value: u64, size: usize) -> String {
        match self {
            Self::Decimal => value.to_string(),
            Self::Hex => format!("0x{value:0width$x}", width = size * 2),
            Self::Char => {
                let byte = value as u8;
                if byte.is_ascii_graphic() || byte == b' ' {
                    format!("'{ch}'", ch = char::from(byte))
                } else {
                    format!("'\\x{byte:02x}'")
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExamineSize {
    Byte,
    Half,
    Word,
    Giant,
}

impl ExamineSize {
    fn parse(ch: char) -> Option<Self> {
        match ch {
            'b' => Some(Self::Byte),
            'h' => Some(Self::Half),
            'w' => Some(Self::Word),
            'g' => Some(Self::Giant),
            _ => None,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Half => 2,
            Self::Word => 4,
            Self::Giant => 8,
        }
    }
}

fn write_symbol<W: Write>(symbol: &DebugSymbol, output: &mut W) -> Result<(), String> {
    match symbol {
        DebugSymbol::Cell { name, cell } => writeln!(output, "{name} cell {cell}"),
        DebugSymbol::Array {
            name,
            base,
            len,
            first,
            last,
        } => writeln!(
            output,
            "{name} array base {base} len {len} data {first} to {last}"
        ),
    }
    .map_err(|err| format!("failed to write output: {err}"))
}

fn build_jumps(insts: &[Inst]) -> Result<Vec<Option<usize>>, String> {
    let mut jumps = vec![None; insts.len()];
    let mut stack = Vec::new();
    for (pc, inst) in insts.iter().enumerate() {
        match inst {
            Inst::LoopStart => stack.push(pc),
            Inst::LoopEnd => {
                let Some(start) = stack.pop() else {
                    return Err(format!("unmatched `]` at instruction {pc}"));
                };
                jumps[start] = Some(pc);
                jumps[pc] = Some(start);
            }
            _ => {}
        }
    }
    if let Some(pc) = stack.pop() {
        Err(format!("unmatched `[` at instruction {pc}"))
    } else {
        Ok(jumps)
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

fn read_line<R: Read>(input: &mut R) -> std::io::Result<Option<String>> {
    let mut line = Vec::new();
    let mut byte = [0];
    loop {
        match input.read(&mut byte) {
            Err(err)
                if err.kind() == ErrorKind::Interrupted && INTERRUPTED.load(Ordering::SeqCst) =>
            {
                return Ok(Some(String::new()));
            }
            Err(err) => return Err(err),
            Ok(0) if line.is_empty() => return Ok(None),
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => line.push(byte[0]),
        }
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

fn parse_required_usize(value: Option<&str>, message: &str) -> Result<usize, String> {
    value
        .ok_or_else(|| message.to_string())?
        .parse()
        .map_err(|_| message.to_string())
}

fn parse_optional_usize(value: Option<&str>, default: usize) -> Result<usize, String> {
    match value {
        Some(value) => value
            .parse()
            .map_err(|_| format!("expected unsigned integer, found `{value}`")),
        None => Ok(default),
    }
}

fn parse_byte_arg(value: Option<&str>) -> Result<u8, String> {
    let value = value.ok_or_else(|| "set requires a byte value".to_string())?;
    value
        .parse()
        .map_err(|_| format!("expected byte value, found `{value}`"))
}

fn parse_inst_arg(value: &str) -> Result<Inst, String> {
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err("next requires a BF instruction".to_string());
    };
    if chars.next().is_some() {
        return Err(format!("expected one BF instruction, found `{value}`"));
    }
    Inst::from_char(ch).ok_or_else(|| format!("expected BF instruction, found `{value}`"))
}

fn parse_symbols(src: &str) -> Vec<DebugSymbol> {
    let mut in_symbols = false;
    let mut symbols = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if line == "SANE SYMBOLS" {
            in_symbols = true;
            continue;
        }
        if line == "END SANE SYMBOLS" {
            break;
        }
        if !in_symbols {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            [name, "CELL", cell] => {
                if let Ok(cell) = cell.parse() {
                    symbols.push(DebugSymbol::Cell {
                        name: (*name).to_string(),
                        cell,
                    });
                }
            }
            [
                name,
                "ARRAY",
                "BASE",
                base,
                "LEN",
                len,
                "DATA",
                "CELLS",
                first,
                "TO",
                last,
            ] => {
                let parsed = base
                    .parse()
                    .ok()
                    .zip(len.parse().ok())
                    .zip(first.parse().ok())
                    .zip(last.parse().ok());
                if let Some((((base, len), first), last)) = parsed {
                    symbols.push(DebugSymbol::Array {
                        name: (*name).to_string(),
                        base,
                        len,
                        first,
                        last,
                    });
                }
            }
            _ => {}
        }
    }
    symbols
}
