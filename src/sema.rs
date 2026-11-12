use std::collections::HashMap;

use crate::ast::{
    ArrayInit, ArrayLen, BinOp, Expr, FormatSpec, Function, InputTarget, IntLiteralKind, Program,
    ReturnType, Stmt, UnOp, ValueType,
};
use crate::diagnostic::{Diagnostic, Span};

pub const TEMP_COUNT: usize = 8;
pub const WORK_CELL_COUNT: usize = 12;
pub const MAX_CALL_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueLayout {
    pub base: usize,
    pub value_type: ValueType,
}

impl ValueLayout {
    pub const fn byte(base: usize) -> Self {
        Self {
            base,
            value_type: ValueType::Byte,
        }
    }

    pub const fn cell_width(self) -> usize {
        self.value_type.cell_width()
    }

    pub fn cells(self) -> std::ops::Range<usize> {
        self.base..self.base + self.cell_width()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayLayout {
    pub base: usize,
    pub len: usize,
    pub value_type: ValueType,
}

impl ArrayLayout {
    pub const fn data_offset(self) -> usize {
        match self.value_type {
            ValueType::Byte => 4,
            ValueType::Int | ValueType::Fixed => 0,
        }
    }

    pub const fn storage_len(self) -> usize {
        self.data_offset() + self.len * self.value_type.cell_width()
    }

    pub const fn element(self, index: usize) -> ValueLayout {
        ValueLayout {
            base: self.base + self.data_offset() + index * self.value_type.cell_width(),
            value_type: self.value_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Symbols {
    cells: HashMap<String, usize>,
    layouts: HashMap<String, ValueLayout>,
    entries: Vec<SymbolInfo>,
    scratch_base: usize,
}

impl Symbols {
    pub fn cell(&self, name: &str) -> Option<usize> {
        self.cells.get(name).copied()
    }

    pub fn layout(&self, name: &str) -> Option<ValueLayout> {
        self.layouts.get(name).copied()
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
        value_type: ValueType,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedProgram {
    pub functions: Vec<ResolvedFunction>,
    pub stmts: Vec<ResolvedStmt>,
    pub spills: Vec<ValueLayout>,
    pub symbols: Symbols,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFunction {
    pub name: String,
    pub params: Vec<ValueLayout>,
    pub return_type: ReturnType,
    pub body: Vec<ResolvedStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedStmt {
    Const,
    Let {
        layout: ValueLayout,
        init: Option<ResolvedExpr>,
    },
    LetArray {
        layout: ArrayLayout,
        init: Vec<u32>,
    },
    Assign {
        layout: ValueLayout,
        expr: ResolvedExpr,
    },
    ArraySet {
        layout: ArrayLayout,
        index: ResolvedExpr,
        expr: ResolvedExpr,
    },
    Put(ResolvedExpr),
    Puts(Vec<u8>),
    Print(ResolvedExpr),
    Println(ResolvedExpr),
    Read(ValueLayout),
    ReadArray {
        layout: ArrayLayout,
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
    Int(u32),
    Fixed(u32),
    Var(ValueLayout),
    ArrayGet {
        layout: ArrayLayout,
        index: Box<ResolvedExpr>,
    },
    Call {
        function: usize,
        args: Vec<ResolvedExpr>,
        value_type: ValueType,
    },
    Input {
        parts: Vec<Vec<u8>>,
        specs: Vec<FormatSpec>,
        targets: Vec<ResolvedInputTarget>,
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
    Cast {
        expr: Box<ResolvedExpr>,
        target: ValueType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedInputTarget {
    Scalar(ValueLayout),
    Array {
        layout: ArrayLayout,
        index: Box<ResolvedExpr>,
    },
}

impl ResolvedInputTarget {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Scalar(layout) => layout.value_type,
            Self::Array { layout, .. } => layout.value_type,
        }
    }
}

impl ResolvedExpr {
    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Byte(_) => ValueType::Byte,
            Self::Int(_) => ValueType::Int,
            Self::Fixed(_) => ValueType::Fixed,
            Self::Var(layout) => layout.value_type,
            Self::ArrayGet { layout, .. } => layout.value_type,
            Self::Call { value_type, .. } => *value_type,
            Self::Input { .. } => ValueType::Byte,
            Self::Unary { op, expr } => match op {
                UnOp::Not => ValueType::Byte,
                UnOp::Neg if expr.value_type() == ValueType::Fixed => ValueType::Fixed,
                UnOp::Neg => ValueType::Int,
                UnOp::BitNot => expr.value_type(),
            },
            Self::Binary {
                left, op, right, ..
            } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::Or
                | BinOp::And => ValueType::Byte,
                _ if left.value_type() == ValueType::Fixed
                    || right.value_type() == ValueType::Fixed =>
                {
                    ValueType::Fixed
                }
                _ if left.value_type() == ValueType::Int
                    || right.value_type() == ValueType::Int =>
                {
                    ValueType::Int
                }
                _ => ValueType::Byte,
            },
            Self::Cast { target, .. } => *target,
        }
    }
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
    let mut spill_types = Vec::new();
    collect_stmt_call_types(&stmts, &mut spill_types);
    for function in &functions {
        collect_stmt_call_types(&function.body, &mut spill_types);
    }
    let spills = spill_types
        .into_iter()
        .map(|value_type| resolver.alloc_fresh_value(value_type))
        .collect::<Vec<_>>();
    Ok(ResolvedProgram {
        functions,
        stmts,
        spills,
        symbols: Symbols {
            cells: resolver.public_cells,
            layouts: resolver.public_layouts,
            entries: resolver.public_entries,
            scratch_base: resolver.high_water,
        },
    })
}

#[derive(Debug, Default)]
struct Scope {
    names: HashMap<String, Symbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Const {
        value: u32,
        value_type: ValueType,
        declared_at: usize,
    },
    Scalar(ValueLayout),
    Array(ArrayLayout),
}

#[derive(Debug, Clone, Copy)]
struct ConstValue {
    value: u32,
    value_type: ValueType,
}

struct Resolver {
    scopes: Vec<Scope>,
    public_cells: HashMap<String, usize>,
    public_layouts: HashMap<String, ValueLayout>,
    public_entries: Vec<SymbolInfo>,
    functions: HashMap<String, usize>,
    function_names: Vec<String>,
    function_arities: Vec<usize>,
    function_param_types: Vec<Vec<ValueType>>,
    function_return_types: Vec<ReturnType>,
    call_graph: Vec<Vec<usize>>,
    current_function: Option<usize>,
    current_function_start: Option<usize>,
    next_cell: usize,
    high_water: usize,
}

impl Resolver {
    fn new() -> Self {
        Self {
            scopes: vec![Scope::default()],
            public_cells: HashMap::new(),
            public_layouts: HashMap::new(),
            public_entries: Vec::new(),
            functions: HashMap::new(),
            function_names: Vec::new(),
            function_arities: Vec::new(),
            function_param_types: Vec::new(),
            function_return_types: Vec::new(),
            call_graph: Vec::new(),
            current_function: None,
            current_function_start: None,
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
            self.function_param_types.push(
                function
                    .params
                    .iter()
                    .map(|param| param.value_type)
                    .collect(),
            );
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
                let layout = self.alloc_fresh_value(param.value_type);
                self.current_scope_mut()
                    .names
                    .insert(param.name.clone(), Symbol::Scalar(layout));
                params.push(layout);
            }
            let previous = self.current_function.replace(index);
            let previous_start = self.current_function_start.replace(function.span.start);
            let body = self.resolve_stmts(&function.body, 0)?;
            self.current_function = previous;
            self.current_function_start = previous_start;
            self.pop_scope_retain();
            if function.return_type != ReturnType::Void && !stmts_always_return(&body) {
                return Err(Diagnostic::new(
                    format!(
                        "{} function `{}` may fall through without returning a value",
                        return_type_name(function.return_type),
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
            Stmt::Const {
                name,
                name_span,
                expr,
                ..
            } => {
                if self.current_scope().names.contains_key(name) {
                    return Err(Diagnostic::new(
                        format!("name `{name}` already declared in this scope"),
                        *name_span,
                    ));
                }
                let value = self.eval_const_expr(expr)?;
                self.current_scope_mut().names.insert(
                    name.clone(),
                    Symbol::Const {
                        value: value.value,
                        value_type: value.value_type,
                        declared_at: name_span.start,
                    },
                );
                Ok(ResolvedStmt::Const)
            }
            Stmt::Let {
                name,
                name_span,
                value_type,
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
                let resolved_type = value_type
                    .or_else(|| init.as_ref().map(ResolvedExpr::value_type))
                    .expect("parser requires an initializer for inferred declarations");
                if let Some(init) = &init
                    && !can_assign(resolved_type, init.value_type())
                {
                    return Err(Diagnostic::new(
                        format!(
                            "cannot assign {:?} expression to {:?} variable",
                            init.value_type(),
                            resolved_type
                        ),
                        *name_span,
                    ));
                }
                let layout = self.alloc_fresh_value(resolved_type);
                let cell = layout.base;
                self.current_scope_mut()
                    .names
                    .insert(name.clone(), Symbol::Scalar(layout));
                self.define_public_scalar(
                    name,
                    layout,
                    SymbolInfo::Scalar {
                        name: name.clone(),
                        cell,
                    },
                );
                Ok(ResolvedStmt::Let { layout, init })
            }
            Stmt::LetArray {
                name,
                name_span,
                value_type,
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
                let len = self.resolve_array_len(len)?;
                let value_type = match value_type {
                    Some(value_type) => *value_type,
                    None => self.infer_array_type(init.as_ref())?,
                };
                let provisional_layout = ArrayLayout {
                    base: 0,
                    len,
                    value_type,
                };
                let storage_len = provisional_layout.storage_len();
                let base = self.alloc_fresh_block(storage_len);
                let layout = ArrayLayout {
                    base,
                    len,
                    value_type,
                };
                self.current_scope_mut()
                    .names
                    .insert(name.clone(), Symbol::Array(layout));
                self.define_public_symbol(
                    name,
                    base,
                    SymbolInfo::Array {
                        name: name.clone(),
                        base,
                        len,
                        value_type,
                    },
                );
                let init = self.resolve_array_init(init.as_ref(), len, value_type)?;
                Ok(ResolvedStmt::LetArray { layout, init })
            }
            Stmt::Assign {
                name,
                name_span,
                expr,
                ..
            } => {
                let layout =
                    self.lookup_scalar(name, *name_span, "assignment to undeclared variable")?;
                let expr = self.resolve_expr(expr)?;
                if !can_assign(layout.value_type, expr.value_type()) {
                    return Err(Diagnostic::new(
                        format!(
                            "cannot assign {:?} expression to {:?} variable",
                            expr.value_type(),
                            layout.value_type
                        ),
                        *name_span,
                    ));
                }
                Ok(ResolvedStmt::Assign { layout, expr })
            }
            Stmt::ArrayAssign {
                name,
                name_span,
                index,
                expr,
                ..
            } => {
                let layout = self.lookup_array(name, *name_span)?;
                self.check_const_index(index, layout.len)?;
                let index = self.resolve_expr(index)?;
                self.require_byte(&index, *name_span, "array indexing")?;
                let expr = self.resolve_expr(expr)?;
                if !can_assign(layout.value_type, expr.value_type()) {
                    return Err(Diagnostic::new(
                        format!(
                            "cannot assign {:?} expression to {:?} array element",
                            expr.value_type(),
                            layout.value_type
                        ),
                        *name_span,
                    ));
                }
                Ok(ResolvedStmt::ArraySet {
                    layout,
                    index,
                    expr,
                })
            }
            Stmt::Put(expr, span) => {
                let expr = self.resolve_expr(expr)?;
                self.require_byte(&expr, *span, "raw output")?;
                Ok(ResolvedStmt::Put(expr))
            }
            Stmt::Puts(bytes, _) => Ok(ResolvedStmt::Puts(bytes.clone())),
            Stmt::Print(expr, _) => {
                let expr = self.resolve_expr(expr)?;
                Ok(ResolvedStmt::Print(expr))
            }
            Stmt::PrintFormat {
                parts, specs, args, ..
            } => {
                let mut stmts = Vec::new();
                for (index, part) in parts.iter().enumerate() {
                    if !part.is_empty() {
                        stmts.push(ResolvedStmt::Puts(part.clone()));
                    }
                    if let Some(arg) = args.get(index) {
                        let expr = self.resolve_expr(arg)?;
                        match specs[index] {
                            FormatSpec::Decimal => stmts.push(ResolvedStmt::Print(expr)),
                            FormatSpec::Byte => {
                                self.require_byte(&expr, arg.span(), "raw formatted output")?;
                                stmts.push(ResolvedStmt::Put(expr));
                            }
                        }
                    }
                }
                Ok(ResolvedStmt::Block(stmts))
            }
            Stmt::Println(expr, _) => {
                let expr = self.resolve_expr(expr)?;
                Ok(ResolvedStmt::Println(expr))
            }
            Stmt::Read(name, span) => {
                let layout = self.lookup_scalar(name, *span, "read into undeclared variable")?;
                if layout.value_type != ValueType::Byte {
                    return Err(Diagnostic::new(
                        "read requires a byte target; parse wider values explicitly",
                        *span,
                    ));
                }
                Ok(ResolvedStmt::Read(layout))
            }
            Stmt::ReadArray {
                name,
                name_span,
                index,
                ..
            } => {
                let layout = self.lookup_array(name, *name_span)?;
                if layout.value_type != ValueType::Byte {
                    return Err(Diagnostic::new(
                        "read requires a byte array element; parse wider values explicitly",
                        *name_span,
                    ));
                }
                self.check_const_index(index, layout.len)?;
                let index = self.resolve_expr(index)?;
                self.require_byte(&index, *name_span, "array indexing")?;
                Ok(ResolvedStmt::ReadArray { layout, index })
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
                        let expr = self.resolve_expr(expr)?;
                        if !can_assign(ValueType::Byte, expr.value_type()) {
                            return Err(Diagnostic::new(
                                "cannot return int expression from byte function",
                                *span,
                            ));
                        }
                        Ok(ResolvedStmt::Return(Some(expr)))
                    }
                    (ReturnType::Int, None) => {
                        Err(Diagnostic::new("int function must return a value", *span))
                    }
                    (ReturnType::Int, Some(expr)) => {
                        let mut expr = self.resolve_expr(expr)?;
                        if !can_assign(ValueType::Int, expr.value_type()) {
                            return Err(Diagnostic::new(
                                "cannot return expression from int function",
                                *span,
                            ));
                        }
                        if expr.value_type() == ValueType::Byte {
                            expr = ResolvedExpr::Cast {
                                expr: Box::new(expr),
                                target: ValueType::Int,
                            };
                        }
                        Ok(ResolvedStmt::Return(Some(expr)))
                    }
                    (ReturnType::Fixed, None) => {
                        Err(Diagnostic::new("fixed function must return a value", *span))
                    }
                    (ReturnType::Fixed, Some(expr)) => {
                        let mut expr = self.resolve_expr(expr)?;
                        if !can_assign(ValueType::Fixed, expr.value_type()) {
                            return Err(Diagnostic::new(
                                "cannot return expression from fixed function",
                                *span,
                            ));
                        }
                        if expr.value_type() == ValueType::Byte {
                            expr = ResolvedExpr::Cast {
                                expr: Box::new(expr),
                                target: ValueType::Fixed,
                            };
                        }
                        Ok(ResolvedStmt::Return(Some(expr)))
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
            Expr::Int(value, kind, span) => {
                if *kind == IntLiteralKind::Decimal && *value > i32::MAX as u32 {
                    return Err(Diagnostic::new(
                        "positive decimal integer literal exceeds signed 32-bit range",
                        *span,
                    ));
                }
                Ok(ResolvedExpr::Int(*value))
            }
            Expr::Fixed(value, span) => {
                if *value > i32::MAX as u32 {
                    return Err(Diagnostic::new(
                        "positive fixed literal exceeds signed Q16.16 range",
                        *span,
                    ));
                }
                Ok(ResolvedExpr::Fixed(*value))
            }
            Expr::Var(name, span) => {
                match self.lookup_symbol(name, *span, "use of undeclared variable")? {
                    Symbol::Const {
                        value, value_type, ..
                    } => Ok(match value_type {
                        ValueType::Byte => ResolvedExpr::Byte(value as u8),
                        ValueType::Int => ResolvedExpr::Int(value),
                        ValueType::Fixed => ResolvedExpr::Fixed(value),
                    }),
                    Symbol::Scalar(layout) => Ok(ResolvedExpr::Var(layout)),
                    Symbol::Array(_) => {
                        Err(Diagnostic::new(format!("`{name}` is an array"), *span))
                    }
                }
            }
            Expr::ArrayGet {
                name,
                name_span,
                index,
                ..
            } => {
                let layout = self.lookup_array(name, *name_span)?;
                self.check_const_index(index, layout.len)?;
                let index = self.resolve_expr(index)?;
                self.require_byte(&index, *name_span, "array indexing")?;
                Ok(ResolvedExpr::ArrayGet {
                    layout,
                    index: Box::new(index),
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
                let value_type = match self.function_return_types[function] {
                    ReturnType::Byte => ValueType::Byte,
                    ReturnType::Int => ValueType::Int,
                    ReturnType::Fixed => ValueType::Fixed,
                    ReturnType::Void => unreachable!(),
                };
                Ok(ResolvedExpr::Call {
                    function,
                    args,
                    value_type,
                })
            }
            Expr::Input {
                parts,
                specs,
                targets,
                span,
            } => {
                if specs.len() != targets.len() {
                    return Err(Diagnostic::new(
                        format!(
                            "input format has {} placeholders, but {} destinations were provided",
                            specs.len(),
                            targets.len()
                        ),
                        *span,
                    ));
                }
                let mut resolved_targets = Vec::new();
                for (spec, target) in specs.iter().zip(targets) {
                    let resolved = match target {
                        InputTarget::Scalar { name, name_span } => ResolvedInputTarget::Scalar(
                            self.lookup_scalar(name, *name_span, "input into undeclared variable")?,
                        ),
                        InputTarget::Array {
                            name,
                            name_span,
                            index,
                            ..
                        } => {
                            let layout = self.lookup_array(name, *name_span)?;
                            self.check_const_index(index, layout.len)?;
                            let index = self.resolve_expr(index)?;
                            self.require_byte(&index, *name_span, "array indexing")?;
                            ResolvedInputTarget::Array {
                                layout,
                                index: Box::new(index),
                            }
                        }
                    };
                    match spec {
                        FormatSpec::Byte if resolved.value_type() != ValueType::Byte => {
                            return Err(Diagnostic::new(
                                "raw formatted input requires a byte destination",
                                target.span(),
                            ));
                        }
                        FormatSpec::Decimal if resolved.value_type() == ValueType::Fixed => {
                            return Err(Diagnostic::new(
                                "formatted decimal input does not yet support fixed destinations",
                                target.span(),
                            ));
                        }
                        _ => {}
                    }
                    resolved_targets.push(resolved);
                }
                Ok(ResolvedExpr::Input {
                    parts: parts.clone(),
                    specs: specs.clone(),
                    targets: resolved_targets,
                })
            }
            Expr::Unary { op, expr, span } => {
                if *op == UnOp::Neg
                    && let Expr::Int(value, IntLiteralKind::Decimal, _) = expr.as_ref()
                {
                    return Ok(ResolvedExpr::Int(0u32.wrapping_sub(*value)));
                }
                if *op == UnOp::Neg
                    && let Expr::Fixed(value, _) = expr.as_ref()
                {
                    return Ok(ResolvedExpr::Fixed(0u32.wrapping_sub(*value)));
                }
                let expr = self.resolve_expr(expr)?;
                if *op == UnOp::Neg {
                    match expr {
                        ResolvedExpr::Byte(value) => {
                            return Ok(ResolvedExpr::Int(0u32.wrapping_sub(value.into())));
                        }
                        ResolvedExpr::Int(value) => {
                            return Ok(ResolvedExpr::Int(0u32.wrapping_sub(value)));
                        }
                        ResolvedExpr::Fixed(value) => {
                            return Ok(ResolvedExpr::Fixed(0u32.wrapping_sub(value)));
                        }
                        expr => {
                            return Ok(ResolvedExpr::Unary {
                                op: *op,
                                expr: Box::new(expr),
                            });
                        }
                    }
                }
                if *op == UnOp::BitNot && expr.value_type() == ValueType::Fixed {
                    return Err(Diagnostic::new(
                        "bitwise complement is not supported for fixed values",
                        *span,
                    ));
                }
                if *op == UnOp::BitNot || *op == UnOp::Not {
                    return Ok(ResolvedExpr::Unary {
                        op: *op,
                        expr: Box::new(expr),
                    });
                }
                self.require_byte(&expr, *span, "unary operator")?;
                Ok(ResolvedExpr::Unary {
                    op: *op,
                    expr: Box::new(expr),
                })
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let mut left = self.resolve_expr(left)?;
                let mut right = self.resolve_expr(right)?;
                let has_fixed =
                    left.value_type() == ValueType::Fixed || right.value_type() == ValueType::Fixed;
                let has_int =
                    left.value_type() == ValueType::Int || right.value_type() == ValueType::Int;
                if has_fixed && has_int {
                    return Err(Diagnostic::new(
                        "int and fixed values require an explicit cast before mixing",
                        expr.span(),
                    ));
                }
                if has_fixed
                    && matches!(
                        op,
                        BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::BitAnd
                            | BinOp::Shl
                            | BinOp::Shr
                            | BinOp::Mod
                    )
                {
                    return Err(Diagnostic::new(
                        "fixed values do not support bitwise, shift, or remainder operators",
                        expr.span(),
                    ));
                }
                if has_fixed {
                    if left.value_type() == ValueType::Byte {
                        left = ResolvedExpr::Cast {
                            expr: Box::new(left),
                            target: ValueType::Fixed,
                        };
                    }
                    if right.value_type() == ValueType::Byte {
                        right = ResolvedExpr::Cast {
                            expr: Box::new(right),
                            target: ValueType::Fixed,
                        };
                    }
                }
                if has_int
                    && matches!(op, BinOp::Shl | BinOp::Shr)
                    && right.value_type() != ValueType::Byte
                {
                    return Err(Diagnostic::new(
                        "int shift counts must be byte values",
                        expr.span(),
                    ));
                }
                Ok(ResolvedExpr::Binary {
                    left: Box::new(left),
                    op: *op,
                    right: Box::new(right),
                })
            }
            Expr::Cast { expr, target, .. } => Ok(ResolvedExpr::Cast {
                expr: Box::new(self.resolve_expr(expr)?),
                target: *target,
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
        for (arg, expected) in args.iter().zip(&self.function_param_types[function]) {
            if !can_assign(*expected, arg.value_type()) {
                return Err(Diagnostic::new(
                    format!(
                        "cannot pass {:?} expression to {:?} parameter",
                        arg.value_type(),
                        expected
                    ),
                    name_span,
                ));
            }
        }
        Ok((function, args))
    }

    fn resolve_array_len(&self, len: &ArrayLen) -> Result<usize, Diagnostic> {
        match len {
            ArrayLen::Inferred(len) => {
                if *len > usize::from(u8::MAX) {
                    Err(Diagnostic::bare(
                        "inferred array length exceeds the byte limit of 255",
                    ))
                } else {
                    Ok(*len)
                }
            }
            ArrayLen::Explicit(expr) => {
                let len = self.eval_const_expr(expr)?;
                if len.value == 0 {
                    Err(Diagnostic::new(
                        "array length must be greater than zero",
                        expr.span(),
                    ))
                } else if len.value > u32::from(u8::MAX) {
                    Err(Diagnostic::new(
                        "array length exceeds the byte limit of 255",
                        expr.span(),
                    ))
                } else {
                    Ok(len.value as usize)
                }
            }
        }
    }

    fn infer_array_type(&mut self, init: Option<&ArrayInit>) -> Result<ValueType, Diagnostic> {
        match init {
            Some(ArrayInit::Bytes(..)) => Ok(ValueType::Byte),
            Some(ArrayInit::Items(items)) => {
                let mut value_type = ValueType::Byte;
                for item in items {
                    let item_type = self.resolve_expr(item)?.value_type();
                    match (value_type, item_type) {
                        (ValueType::Int, ValueType::Fixed) | (ValueType::Fixed, ValueType::Int) => {
                            return Err(Diagnostic::new(
                                "cannot infer an array type from mixed int and fixed values",
                                item.span(),
                            ));
                        }
                        (_, ValueType::Int) => value_type = ValueType::Int,
                        (_, ValueType::Fixed) => value_type = ValueType::Fixed,
                        _ => {}
                    }
                }
                Ok(value_type)
            }
            None => Ok(ValueType::Byte),
        }
    }

    fn resolve_array_init(
        &mut self,
        init: Option<&ArrayInit>,
        len: usize,
        value_type: ValueType,
    ) -> Result<Vec<u32>, Diagnostic> {
        let Some(init) = init else {
            return Ok(Vec::new());
        };

        match init {
            ArrayInit::Bytes(bytes, span) => {
                if value_type != ValueType::Byte {
                    return Err(Diagnostic::new(
                        "string initializer requires a byte array",
                        *span,
                    ));
                }
                if bytes.len() != len {
                    return Err(Diagnostic::new(
                        format!(
                            "array initializer has {} elements, but length is {len}",
                            bytes.len()
                        ),
                        *span,
                    ));
                }
                Ok(bytes.iter().copied().map(u32::from).collect())
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
                        let resolved = self.resolve_expr(expr)?;
                        if !can_assign(value_type, resolved.value_type()) {
                            return Err(Diagnostic::new(
                                format!(
                                    "cannot initialize {:?} array with {:?} expression",
                                    value_type,
                                    resolved.value_type()
                                ),
                                expr.span(),
                            ));
                        }
                        let value = eval_resolved_const(&resolved).ok_or_else(|| {
                            Diagnostic::new(
                                match value_type {
                                    ValueType::Byte => {
                                        "array initializer elements must be constant bytes"
                                    }
                                    ValueType::Int | ValueType::Fixed => {
                                        "array initializer elements must be constant scalar expressions"
                                    }
                                },
                                expr.span(),
                            )
                        })?;
                        Ok(convert_const_value(value, resolved.value_type(), value_type))
                    })
                    .collect()
            }
        }
    }

    fn check_const_index(&self, index: &Expr, len: usize) -> Result<(), Diagnostic> {
        if let Ok(value) = self.eval_const_expr(index) {
            if value.value as usize >= len {
                return Err(Diagnostic::new(
                    format!(
                        "array index {} is out of bounds for length {len}",
                        value.value
                    ),
                    index.span(),
                ));
            }
        }
        Ok(())
    }

    fn eval_const_expr(&self, expr: &Expr) -> Result<ConstValue, Diagnostic> {
        match expr {
            Expr::Byte(value, _) => Ok(ConstValue {
                value: u32::from(*value),
                value_type: ValueType::Byte,
            }),
            Expr::Int(value, kind, span) => {
                if *kind == IntLiteralKind::Decimal && *value > i32::MAX as u32 {
                    return Err(Diagnostic::new(
                        "positive decimal integer literal exceeds signed 32-bit range",
                        *span,
                    ));
                }
                Ok(ConstValue {
                    value: *value,
                    value_type: ValueType::Int,
                })
            }
            Expr::Fixed(value, span) => {
                if *value > i32::MAX as u32 {
                    return Err(Diagnostic::new(
                        "positive fixed literal exceeds signed Q16.16 range",
                        *span,
                    ));
                }
                Ok(ConstValue {
                    value: *value,
                    value_type: ValueType::Fixed,
                })
            }
            Expr::Var(name, span) => match self.lookup_symbol(
                name,
                *span,
                "constant expression references undeclared or forward name",
            )? {
                Symbol::Const {
                    value, value_type, ..
                } => Ok(ConstValue { value, value_type }),
                Symbol::Scalar(_) => Err(Diagnostic::new(
                    format!("constant expression cannot use variable `{name}`"),
                    *span,
                )),
                Symbol::Array(_) => Err(Diagnostic::new(
                    format!("constant expression cannot use array `{name}`"),
                    *span,
                )),
            },
            Expr::ArrayGet { span, .. } => Err(Diagnostic::new(
                "constant expression cannot access an array",
                *span,
            )),
            Expr::Call { span, .. } => Err(Diagnostic::new(
                "constant expression cannot call a function",
                *span,
            )),
            Expr::Input { span, .. } => Err(Diagnostic::new(
                "constant expression cannot perform formatted input",
                *span,
            )),
            Expr::Unary { op, expr, .. } => {
                if *op == UnOp::Neg
                    && let Expr::Int(value, IntLiteralKind::Decimal, _) = expr.as_ref()
                {
                    return Ok(ConstValue {
                        value: 0u32.wrapping_sub(*value),
                        value_type: ValueType::Int,
                    });
                }
                if *op == UnOp::Neg
                    && let Expr::Fixed(value, _) = expr.as_ref()
                {
                    return Ok(ConstValue {
                        value: 0u32.wrapping_sub(*value),
                        value_type: ValueType::Fixed,
                    });
                }
                let value = self.eval_const_expr(expr)?;
                let value_type = match op {
                    UnOp::Not => ValueType::Byte,
                    UnOp::Neg if value.value_type == ValueType::Fixed => ValueType::Fixed,
                    UnOp::Neg => ValueType::Int,
                    UnOp::BitNot => value.value_type,
                };
                let raw = match (op, value.value_type) {
                    (UnOp::Not, _) => u32::from(value.value == 0),
                    (UnOp::Neg, _) => 0u32.wrapping_sub(value.value),
                    (UnOp::BitNot, ValueType::Byte) => u32::from(!(value.value as u8)),
                    (UnOp::BitNot, ValueType::Int) => !value.value,
                    (UnOp::BitNot, ValueType::Fixed) => {
                        return Err(Diagnostic::new(
                            "bitwise complement is not supported for fixed values",
                            expr.span(),
                        ));
                    }
                };
                Ok(ConstValue {
                    value: raw,
                    value_type,
                })
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let left = self.eval_const_expr(left)?;
                let right = self.eval_const_expr(right)?;
                let has_fixed =
                    left.value_type == ValueType::Fixed || right.value_type == ValueType::Fixed;
                let int_op =
                    left.value_type == ValueType::Int || right.value_type == ValueType::Int;
                if has_fixed && int_op {
                    return Err(Diagnostic::new(
                        "int and fixed values require an explicit cast before mixing",
                        expr.span(),
                    ));
                }
                if has_fixed
                    && matches!(
                        op,
                        BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::BitAnd
                            | BinOp::Shl
                            | BinOp::Shr
                            | BinOp::Mod
                    )
                {
                    return Err(Diagnostic::new(
                        "fixed values do not support bitwise, shift, or remainder operators",
                        expr.span(),
                    ));
                }
                let left_value = if has_fixed {
                    convert_const_value(left.value, left.value_type, ValueType::Fixed)
                } else {
                    left.value
                };
                let right_value = if has_fixed {
                    convert_const_value(right.value, right.value_type, ValueType::Fixed)
                } else {
                    right.value
                };
                let value = if has_fixed {
                    eval_fixed_const_binary(left_value, *op, right_value)
                } else if int_op {
                    eval_int_const_binary(left_value, *op, right_value)
                } else {
                    u32::from(eval_const_binary(left_value as u8, *op, right_value as u8))
                };
                let value_type = if matches!(
                    op,
                    BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::Or
                        | BinOp::And
                ) {
                    ValueType::Byte
                } else if has_fixed {
                    ValueType::Fixed
                } else if int_op {
                    ValueType::Int
                } else {
                    ValueType::Byte
                };
                Ok(ConstValue { value, value_type })
            }
            Expr::Cast { expr, target, .. } => {
                let value = self.eval_const_expr(expr)?;
                Ok(ConstValue {
                    value: convert_const_value(value.value, value.value_type, *target),
                    value_type: *target,
                })
            }
        }
    }

    fn lookup_symbol(&self, name: &str, span: Span, prefix: &str) -> Result<Symbol, Diagnostic> {
        for (scope_index, scope) in self.scopes.iter().enumerate().rev() {
            let Some(symbol) = scope.names.get(name).copied() else {
                continue;
            };
            if scope_index == 0 {
                if let (Symbol::Const { declared_at, .. }, Some(function_start)) =
                    (symbol, self.current_function_start)
                {
                    if declared_at > function_start {
                        break;
                    }
                }
            }
            return Ok(symbol);
        }
        Err(Diagnostic::new(format!("{prefix} `{name}`"), span))
    }

    fn lookup_scalar(
        &self,
        name: &str,
        span: Span,
        prefix: &str,
    ) -> Result<ValueLayout, Diagnostic> {
        match self.lookup_symbol(name, span, prefix)? {
            Symbol::Scalar(layout) => Ok(layout),
            Symbol::Const { .. } => Err(Diagnostic::new(format!("`{name}` is a constant"), span)),
            Symbol::Array(_) => Err(Diagnostic::new(format!("`{name}` is an array"), span)),
        }
    }

    fn require_byte(
        &self,
        expr: &ResolvedExpr,
        span: Span,
        context: &str,
    ) -> Result<(), Diagnostic> {
        if expr.value_type() == ValueType::Byte {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!("{context} requires a byte expression"),
                span,
            ))
        }
    }

    fn lookup_array(&self, name: &str, span: Span) -> Result<ArrayLayout, Diagnostic> {
        match self.lookup_symbol(name, span, "use of undeclared array")? {
            Symbol::Array(layout) => Ok(layout),
            Symbol::Const { .. } | Symbol::Scalar(_) => {
                Err(Diagnostic::new(format!("`{name}` is not an array"), span))
            }
        }
    }

    fn alloc_fresh_value(&mut self, value_type: ValueType) -> ValueLayout {
        let base = self.alloc_fresh_block(value_type.cell_width());
        ValueLayout { base, value_type }
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
        self.scopes.pop().expect("resolver always has a scope");
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

    fn define_public_scalar(&mut self, name: &str, layout: ValueLayout, info: SymbolInfo) {
        if self.scopes.len() != 1 {
            return;
        }
        if self.public_cells.contains_key(name) {
            return;
        }
        self.public_cells.insert(name.to_string(), layout.base);
        self.public_layouts.insert(name.to_string(), layout);
        self.public_entries.push(info);
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

fn can_assign(target: ValueType, source: ValueType) -> bool {
    target == source
        || (source == ValueType::Byte && matches!(target, ValueType::Int | ValueType::Fixed))
}

fn convert_const_value(value: u32, source: ValueType, target: ValueType) -> u32 {
    match (source, target) {
        (_, _) if source == target => value,
        (_, ValueType::Byte) if source == ValueType::Fixed => {
            ((value as i32) / 65_536) as u32 & 0xff
        }
        (_, ValueType::Byte) => value & 0xff,
        (ValueType::Fixed, ValueType::Int) => ((value as i32) / 65_536) as u32,
        (ValueType::Byte | ValueType::Int, ValueType::Fixed) => value.wrapping_shl(16),
        (ValueType::Byte, ValueType::Int) => value,
        _ => value,
    }
}

fn eval_resolved_const(expr: &ResolvedExpr) -> Option<u32> {
    match expr {
        ResolvedExpr::Byte(value) => Some(u32::from(*value)),
        ResolvedExpr::Int(value) => Some(*value),
        ResolvedExpr::Fixed(value) => Some(*value),
        ResolvedExpr::Cast { expr, target } => {
            let value = eval_resolved_const(expr)?;
            Some(convert_const_value(value, expr.value_type(), *target))
        }
        ResolvedExpr::Unary { op, expr } => {
            let value = eval_resolved_const(expr)?;
            Some(match op {
                UnOp::Neg => 0u32.wrapping_sub(value),
                UnOp::BitNot => match expr.value_type() {
                    ValueType::Byte => u32::from(!(value as u8)),
                    ValueType::Int => !value,
                    ValueType::Fixed => return None,
                },
                UnOp::Not => u32::from(value == 0),
            })
        }
        ResolvedExpr::Binary { left, op, right } => {
            let left_value = eval_resolved_const(left)?;
            let right_value = eval_resolved_const(right)?;
            let fixed_op =
                left.value_type() == ValueType::Fixed || right.value_type() == ValueType::Fixed;
            let int_op =
                left.value_type() == ValueType::Int || right.value_type() == ValueType::Int;
            let value = if fixed_op {
                eval_fixed_const_binary(
                    convert_const_value(left_value, left.value_type(), ValueType::Fixed),
                    *op,
                    convert_const_value(right_value, right.value_type(), ValueType::Fixed),
                )
            } else if int_op {
                eval_int_const_binary(left_value, *op, right_value)
            } else {
                u32::from(eval_const_binary(left_value as u8, *op, right_value as u8))
            };
            Some(value)
        }
        ResolvedExpr::Var(_)
        | ResolvedExpr::ArrayGet { .. }
        | ResolvedExpr::Call { .. }
        | ResolvedExpr::Input { .. } => None,
    }
}

fn eval_int_const_binary(left: u32, op: BinOp, right: u32) -> u32 {
    match op {
        BinOp::Or => u32::from(left != 0 || right != 0),
        BinOp::And => u32::from(left != 0 && right != 0),
        BinOp::BitOr => left | right,
        BinOp::BitXor => left ^ right,
        BinOp::BitAnd => left & right,
        BinOp::Shl => {
            if right >= 32 {
                0
            } else {
                left.wrapping_shl(right)
            }
        }
        BinOp::Shr => {
            if right >= 32 {
                if (left as i32) < 0 { u32::MAX } else { 0 }
            } else {
                ((left as i32) >> right) as u32
            }
        }
        BinOp::Add => left.wrapping_add(right),
        BinOp::Sub => left.wrapping_sub(right),
        BinOp::Mul => left.wrapping_mul(right),
        BinOp::Div => {
            if right == 0 {
                0
            } else {
                (left as i32).wrapping_div(right as i32) as u32
            }
        }
        BinOp::Mod => {
            if right == 0 {
                left
            } else {
                (left as i32).wrapping_rem(right as i32) as u32
            }
        }
        BinOp::Eq => u32::from(left == right),
        BinOp::Ne => u32::from(left != right),
        BinOp::Lt => u32::from((left as i32) < right as i32),
        BinOp::Le => u32::from((left as i32) <= right as i32),
        BinOp::Gt => u32::from((left as i32) > right as i32),
        BinOp::Ge => u32::from((left as i32) >= right as i32),
    }
}

fn eval_fixed_const_binary(left: u32, op: BinOp, right: u32) -> u32 {
    match op {
        BinOp::Or => u32::from(left != 0 || right != 0),
        BinOp::And => u32::from(left != 0 && right != 0),
        BinOp::Add => left.wrapping_add(right),
        BinOp::Sub => left.wrapping_sub(right),
        BinOp::Mul => (((left as i32 as i64) * (right as i32 as i64)) / 65_536) as u32,
        BinOp::Div => {
            if right == 0 {
                0
            } else {
                (((left as i32 as i64) * 65_536) / right as i32 as i64) as u32
            }
        }
        BinOp::Eq => u32::from(left == right),
        BinOp::Ne => u32::from(left != right),
        BinOp::Lt => u32::from((left as i32) < right as i32),
        BinOp::Le => u32::from((left as i32) <= right as i32),
        BinOp::Gt => u32::from((left as i32) > right as i32),
        BinOp::Ge => u32::from((left as i32) >= right as i32),
        BinOp::BitOr | BinOp::BitXor | BinOp::BitAnd | BinOp::Shl | BinOp::Shr | BinOp::Mod => {
            unreachable!("fixed operator rejected during resolution")
        }
    }
}

fn return_type_name(return_type: ReturnType) -> &'static str {
    match return_type {
        ReturnType::Void => "void",
        ReturnType::Byte => "byte",
        ReturnType::Int => "int",
        ReturnType::Fixed => "fixed",
    }
}

fn eval_const_binary(left: u8, op: BinOp, right: u8) -> u8 {
    match op {
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
    }
}

fn collect_stmt_call_types(stmts: &[ResolvedStmt], types: &mut Vec<ValueType>) {
    for stmt in stmts {
        collect_stmt_call_type(stmt, types);
    }
}

fn collect_stmt_call_type(stmt: &ResolvedStmt, types: &mut Vec<ValueType>) {
    match stmt {
        ResolvedStmt::Let { init, .. } => {
            if let Some(expr) = init {
                collect_expr_call_types(expr, types);
            }
        }
        ResolvedStmt::Const
        | ResolvedStmt::LetArray { .. }
        | ResolvedStmt::Puts(_)
        | ResolvedStmt::Read(_) => {}
        ResolvedStmt::Assign { expr, .. }
        | ResolvedStmt::Put(expr)
        | ResolvedStmt::Print(expr)
        | ResolvedStmt::Println(expr) => collect_expr_call_types(expr, types),
        ResolvedStmt::Call { args, .. } => {
            for arg in args {
                collect_expr_call_types(arg, types);
            }
        }
        ResolvedStmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_expr_call_types(expr, types);
            }
        }
        ResolvedStmt::ArraySet { index, expr, .. } => {
            collect_expr_call_types(index, types);
            collect_expr_call_types(expr, types);
        }
        ResolvedStmt::ReadArray { index, .. } => collect_expr_call_types(index, types),
        ResolvedStmt::Break | ResolvedStmt::Continue => {}
        ResolvedStmt::Block(stmts) | ResolvedStmt::Loop { body: stmts } => {
            collect_stmt_call_types(stmts, types);
        }
        ResolvedStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_expr_call_types(cond, types);
            collect_stmt_call_types(then_branch, types);
            collect_stmt_call_types(else_branch, types);
        }
        ResolvedStmt::While { cond, body } => {
            collect_expr_call_types(cond, types);
            collect_stmt_call_types(body, types);
        }
        ResolvedStmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(init) = init.as_deref() {
                collect_stmt_call_type(init, types);
            }
            if let Some(cond) = cond {
                collect_expr_call_types(cond, types);
            }
            if let Some(step) = step.as_deref() {
                collect_stmt_call_type(step, types);
            }
            collect_stmt_call_types(body, types);
        }
    }
}

fn collect_expr_call_types(expr: &ResolvedExpr, types: &mut Vec<ValueType>) {
    match expr {
        ResolvedExpr::Byte(_)
        | ResolvedExpr::Int(_)
        | ResolvedExpr::Fixed(_)
        | ResolvedExpr::Var(_) => {}
        ResolvedExpr::ArrayGet { index, .. } => collect_expr_call_types(index, types),
        ResolvedExpr::Call {
            args, value_type, ..
        } => {
            for arg in args {
                collect_expr_call_types(arg, types);
            }
            types.push(*value_type);
        }
        ResolvedExpr::Input { targets, .. } => {
            for target in targets {
                if let ResolvedInputTarget::Array { index, .. } = target {
                    collect_expr_call_types(index, types);
                }
            }
        }
        ResolvedExpr::Unary { expr, .. } => collect_expr_call_types(expr, types),
        ResolvedExpr::Binary { left, right, .. } => {
            collect_expr_call_types(left, types);
            collect_expr_call_types(right, types);
        }
        ResolvedExpr::Cast { expr, .. } => collect_expr_call_types(expr, types),
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
