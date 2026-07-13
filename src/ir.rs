use crate::sema::{ResolvedExpr, ResolvedProgram, ResolvedStmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub blocks: Vec<Block>,
    pub entry: BlockId,
    pub function_entries: Vec<BlockId>,
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
    StoreReturn(usize),
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
        builder.set_terminator_if_open(end, Terminator::Return(ResolvedExpr::Byte(0)));
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
    spills: Vec<usize>,
    next_spill: usize,
}

impl Builder {
    fn new(spills: Vec<usize>) -> Self {
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
            ResolvedStmt::Let { cell, init } => {
                let (current, init) = if let Some(init) = init {
                    let (current, init) = self.lower_expr_calls(current, init);
                    (current, Some(init))
                } else {
                    (current, None)
                };
                self.push_op(current, Op::Let { cell: *cell, init });
                current
            }
            ResolvedStmt::LetArray { base, len, init } => {
                self.push_op(
                    current,
                    Op::LetArray {
                        base: *base,
                        len: *len,
                        init: init.clone(),
                    },
                );
                current
            }
            ResolvedStmt::Assign { cell, expr } => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(current, Op::Assign { cell: *cell, expr });
                current
            }
            ResolvedStmt::ArraySet {
                base,
                len,
                index,
                expr,
            } => {
                let (current, index) = self.lower_expr_calls(current, index);
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.push_op(
                    current,
                    Op::ArraySet {
                        base: *base,
                        len: *len,
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
            ResolvedStmt::Read(cell) => {
                self.push_op(current, Op::Read(*cell));
                current
            }
            ResolvedStmt::ReadArray { base, len, index } => {
                let (current, index) = self.lower_expr_calls(current, index);
                self.push_op(
                    current,
                    Op::ReadArray {
                        base: *base,
                        len: *len,
                        index,
                    },
                );
                current
            }
            ResolvedStmt::Return(expr) => {
                let (current, expr) = self.lower_expr_calls(current, expr);
                self.set_terminator_if_open(current, Terminator::Return(expr));
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
            ResolvedExpr::Byte(_) | ResolvedExpr::Var(_) => (current, expr.clone()),
            ResolvedExpr::ArrayGet { base, len, index } => {
                let (current, index) = self.lower_expr_calls(current, index);
                (
                    current,
                    ResolvedExpr::ArrayGet {
                        base: *base,
                        len: *len,
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
            ResolvedExpr::Call { function, args } => {
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
                (after, ResolvedExpr::Var(spill))
            }
        }
    }

    fn alloc_spill(&mut self) -> usize {
        let cell = self
            .spills
            .get(self.next_spill)
            .copied()
            .expect("semantic analysis reserves enough spill cells");
        self.next_spill += 1;
        cell
    }

    fn set_terminator_if_open(&mut self, block: BlockId, terminator: Terminator) {
        if matches!(self.blocks[block.0].terminator, Terminator::Halt) {
            self.blocks[block.0].terminator = terminator;
        }
    }
}
