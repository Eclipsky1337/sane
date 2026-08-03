use crate::ast::ReturnType;
use crate::sema::{ArrayLayout, ResolvedExpr, ResolvedProgram, ResolvedStmt, ValueLayout};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub blocks: Vec<Block>,
    pub entry: BlockId,
    pub function_entries: Vec<BlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrStats {
    pub blocks: usize,
    pub reachable_blocks: usize,
    pub ops: usize,
    pub empty_blocks: usize,
    pub trivial_jumps: usize,
    pub calls: usize,
    pub branches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    pub blocks: Vec<BlockId>,
}

impl Program {
    pub fn stats(&self) -> IrStats {
        let mut reachable = vec![false; self.blocks.len()];
        let mut pending = vec![self.entry];

        while let Some(block_id) = pending.pop() {
            if reachable[block_id.0] {
                continue;
            }
            reachable[block_id.0] = true;

            match &self.blocks[block_id.0].terminator {
                Terminator::Jump(target) => pending.push(*target),
                Terminator::Branch {
                    then_target,
                    else_target,
                    ..
                } => {
                    pending.push(*then_target);
                    pending.push(*else_target);
                }
                Terminator::Call {
                    function,
                    return_target,
                    ..
                } => {
                    pending.push(self.function_entries[*function]);
                    pending.push(*return_target);
                }
                Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
            }
        }

        IrStats {
            blocks: self.blocks.len(),
            reachable_blocks: reachable.into_iter().filter(|reachable| *reachable).count(),
            ops: self.blocks.iter().map(|block| block.ops.len()).sum(),
            empty_blocks: self
                .blocks
                .iter()
                .filter(|block| block.ops.is_empty())
                .count(),
            trivial_jumps: self
                .blocks
                .iter()
                .filter(|block| block.ops.is_empty())
                .filter(|block| matches!(block.terminator, Terminator::Jump(_)))
                .count(),
            calls: self
                .blocks
                .iter()
                .filter(|block| matches!(block.terminator, Terminator::Call { .. }))
                .count(),
            branches: self
                .blocks
                .iter()
                .filter(|block| matches!(block.terminator, Terminator::Branch { .. }))
                .count(),
        }
    }

    pub fn superblocks(&self) -> Vec<Superblock> {
        let mut predecessor_counts = vec![0usize; self.blocks.len()];
        for block in &self.blocks {
            match block.terminator {
                Terminator::Jump(target) => predecessor_counts[target.0] += 1,
                Terminator::Branch {
                    then_target,
                    else_target,
                    ..
                } => {
                    predecessor_counts[then_target.0] += 1;
                    if else_target != then_target {
                        predecessor_counts[else_target.0] += 1;
                    }
                }
                Terminator::Call {
                    function,
                    return_target,
                    ..
                } => {
                    let function_entry = self.function_entries[function];
                    predecessor_counts[function_entry.0] += 1;
                    if return_target != function_entry {
                        predecessor_counts[return_target.0] += 1;
                    }
                }
                Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
            }
        }

        let mut entry_points = vec![false; self.blocks.len()];
        entry_points[self.entry.0] = true;
        for entry in &self.function_entries {
            entry_points[entry.0] = true;
        }

        let mut chained_targets = vec![false; self.blocks.len()];
        for block in &self.blocks {
            if let Terminator::Jump(target) = block.terminator
                && predecessor_counts[target.0] == 1
                && !entry_points[target.0]
            {
                chained_targets[target.0] = true;
            }
        }

        let mut included = vec![false; self.blocks.len()];
        let mut superblocks = Vec::new();
        for block_index in 0..self.blocks.len() {
            if !chained_targets[block_index] {
                superblocks.push(self.build_superblock(
                    BlockId(block_index),
                    &chained_targets,
                    &mut included,
                ));
            }
        }
        for block_index in 0..self.blocks.len() {
            if !included[block_index] {
                superblocks.push(self.build_superblock(
                    BlockId(block_index),
                    &chained_targets,
                    &mut included,
                ));
            }
        }
        superblocks
    }

