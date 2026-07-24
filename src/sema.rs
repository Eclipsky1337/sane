use std::collections::HashMap;

use crate::ast::{ArrayInit, BinOp, Expr, Function, Program, ReturnType, Stmt, UnOp};
use crate::diagnostic::{Diagnostic, Span};

pub const TEMP_COUNT: usize = 8;
pub const WORK_CELL_COUNT: usize = 12;
pub const MAX_CALL_DEPTH: usize = 16;

#[derive(Debug, Clone)]
pub struct Symbols {
    cells: HashMap<String, usize>,
    entries: Vec<SymbolInfo>,
    scratch_base: usize,
}

impl Symbols {
    pub fn cell(&self, name: &str) -> Option<usize> {
        self.cells.get(name).copied()
    }

    pub fn scratch_base(&self) -> usize {
        self.scratch_base
    }

    pub fn control_base(&self) -> usize {
        self.scratch_base + WORK_CELL_COUNT
    }

    pub fn entries(&self) -> &[SymbolInfo] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolInfo {
    Scalar {
        name: String,
        cell: usize,
    },
    Array {
        name: String,
        base: usize,
        len: usize,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    pub functions: Vec<ResolvedFunction>,
    pub stmts: Vec<ResolvedStmt>,
    pub spills: Vec<usize>,
    pub symbols: Symbols,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFunction {
    pub name: String,
    pub params: Vec<usize>,
    pub return_type: ReturnType,
    pub body: Vec<ResolvedStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedStmt {
    Let {
        cell: usize,
        init: Option<ResolvedExpr>,
    },
    LetArray {
        base: usize,
        len: usize,
        init: Vec<u8>,
    },
    Assign {
        cell: usize,
        expr: ResolvedExpr,
    },
    ArraySet {
        base: usize,
        len: usize,
        index: ResolvedExpr,
        expr: ResolvedExpr,
    },
    Put(ResolvedExpr),
    Puts(Vec<u8>),
    Print(ResolvedExpr),
    Println(ResolvedExpr),
    Read(usize),
    ReadArray {
        base: usize,
        len: usize,
        index: ResolvedExpr,
    },
    Call {
        function: usize,
        args: Vec<ResolvedExpr>,
    },
    Return(Option<ResolvedExpr>),
    Break,
    Continue,
    Block(Vec<ResolvedStmt>),
    If {
        cond: ResolvedExpr,
        then_branch: Vec<ResolvedStmt>,
        else_branch: Vec<ResolvedStmt>,
    },
    While {
        cond: ResolvedExpr,
        body: Vec<ResolvedStmt>,
    },
    Loop {
        body: Vec<ResolvedStmt>,
    },
    For {
        init: Option<Box<ResolvedStmt>>,
        cond: Option<ResolvedExpr>,
        step: Option<Box<ResolvedStmt>>,
        body: Vec<ResolvedStmt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedExpr {
    Byte(u8),
    Var(usize),
    ArrayGet {
        base: usize,
        len: usize,
        index: Box<ResolvedExpr>,
    },
    Call {
        function: usize,
        args: Vec<ResolvedExpr>,
    },
    Unary {
        op: UnOp,
        expr: Box<ResolvedExpr>,
    },
    Binary {
        left: Box<ResolvedExpr>,
        op: BinOp,
        right: Box<ResolvedExpr>,
    },
}

pub fn analyze(program: &Program) -> Result<Symbols, Diagnostic> {
    Ok(resolve(program)?.symbols)
}

pub fn resolve(program: &Program) -> Result<ResolvedProgram, Diagnostic> {
    let mut resolver = Resolver::new();
    resolver.define_functions(&program.functions)?;
    let stmts = resolver.resolve_stmts(&program.stmts, 0)?;
    let functions = resolver.resolve_functions(&program.functions)?;
    resolver.check_call_graph()?;
    resolver.check_call_depth()?;
    let spill_count = count_stmt_calls(&stmts)
        + functions
            .iter()
            .map(|function| count_stmt_calls(&function.body))
            .sum::<usize>();
    let spills = (0..spill_count)
        .map(|_| resolver.alloc_fresh_cell())
        .collect::<Vec<_>>();
    Ok(ResolvedProgram {
        functions,
        stmts,
        spills,
        symbols: Symbols {
            cells: resolver.public_cells,
            entries: resolver.public_entries,
            scratch_base: resolver.high_water,
        },
    })
}

#[derive(Debug, Default)]
struct Scope {
    names: HashMap<String, Symbol>,
    owned_cells: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Scalar(usize),
    Array { base: usize, len: usize },
}

struct Resolver {
    scopes: Vec<Scope>,
    public_cells: HashMap<String, usize>,
    public_entries: Vec<SymbolInfo>,
    functions: HashMap<String, usize>,
    function_names: Vec<String>,
    function_arities: Vec<usize>,
    function_return_types: Vec<ReturnType>,
    call_graph: Vec<Vec<usize>>,
    current_function: Option<usize>,
    free_cells: Vec<usize>,
    next_cell: usize,
    high_water: usize,
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: vec![Scope::default()],
            public_cells: HashMap::new(),
            public_entries: Vec::new(),
            functions: HashMap::new(),
            function_names: Vec::new(),
            function_arities: Vec::new(),
            function_return_types: Vec::new(),
            call_graph: Vec::new(),
            current_function: None,
            free_cells: Vec::new(),
            next_cell: TEMP_COUNT,
            high_water: TEMP_COUNT,
        }
    }

    fn define_functions(&mut self, functions: &[Function]) -> Result<(), Diagnostic> {
        for function in functions {
            if self.functions.contains_key(&function.name) {
                return Err(Diagnostic::new(
                    format!("function `{}` already declared", function.name),
                    function.name_span,
                ));
            }
            let index = self.function_names.len();
            self.functions.insert(function.name.clone(), index);
            self.function_names.push(function.name.clone());
            self.function_arities.push(function.params.len());
            self.function_return_types.push(function.return_type);
            self.call_graph.push(Vec::new());
        }
        Ok(())
    }

    fn resolve_functions(
        &mut self,
        functions: &[Function],
    ) -> Result<Vec<ResolvedFunction>, Diagnostic> {
        let mut resolved = Vec::new();
        for (index, function) in functions.iter().enumerate() {
            self.push_scope();
            let mut params = Vec::new();
            for param in &function.params {
                if self.current_scope().names.contains_key(&param.name) {
                    return Err(Diagnostic::new(
                        format!("parameter `{}` already declared", param.name),
                        param.name_span,
                    ));
                }
                let cell = self.alloc_fresh_cell();
                self.current_scope_mut()
                    .names
                    .insert(param.name.clone(), Symbol::Scalar(cell));
                self.current_scope_mut().owned_cells.push(cell);
                params.push(cell);
            }
            let previous = self.current_function.replace(index);
            let body = self.resolve_stmts(&function.body, 0)?;
            self.current_function = previous;
            self.pop_scope_retain();
            if function.return_type == ReturnType::Byte && !stmts_always_return(&body) {
                return Err(Diagnostic::new(
                    format!(
                        "byte function `{}` may fall through without returning a value",
                        function.name
                    ),
                    function.span,
                ));
            }
            resolved.push(ResolvedFunction {
                name: function.name.clone(),
                params,
                return_type: function.return_type,
                body,
            });
        }
        Ok(resolved)
    }

    fn check_call_graph(&self) -> Result<(), Diagnostic> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Mark {
            Visiting,
            Done,
        }

        fn visit(node: usize, graph: &[Vec<usize>], marks: &mut [Option<Mark>]) -> Result<(), ()> {
            if marks[node] == Some(Mark::Visiting) {
                return Err(());
            }
            if marks[node] == Some(Mark::Done) {
                return Ok(());
            }
            marks[node] = Some(Mark::Visiting);
            for &callee in &graph[node] {
                visit(callee, graph, marks)?;
            }
            marks[node] = Some(Mark::Done);
            Ok(())
        }

        let mut marks = vec![None; self.call_graph.len()];
        for node in 0..self.call_graph.len() {
            if visit(node, &self.call_graph, &mut marks).is_err() {
                return Err(Diagnostic::bare(
                    "recursive function calls are not supported",
                ));
            }
        }
        Ok(())
    }

    fn check_call_depth(&self) -> Result<(), Diagnostic> {
        fn depth(node: usize, graph: &[Vec<usize>], memo: &mut [Option<usize>]) -> usize {
            if let Some(value) = memo[node] {
                return value;
            }
            let value = 1 + graph[node]
                .iter()
                .map(|&callee| depth(callee, graph, memo))
                .max()
                .unwrap_or(0);
            memo[node] = Some(value);
            value
        }

        let mut memo = vec![None; self.call_graph.len()];
        let max_depth = (0..self.call_graph.len())
            .map(|node| depth(node, &self.call_graph, &mut memo))
            .max()
            .unwrap_or(0);
        if max_depth > MAX_CALL_DEPTH {
            return Err(Diagnostic::bare(format!(
                "function call depth {max_depth} exceeds limit {MAX_CALL_DEPTH}"
            )));
        }
        Ok(())
    }

    fn resolve_stmts(
        &mut self,
        stmts: &[Stmt],
        loop_depth: usize,
    ) -> Result<Vec<ResolvedStmt>, Diagnostic> {
        stmts
            .iter()
            .map(|stmt| self.resolve_stmt(stmt, loop_depth))
            .collect()
    }

    fn resolve_scoped_stmts(
        &mut self,
        stmts: &[Stmt],
        loop_depth: usize,
    ) -> Result<Vec<ResolvedStmt>, Diagnostic> {
        self.push_scope();
        let result = self.resolve_stmts(stmts, loop_depth);
        self.pop_scope();
        result
    }

    fn resolve_stmt(&mut self, stmt: &Stmt, loop_depth: usize) -> Result<ResolvedStmt, Diagnostic> {
        match stmt {
            Stmt::Let {
                name,
                name_span,
                init,
                ..
            } => {
                if self.current_scope().names.contains_key(name) {
                    return Err(Diagnostic::new(
                        format!("variable `{name}` already declared in this scope"),
                        *name_span,
                    ));
                }
                let init = init
                    .as_ref()
                    .map(|expr| self.resolve_expr(expr))
                    .transpose()?;
                let cell = self.alloc_scope_cell();
                self.current_scope_mut()
                    .names
                    .insert(name.clone(), Symbol::Scalar(cell));
                self.current_scope_mut().owned_cells.push(cell);
                self.define_public_symbol(
                    name,
                    cell,
                    SymbolInfo::Scalar {
                        name: name.clone(),
                        cell,
                    },
                );
                Ok(ResolvedStmt::Let { cell, init })
            }
            Stmt::LetArray {
                name,
                name_span,
                len,
                init,
                ..
            } => {
                if self.current_scope().names.contains_key(name) {
                    return Err(Diagnostic::new(
                        format!("variable `{name}` already declared in this scope"),
                        *name_span,
                    ));
                }
                let base = self.alloc_scope_block(4 + *len);
                self.current_scope_mut()
                    .names
                    .insert(name.clone(), Symbol::Array { base, len: *len });
                self.current_scope_mut()
                    .owned_cells
                    .extend(base..base + 4 + *len);
                self.define_public_symbol(
                    name,
                    base,
                    SymbolInfo::Array {
                        name: name.clone(),
                        base,
                        len: *len,
                    },
                );
                let init = resolve_array_init(init.as_ref(), *len)?;
                Ok(ResolvedStmt::LetArray {
                    base,
                    len: *len,
                    init,
                })
            }
            Stmt::Assign {
                name,
                name_span,
                expr,
                ..
            } => {
                let cell =
                    self.lookup_scalar(name, *name_span, "assignment to undeclared variable")?;
                Ok(ResolvedStmt::Assign {
                    cell,
                    expr: self.resolve_expr(expr)?,
                })
            }
            Stmt::ArrayAssign {
                name,
                name_span,
                index,
                expr,
                ..
            } => {
                let (base, len) = self.lookup_array(name, *name_span)?;
                check_const_index(index, len)?;
                Ok(ResolvedStmt::ArraySet {
                    base,
                    len,
                    index: self.resolve_expr(index)?,
                    expr: self.resolve_expr(expr)?,
                })
            }
            Stmt::Put(expr, _) => Ok(ResolvedStmt::Put(self.resolve_expr(expr)?)),
            Stmt::Puts(bytes, _) => Ok(ResolvedStmt::Puts(bytes.clone())),
            Stmt::Print(expr, _) => Ok(ResolvedStmt::Print(self.resolve_expr(expr)?)),
            Stmt::Println(expr, _) => Ok(ResolvedStmt::Println(self.resolve_expr(expr)?)),
            Stmt::Read(name, span) => Ok(ResolvedStmt::Read(self.lookup_scalar(
                name,
                *span,
                "read into undeclared variable",
            )?)),
            Stmt::ReadArray {
                name,
                name_span,
                index,
                ..
            } => {
                let (base, len) = self.lookup_array(name, *name_span)?;
                check_const_index(index, len)?;
                Ok(ResolvedStmt::ReadArray {
                    base,
                    len,
                    index: self.resolve_expr(index)?,
                })
            }
            Stmt::Call {
                name,
                name_span,
                args,
                ..
            } => {
                let (function, args) = self.resolve_call(name, *name_span, args)?;
                Ok(ResolvedStmt::Call { function, args })
            }
            Stmt::Return(expr, span) => {
                let Some(function) = self.current_function else {
                    return Err(Diagnostic::new("`return` outside function", *span));
                };
                match (self.function_return_types[function], expr) {
                    (ReturnType::Void, None) => Ok(ResolvedStmt::Return(None)),
                    (ReturnType::Void, Some(_)) => Err(Diagnostic::new(
                        "void function cannot return a value",
                        *span,
                    )),
                    (ReturnType::Byte, None) => {
                        Err(Diagnostic::new("byte function must return a value", *span))
                    }
                    (ReturnType::Byte, Some(expr)) => {
                        Ok(ResolvedStmt::Return(Some(self.resolve_expr(expr)?)))
                    }
                }
            }
            Stmt::Break(span) => {
                if loop_depth == 0 {
                    return Err(Diagnostic::new("`break` outside loop", *span));
                }
                Ok(ResolvedStmt::Break)
            }
            Stmt::Continue(span) => {
                if loop_depth == 0 {
                    return Err(Diagnostic::new("`continue` outside loop", *span));
                }
                Ok(ResolvedStmt::Continue)
            }
            Stmt::Block(stmts, _) => Ok(ResolvedStmt::Block(
                self.resolve_scoped_stmts(stmts, loop_depth)?,
            )),
            Stmt::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => Ok(ResolvedStmt::If {
                cond: self.resolve_expr(cond)?,
                then_branch: self.resolve_scoped_stmts(then_branch, loop_depth)?,
                else_branch: self.resolve_scoped_stmts(else_branch, loop_depth)?,
            }),
            Stmt::While { cond, body, .. } => Ok(ResolvedStmt::While {
                cond: self.resolve_expr(cond)?,
                body: self.resolve_scoped_stmts(body, loop_depth + 1)?,
            }),
            Stmt::Loop { body, .. } => Ok(ResolvedStmt::Loop {
                body: self.resolve_scoped_stmts(body, loop_depth + 1)?,
            }),
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.push_scope();
                let init = init
                    .as_ref()
                    .map(|stmt| self.resolve_for_header_stmt(stmt, loop_depth))
                    .transpose()?
                    .map(Box::new);
                let cond = cond
                    .as_ref()
                    .map(|expr| self.resolve_expr(expr))
                    .transpose()?;
                let step = step
                    .as_ref()
                    .map(|stmt| self.resolve_for_header_stmt(stmt, loop_depth))
                    .transpose()?
                    .map(Box::new);
                let body = self.resolve_scoped_stmts(body, loop_depth + 1)?;
                self.pop_scope();
                Ok(ResolvedStmt::For {
                    init,
                    cond,
                    step,
                    body,
                })
            }
        }
    }

    fn resolve_for_header_stmt(
        &mut self,
        stmt: &Stmt,
        loop_depth: usize,
    ) -> Result<ResolvedStmt, Diagnostic> {
        match stmt {
            Stmt::Let { .. } | Stmt::Assign { .. } | Stmt::ArrayAssign { .. } => {
                self.resolve_stmt(stmt, loop_depth)
            }
            _ => Err(Diagnostic::bare(
                "for header only supports let and assignment",
            )),
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) -> Result<ResolvedExpr, Diagnostic> {
        match expr {
            Expr::Byte(value, _) => Ok(ResolvedExpr::Byte(*value)),
            Expr::Var(name, span) => Ok(ResolvedExpr::Var(self.lookup_scalar(
                name,
                *span,
                "use of undeclared variable",
            )?)),
            Expr::ArrayGet {
                name,
                name_span,
                index,
                ..
            } => {
                let (base, len) = self.lookup_array(name, *name_span)?;
                check_const_index(index, len)?;
                Ok(ResolvedExpr::ArrayGet {
                    base,
                    len,
                    index: Box::new(self.resolve_expr(index)?),
                })
            }
            Expr::Call {
                name,
                name_span,
                args,
                ..
            } => {
                let (function, args) = self.resolve_call(name, *name_span, args)?;
                if self.function_return_types[function] == ReturnType::Void {
                    return Err(Diagnostic::new(
                        format!("void function `{name}` cannot be used as a value"),
                        *name_span,
                    ));
                }
                Ok(ResolvedExpr::Call { function, args })
            }
            Expr::Unary { op, expr, .. } => Ok(ResolvedExpr::Unary {
                op: *op,
                expr: Box::new(self.resolve_expr(expr)?),
            }),
            Expr::Binary {
                left, op, right, ..
            } => Ok(ResolvedExpr::Binary {
                left: Box::new(self.resolve_expr(left)?),
                op: *op,
                right: Box::new(self.resolve_expr(right)?),
            }),
        }
    }

    fn resolve_call(
        &mut self,
        name: &str,
        name_span: Span,
        args: &[Expr],
    ) -> Result<(usize, Vec<ResolvedExpr>), Diagnostic> {
        let function = self.functions.get(name).copied().ok_or_else(|| {
            Diagnostic::new(format!("call to undeclared function `{name}`"), name_span)
        })?;
        if args.len() != self.function_arities[function] {
            return Err(Diagnostic::new(
                format!(
                    "function `{name}` expects {} arguments, got {}",
                    self.function_arities[function],
                    args.len()
                ),
                name_span,
            ));
        }
        if let Some(caller) = self.current_function {
            if !self.call_graph[caller].contains(&function) {
                self.call_graph[caller].push(function);
            }
        }
        let args = args
            .iter()
            .map(|arg| self.resolve_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((function, args))
    }

    fn lookup_symbol(&self, name: &str, span: Span, prefix: &str) -> Result<Symbol, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.names.get(name).copied())
            .ok_or_else(|| Diagnostic::new(format!("{prefix} `{name}`"), span))
    }

    fn lookup_scalar(&self, name: &str, span: Span, prefix: &str) -> Result<usize, Diagnostic> {
        match self.lookup_symbol(name, span, prefix)? {
            Symbol::Scalar(cell) => Ok(cell),
            Symbol::Array { .. } => Err(Diagnostic::new(format!("`{name}` is an array"), span)),
        }
    }

    fn lookup_array(&self, name: &str, span: Span) -> Result<(usize, usize), Diagnostic> {
        match self.lookup_symbol(name, span, "use of undeclared array")? {
            Symbol::Array { base, len } => Ok((base, len)),
            Symbol::Scalar(_) => Err(Diagnostic::new(format!("`{name}` is not an array"), span)),
        }
    }

    fn alloc_scope_cell(&mut self) -> usize {
        if self.current_function.is_some() {
            return self.alloc_fresh_cell();
        }
        if let Some(cell) = self.free_cells.pop() {
            cell
        } else {
            self.alloc_fresh_cell()
        }
    }

    fn alloc_fresh_cell(&mut self) -> usize {
        let cell = self.next_cell;
        self.next_cell += 1;
        self.high_water = self.high_water.max(self.next_cell);
        cell
    }

    fn alloc_scope_block(&mut self, len: usize) -> usize {
        if self.current_function.is_some() {
            return self.alloc_fresh_block(len);
        }
        self.alloc_fresh_block(len)
    }

    fn alloc_fresh_block(&mut self, len: usize) -> usize {
        let base = self.next_cell;
        self.next_cell += len;
        self.high_water = self.high_water.max(self.next_cell);
        base
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        let scope = self.scopes.pop().expect("resolver always has a scope");
        if self.current_function.is_none() {
            self.free_cells.extend(scope.owned_cells);
            self.free_cells.sort_by(|a, b| b.cmp(a));
        }
    }

    fn pop_scope_retain(&mut self) {
        self.scopes.pop().expect("resolver always has a scope");
    }

    fn current_scope(&self) -> &Scope {
        self.scopes.last().expect("resolver always has a scope")
    }

    fn current_scope_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("resolver always has a scope")
    }

    fn define_public_symbol(&mut self, name: &str, cell: usize, info: SymbolInfo) {
        if self.scopes.len() != 1 {
            return;
        }
        if self.public_cells.contains_key(name) {
            return;
        }
        self.public_cells.insert(name.to_string(), cell);
        self.public_entries.push(info);
    }
}

fn check_const_index(index: &Expr, len: usize) -> Result<(), Diagnostic> {
    if let Expr::Byte(value, span) = index {
        if usize::from(*value) >= len {
            return Err(Diagnostic::new(
                format!("array index {value} is out of bounds for length {len}"),
                *span,
            ));
        }
    }
    Ok(())
}

fn resolve_array_init(init: Option<&ArrayInit>, len: usize) -> Result<Vec<u8>, Diagnostic> {
    let Some(init) = init else {
        return Ok(Vec::new());
    };

    match init {
        ArrayInit::Bytes(bytes, span) => {
            if bytes.len() != len {
                return Err(Diagnostic::new(
                    format!(
                        "array initializer has {} elements, but length is {len}",
                        bytes.len()
                    ),
                    *span,
                ));
            }
            Ok(bytes.clone())
        }
        ArrayInit::Items(items) => {
            if items.len() != len {
                let span = items
                    .last()
                    .map(Expr::span)
                    .unwrap_or_else(|| Span::point(0));
                return Err(Diagnostic::new(
                    format!(
                        "array initializer has {} elements, but length is {len}",
                        items.len()
                    ),
                    span,
                ));
            }
            items
                .iter()
                .map(|expr| {
                    const_eval(expr).ok_or_else(|| {
                        Diagnostic::new(
                            "array initializer elements must be constant bytes",
                            expr.span(),
                        )
                    })
                })
                .collect()
        }
    }
}

fn const_eval(expr: &Expr) -> Option<u8> {
    match expr {
        Expr::Byte(value, _) => Some(*value),
        Expr::Var(_, _) | Expr::ArrayGet { .. } | Expr::Call { .. } => None,
        Expr::Unary { op, expr, .. } => {
            let value = const_eval(expr)?;
            Some(match op {
                UnOp::Not => u8::from(value == 0),
                UnOp::BitNot => !value,
            })
        }
        Expr::Binary {
            left, op, right, ..
        } => {
            let left = const_eval(left)?;
            let right = const_eval(right)?;
            Some(match op {
                BinOp::Or => u8::from(left != 0 || right != 0),
                BinOp::And => u8::from(left != 0 && right != 0),
                BinOp::BitOr => left | right,
                BinOp::BitXor => left ^ right,
                BinOp::BitAnd => left & right,
                BinOp::Shl => {
                    if right >= 8 {
                        0
                    } else {
                        left << right
                    }
                }
                BinOp::Shr => {
                    if right >= 8 {
                        0
                    } else {
                        left >> right
                    }
                }
                BinOp::Add => left.wrapping_add(right),
                BinOp::Sub => left.wrapping_sub(right),
                BinOp::Mul => left.wrapping_mul(right),
                BinOp::Div => {
                    if right == 0 {
                        0
                    } else {
                        left / right
                    }
                }
                BinOp::Mod => {
                    if right == 0 {
                        left
                    } else {
                        left % right
                    }
                }
                BinOp::Eq => u8::from(left == right),
                BinOp::Ne => u8::from(left != right),
                BinOp::Lt => u8::from(left < right),
                BinOp::Le => u8::from(left <= right),
                BinOp::Gt => u8::from(left > right),
                BinOp::Ge => u8::from(left >= right),
            })
        }
    }
}

fn count_stmt_calls(stmts: &[ResolvedStmt]) -> usize {
    stmts.iter().map(count_stmt_call).sum()
}

fn count_stmt_call(stmt: &ResolvedStmt) -> usize {
    match stmt {
        ResolvedStmt::Let { init, .. } => init.as_ref().map_or(0, count_expr_calls),
        ResolvedStmt::LetArray { .. } | ResolvedStmt::Puts(_) | ResolvedStmt::Read(_) => 0,
        ResolvedStmt::Assign { expr, .. }
        | ResolvedStmt::Put(expr)
        | ResolvedStmt::Print(expr)
        | ResolvedStmt::Println(expr) => count_expr_calls(expr),
        ResolvedStmt::Call { args, .. } => args.iter().map(count_expr_calls).sum(),
        ResolvedStmt::Return(expr) => expr.as_ref().map_or(0, count_expr_calls),
        ResolvedStmt::ArraySet { index, expr, .. } => {
            count_expr_calls(index) + count_expr_calls(expr)
        }
        ResolvedStmt::ReadArray { index, .. } => count_expr_calls(index),
        ResolvedStmt::Break | ResolvedStmt::Continue => 0,
        ResolvedStmt::Block(stmts) | ResolvedStmt::Loop { body: stmts } => count_stmt_calls(stmts),
        ResolvedStmt::If {
            cond,
            then_branch,
            else_branch,
        } => count_expr_calls(cond) + count_stmt_calls(then_branch) + count_stmt_calls(else_branch),
        ResolvedStmt::While { cond, body } => count_expr_calls(cond) + count_stmt_calls(body),
        ResolvedStmt::For {
            init,
            cond,
            step,
            body,
        } => {
            init.as_deref().map_or(0, count_stmt_call)
                + cond.as_ref().map_or(0, count_expr_calls)
                + step.as_deref().map_or(0, count_stmt_call)
                + count_stmt_calls(body)
        }
    }
}

fn count_expr_calls(expr: &ResolvedExpr) -> usize {
    match expr {
        ResolvedExpr::Byte(_) | ResolvedExpr::Var(_) => 0,
        ResolvedExpr::ArrayGet { index, .. } => count_expr_calls(index),
        ResolvedExpr::Call { args, .. } => 1 + args.iter().map(count_expr_calls).sum::<usize>(),
        ResolvedExpr::Unary { expr, .. } => count_expr_calls(expr),
        ResolvedExpr::Binary { left, right, .. } => {
            count_expr_calls(left) + count_expr_calls(right)
        }
    }
}

fn stmts_always_return(stmts: &[ResolvedStmt]) -> bool {
    stmts.iter().any(stmt_always_returns)
}

fn stmt_always_returns(stmt: &ResolvedStmt) -> bool {
    match stmt {
        ResolvedStmt::Return(Some(_)) => true,
        ResolvedStmt::Block(stmts) => stmts_always_return(stmts),
        ResolvedStmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            !else_branch.is_empty()
                && stmts_always_return(then_branch)
                && stmts_always_return(else_branch)
        }
        _ => false,
    }
}
