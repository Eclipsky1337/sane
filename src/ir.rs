use crate::ast::ReturnType;
use crate::sema::{
    ArrayLayout, ResolvedExpr, ResolvedInputTarget, ResolvedProgram, ResolvedStmt, ValueLayout,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub blocks: Vec<Block>,
    pub entry: BlockId,
    pub function_entries: Vec<BlockId>,
    pub function_indices: Vec<usize>,
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
        function_indices: (0..program.functions.len()).collect(),
    }
    .remove_unreachable_blocks()
    .remove_trivial_jump_blocks()
    .fold_identical_branch_targets()
    .remove_trivial_jump_blocks()
    .merge_identical_blocks()
    .fold_identical_branch_targets()
    .remove_trivial_jump_blocks()
    .remove_unused_functions()
}

impl Program {
    fn remove_unreachable_blocks(self) -> Self {
        let mut reachable = vec![false; self.blocks.len()];
        let mut pending = Vec::with_capacity(self.function_entries.len() + 1);
        pending.push(self.entry);
        pending.extend(self.function_entries.iter().copied());

        while let Some(block_id) = pending.pop() {
            if reachable[block_id.0] {
                continue;
            }
            reachable[block_id.0] = true;

            match self.blocks[block_id.0].terminator {
                Terminator::Jump(target) => pending.push(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                    ..
                } => {
                    pending.push(then_target);
                    pending.push(else_target);
                }
                Terminator::Call {
                    function,
                    return_target,
                    ..
                } => {
                    pending.push(self.function_entries[function]);
                    pending.push(return_target);
                }
                Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
            }
        }

        self.retain_blocks(&reachable)
    }

    fn remove_trivial_jump_blocks(mut self) -> Self {
        let mut protected = vec![false; self.blocks.len()];
        protected[self.entry.0] = true;
        for entry in &self.function_entries {
            protected[entry.0] = true;
        }

        let candidates = self
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                !protected[index]
                    && block.ops.is_empty()
                    && matches!(block.terminator, Terminator::Jump(_))
            })
            .collect::<Vec<_>>();
        let final_targets = (0..self.blocks.len())
            .map(|index| self.final_jump_target(BlockId(index), &candidates))
            .collect::<Vec<_>>();

        for block in &mut self.blocks {
            thread_terminator_targets(&mut block.terminator, &final_targets);
        }

        let retained = candidates
            .iter()
            .zip(&final_targets)
            .map(|(candidate, final_target)| !candidate || final_target.is_none())
            .collect::<Vec<_>>();
        self.retain_blocks(&retained)
    }

    fn fold_identical_branch_targets(mut self) -> Self {
        for block in &mut self.blocks {
            if let Terminator::Branch {
                then_target,
                else_target,
                ..
            } = block.terminator
                && then_target == else_target
            {
                block.terminator = Terminator::Jump(then_target);
            }
        }
        self
    }

    fn merge_identical_blocks(mut self) -> Self {
        let mut protected = vec![false; self.blocks.len()];
        protected[self.entry.0] = true;
        for entry in &self.function_entries {
            protected[entry.0] = true;
        }

        let mut canonical = (0..self.blocks.len()).map(BlockId).collect::<Vec<_>>();
        for block_index in 0..self.blocks.len() {
            if protected[block_index] {
                continue;
            }
            if let Some(existing) = (0..block_index).find(|existing| {
                !protected[*existing]
                    && canonical[*existing].0 == *existing
                    && self.blocks[*existing] == self.blocks[block_index]
            }) {
                canonical[block_index] = BlockId(existing);
            }
        }

        if canonical
            .iter()
            .enumerate()
            .all(|(index, canonical)| canonical.0 == index)
        {
            return self;
        }

        for block in &mut self.blocks {
            remap_terminator_blocks(&mut block.terminator, &canonical);
        }
        let retained = canonical
            .iter()
            .enumerate()
            .map(|(index, canonical)| canonical.0 == index)
            .collect::<Vec<_>>();
        self.retain_blocks(&retained)
    }

    fn remove_unused_functions(mut self) -> Self {
        let mut reachable = vec![false; self.blocks.len()];
        let mut used_functions = vec![false; self.function_entries.len()];
        let mut pending = vec![self.entry];

        while let Some(block_id) = pending.pop() {
            if reachable[block_id.0] {
                continue;
            }
            reachable[block_id.0] = true;

            match self.blocks[block_id.0].terminator {
                Terminator::Jump(target) => pending.push(target),
                Terminator::Branch {
                    then_target,
                    else_target,
                    ..
                } => {
                    pending.push(then_target);
                    pending.push(else_target);
                }
                Terminator::Call {
                    function,
                    return_target,
                    ..
                } => {
                    used_functions[function] = true;
                    pending.push(self.function_entries[function]);
                    pending.push(return_target);
                }
                Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
            }
        }

        let mut function_remap = vec![0usize; self.function_entries.len()];
        let mut next_function = 0;
        for (function, used) in used_functions.iter().copied().enumerate() {
            if used {
                function_remap[function] = next_function;
                next_function += 1;
            }
        }
        for block in &mut self.blocks {
            if let Terminator::Call { function, .. } = &mut block.terminator {
                *function = function_remap[*function];
            }
        }

        self.function_entries = self
            .function_entries
            .into_iter()
            .zip(&used_functions)
            .filter_map(|(entry, used)| used.then_some(entry))
            .collect();
        self.function_indices = self
            .function_indices
            .into_iter()
            .zip(&used_functions)
            .filter_map(|(index, used)| used.then_some(index))
            .collect();
        self.retain_blocks(&reachable)
    }

    fn final_jump_target(&self, start: BlockId, candidates: &[bool]) -> Option<BlockId> {
        let mut visited = vec![false; self.blocks.len()];
        let mut current = start;

        while candidates[current.0] {
            if visited[current.0] {
                return None;
            }
            visited[current.0] = true;
            let Terminator::Jump(target) = self.blocks[current.0].terminator else {
                unreachable!("trivial jump candidates always end in a jump");
            };
            current = target;
        }

        Some(current)
    }

    fn retain_blocks(self, retained: &[bool]) -> Self {
        let mut remap = vec![BlockId(0); self.blocks.len()];
        let mut blocks = Vec::with_capacity(retained.iter().filter(|keep| **keep).count());

        for (old_index, (block, keep)) in self.blocks.into_iter().zip(retained).enumerate() {
            if *keep {
                remap[old_index] = BlockId(blocks.len());
                blocks.push(block);
            }
        }

        for block in &mut blocks {
            remap_terminator_blocks(&mut block.terminator, &remap);
        }

        Self {
            blocks,
            entry: remap[self.entry.0],
            function_entries: self
                .function_entries
                .into_iter()
                .map(|entry| remap[entry.0])
                .collect(),
            function_indices: self.function_indices,
        }
    }
}