    fn build_superblock(
        &self,
        start: BlockId,
        chained_targets: &[bool],
        included: &mut [bool],
    ) -> Superblock {
        let mut blocks = Vec::new();
        let mut current = start;

        loop {
            if included[current.0] {
                break;
            }
            included[current.0] = true;
            blocks.push(current);

            let Terminator::Jump(target) = self.blocks[current.0].terminator else {
                break;
            };
            if !chained_targets[target.0] || included[target.0] {
                break;
            }
            current = target;
        }

        Superblock { blocks }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub ops: Vec<Op>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
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
    StoreReturn(ValueLayout),
    PutReturn,
    PrintReturn,
    PrintlnReturn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jump(BlockId),
    Branch {
        cond: ResolvedExpr,
        then_target: BlockId,
        else_target: BlockId,
    },
    Call {
        function: usize,
        args: Vec<ResolvedExpr>,
        return_target: BlockId,
    },
    Return(ResolvedExpr),
    ReturnValue,
    Halt,
}

pub fn lower(program: &ResolvedProgram) -> Program {
    let mut builder = Builder::new(program.spills.clone());
    let function_entries = program
        .functions
        .iter()
        .map(|_| builder.new_block())
        .collect::<Vec<_>>();
    let entry = builder.new_block();
    let end = builder.lower_stmts(&program.stmts, entry, &mut Vec::new());
    builder.set_terminator_if_open(end, Terminator::Halt);

    for (function, entry) in program
        .functions
        .iter()
        .zip(function_entries.iter().copied())
    {
        let end = builder.lower_stmts(&function.body, entry, &mut Vec::new());
        let terminator = match function.return_type {
            ReturnType::Void => Terminator::ReturnValue,
            ReturnType::Byte => Terminator::Return(ResolvedExpr::Byte(0)),
            ReturnType::Int => Terminator::Return(ResolvedExpr::Int(0)),
            ReturnType::Fixed => Terminator::Return(ResolvedExpr::Fixed(0)),
        };
        builder.set_terminator_if_open(end, terminator);
    }

    Program {
        blocks: builder.blocks,
        entry,
        function_entries,
    }
}

#[derive(Debug, Clone, Copy)]
struct LoopTargets {
    break_target: BlockId,
    continue_target: BlockId,
}

struct Builder {
    blocks: Vec<Block>,
    spills: Vec<ValueLayout>,
    next_spill: usize,
}

impl Builder {
    fn new(spills: Vec<ValueLayout>) -> Self {
        Self {
            blocks: Vec::new(),
            spills,
            next_spill: 0,
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(Block {
            ops: Vec::new(),
            terminator: Terminator::Halt,
        });
        id
    }

    fn lower_stmts(
        &mut self,
        stmts: &[ResolvedStmt],
        mut current: BlockId,
        loops: &mut Vec<LoopTargets>,
    ) -> BlockId {
        for stmt in stmts {
            current = self.lower_stmt(stmt, current, loops);
        }
        current
    }

    fn lower_stmt(
        &mut self,
        stmt: &ResolvedStmt,
        current: BlockId,
        loops: &mut Vec<LoopTargets>,
    ) -> BlockId {
        match stmt {
            ResolvedStmt::Const => current,
            ResolvedStmt::Let { layout, init } => {
                let (current, init) = if let Some(init) = init {
                    let (current, init) = self.lower_expr_calls(current, init);
                    (current, Some(init))
                } else {
                    (current, None)
                };
                self.push_op(
                    current,
                    Op::Let {
                        layout: *layout,
                        init,
                    },
                );
                current
            }
            ResolvedStmt::LetArray { layout, init } => {
                self.push_op(
                    current,
                    Op::LetArray {
                        layout: *layout,
                        init: init.clone(),
                    },
                );
                current
            }
            ResolvedStmt::Assign { layout, expr } => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(
                    current,
                    Op::Assign {
                        layout: *layout,
                        expr,
                    },
                );
                current
            }
            ResolvedStmt::ArraySet {
                layout,
                index,
                expr,
            } => {
                let (current, index) = self.lower_expr_calls(current, index);
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(
                    current,
                    Op::ArraySet {
                        layout: *layout,
                        index,
                        expr,
                    },
                );
                current
            }
            ResolvedStmt::Put(expr) => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(current, Op::Put(expr));
                current
            }
            ResolvedStmt::Puts(bytes) => {
                self.push_op(current, Op::Puts(bytes.clone()));
                current
            }
            ResolvedStmt::Print(expr) => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(current, Op::Print(expr));
                current
            }
            ResolvedStmt::Println(expr) => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(current, Op::Println(expr));
                current
            }
            ResolvedStmt::Read(layout) => {
                self.push_op(current, Op::Read(*layout));
                current
            }
            ResolvedStmt::ReadArray { layout, index } => {
                let (current, index) = self.lower_expr_calls(current, index);
                self.push_op(
                    current,
                    Op::ReadArray {
                        layout: *layout,
                        index,
                    },
                );
                current
            }
            ResolvedStmt::Call { function, args } => {
                let mut current = current;
                let mut lowered_args = Vec::new();
                for arg in args {
                    let (next, arg) = self.lower_expr_calls(current, arg);
                    current = next;
                    lowered_args.push(arg);
                }
                let after = self.new_block();
                self.set_terminator_if_open(
                    current,
                    Terminator::Call {
                        function: *function,
                        args: lowered_args,
                        return_target: after,
                    },
                );
                after
            }
            ResolvedStmt::Return(expr) => {
                if let Some(expr) = expr {
                    let (current, expr) = self.lower_expr_calls(current, expr);
                    self.set_terminator_if_open(current, Terminator::Return(expr));
                } else {
                    self.set_terminator_if_open(current, Terminator::ReturnValue);
                }
                self.new_block()
            }
            ResolvedStmt::Break => {
                let target = loops
                    .last()
                    .expect("semantic analysis rejects break outside loops")
                    .break_target;
                self.set_terminator_if_open(current, Terminator::Jump(target));
                self.new_block()
            }
            ResolvedStmt::Continue => {
                let target = loops
                    .last()
                    .expect("semantic analysis rejects continue outside loops")
                    .continue_target;
                self.set_terminator_if_open(current, Terminator::Jump(target));
                self.new_block()
            }
            ResolvedStmt::Block(stmts) => self.lower_stmts(stmts, current, loops),
            ResolvedStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let (current, cond) = self.lower_expr_calls(current, cond);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let after = self.new_block();
                self.set_terminator_if_open(
                    current,
                    Terminator::Branch {
                        cond,
                        then_target: then_block,
                        else_target: else_block,
                    },
                );

                let then_end = self.lower_stmts(then_branch, then_block, loops);
                self.set_terminator_if_open(then_end, Terminator::Jump(after));

                let else_end = self.lower_stmts(else_branch, else_block, loops);
                self.set_terminator_if_open(else_end, Terminator::Jump(after));

                after
            }
            ResolvedStmt::While { cond, body } => {
                let cond_block = self.new_block();
                let body_block = self.new_block();
                let after = self.new_block();
                self.set_terminator_if_open(current, Terminator::Jump(cond_block));
                let (cond_end, cond) = self.lower_expr_calls(cond_block, cond);
                self.set_terminator_if_open(
                    cond_end,
                    Terminator::Branch {
                        cond,
                        then_target: body_block,
                        else_target: after,
                    },
                );

                loops.push(LoopTargets {
                    break_target: after,
                    continue_target: cond_block,
                });
                let body_end = self.lower_stmts(body, body_block, loops);
                loops.pop();
                self.set_terminator_if_open(body_end, Terminator::Jump(cond_block));
                after
            }
            ResolvedStmt::Loop { body } => {
                let body_block = self.new_block();
                let after = self.new_block();
                self.set_terminator_if_open(current, Terminator::Jump(body_block));

                loops.push(LoopTargets {
                    break_target: after,
                    continue_target: body_block,
                });
                let body_end = self.lower_stmts(body, body_block, loops);
                loops.pop();
                self.set_terminator_if_open(body_end, Terminator::Jump(body_block));
                after
            }
            ResolvedStmt::For {
                init,
                cond,
                step,
                body,
            } => {
                let current = if let Some(init) = init {
                    self.lower_stmt(init, current, loops)
                } else {
                    current
                };
                let cond_block = self.new_block();
                let body_block = self.new_block();
                let step_block = self.new_block();
                let after = self.new_block();
                self.set_terminator_if_open(current, Terminator::Jump(cond_block));
                if let Some(cond) = cond {
                    let (cond_end, cond) = self.lower_expr_calls(cond_block, cond);
                    self.set_terminator_if_open(
                        cond_end,
                        Terminator::Branch {
                            cond,
                            then_target: body_block,
                            else_target: after,
                        },
                    );
                } else {
                    self.set_terminator_if_open(cond_block, Terminator::Jump(body_block));
                }

                loops.push(LoopTargets {
                    break_target: after,
                    continue_target: step_block,
                });
                let body_end = self.lower_stmts(body, body_block, loops);
                loops.pop();
                self.set_terminator_if_open(body_end, Terminator::Jump(step_block));

                let step_end = if let Some(step) = step {
                    self.lower_stmt(step, step_block, loops)
                } else {
                    step_block
                };
                self.set_terminator_if_open(step_end, Terminator::Jump(cond_block));
                after
            }
        }
    }

    fn push_op(&mut self, block: BlockId, op: Op) {
        self.blocks[block.0].ops.push(op);
    }

    fn lower_expr_calls(
        &mut self,
        current: BlockId,
        expr: &ResolvedExpr,
    ) -> (BlockId, ResolvedExpr) {
        match expr {
            ResolvedExpr::Byte(_)
            | ResolvedExpr::Int(_)
            | ResolvedExpr::Fixed(_)
            | ResolvedExpr::Var(_) => (current, expr.clone()),
            ResolvedExpr::ArrayGet { layout, index } => {
                let (current, index) = self.lower_expr_calls(current, index);
                (
                    current,
                    ResolvedExpr::ArrayGet {
                        layout: *layout,
                        index: Box::new(index),
                    },
                )
            }
            ResolvedExpr::Unary { op, expr } => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                (
                    current,
                    ResolvedExpr::Unary {
                        op: *op,
                        expr: Box::new(expr),
                    },
                )
            }
            ResolvedExpr::Binary { left, op, right } => {
                let (current, left) = self.lower_expr_calls(current, left);
                let (current, right) = self.lower_expr_calls(current, right);
                (
                    current,
                    ResolvedExpr::Binary {
                        left: Box::new(left),
                        op: *op,
                        right: Box::new(right),
                    },
                )
            }
            ResolvedExpr::Cast { expr, target } => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                (
                    current,
                    ResolvedExpr::Cast {
                        expr: Box::new(expr),
                        target: *target,
                    },
                )
            }
            ResolvedExpr::Call {
                function,
                args,
                value_type,
            } => {
                let mut current = current;
                let mut lowered_args = Vec::new();
                for arg in args {
                    let (next, arg) = self.lower_expr_calls(current, arg);
                    current = next;
                    lowered_args.push(arg);
                }
                let spill = self.alloc_spill();
                let after = self.new_block();
                self.set_terminator_if_open(
                    current,
                    Terminator::Call {
                        function: *function,
                        args: lowered_args,
                        return_target: after,
                    },
                );
                self.push_op(after, Op::StoreReturn(spill));
                debug_assert_eq!(spill.value_type, *value_type);
                (after, ResolvedExpr::Var(spill))
            }
        }
    }

    fn alloc_spill(&mut self) -> ValueLayout {
        let layout = self
            .spills
            .get(self.next_spill)
            .copied()
            .expect("semantic analysis reserves enough spill cells");
        self.next_spill += 1;
        layout
    }

    fn set_terminator_if_open(&mut self, block: BlockId, terminator: Terminator) {
        if matches!(self.blocks[block.0].terminator, Terminator::Halt) {
            self.blocks[block.0].terminator = terminator;
        }
    }
}