fn remap_terminator_blocks(terminator: &mut Terminator, remap: &[BlockId]) {
    match terminator {
        Terminator::Jump(target) => *target = remap[target.0],
        Terminator::Branch {
            then_target,
            else_target,
            ..
        } => {
            *then_target = remap[then_target.0];
            *else_target = remap[else_target.0];
        }
        Terminator::Call { return_target, .. } => {
            *return_target = remap[return_target.0];
        }
        Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
    }
}

fn thread_terminator_targets(terminator: &mut Terminator, final_targets: &[Option<BlockId>]) {
    let thread = |target: &mut BlockId| {
        if let Some(final_target) = final_targets[target.0] {
            *target = final_target;
        }
    };

    match terminator {
        Terminator::Jump(target) => thread(target),
        Terminator::Branch {
            then_target,
            else_target,
            ..
        } => {
            thread(then_target);
            thread(else_target);
        }
        Terminator::Call { return_target, .. } => thread(return_target),
        Terminator::Return(_) | Terminator::ReturnValue | Terminator::Halt => {}
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
            ResolvedExpr::Input {
                parts,
                specs,
                targets,
            } => {
                let mut current = current;
                let mut lowered_targets = Vec::new();
                for target in targets {
                    match target {
                        ResolvedInputTarget::Scalar(layout) => {
                            lowered_targets.push(ResolvedInputTarget::Scalar(*layout));
                        }
                        ResolvedInputTarget::Array { layout, index } => {
                            let (next, index) = self.lower_expr_calls(current, index);
                            current = next;
                            lowered_targets.push(ResolvedInputTarget::Array {
                                layout: *layout,
                                index: Box::new(index),
                            });
                        }
                    }
                }
                (
                    current,
                    ResolvedExpr::Input {
                        parts: parts.clone(),
                        specs: specs.clone(),
                        targets: lowered_targets,
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
