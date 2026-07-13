use crate::ast::{BinOp, UnOp};
use crate::ir::{BlockId, Op, Terminator};
use crate::sema::{
    MAX_CALL_DEPTH, ResolvedExpr as Expr, ResolvedProgram, ResolvedStmt as Stmt, WORK_CELL_COUNT,
};

const T0: usize = 0;
const GENERAL_TEMP_COUNT: usize = 8;

pub fn compile(program: &ResolvedProgram) -> Result<String, String> {
    if !program.functions.is_empty() {
        return Err("functions require the pc backend (`-b pc`)".to_string());
    }
    let mut out = BfOut::new(program.symbols.scratch_base());
    emit_stmts(&program.stmts, &mut out, None)?;
    Ok(optimize_bf(&out.code))
}

pub fn compile_pc(program: &ResolvedProgram) -> Result<String, String> {
    let ir = crate::ir::lower(program);
    if ir.blocks.len() > u8::MAX as usize + 1 {
        return Err("pc backend supports at most 256 basic blocks".to_string());
    }

    const PC_RESERVED_CELLS: usize = 7 + MAX_CALL_DEPTH;
    let mut out = BfOut::with_control_depth(program.symbols.scratch_base(), PC_RESERVED_CELLS);
    let rt = PcRuntime {
        pc: out.control_base(),
        running: out.control_base() + 1,
        dispatch_pc: out.control_base() + 2,
        matched: out.control_base() + 3,
        expected: out.control_base() + 4,
        rv: out.control_base() + 5,
        call_depth: out.control_base() + 6,
        ret_base: out.control_base() + 7,
    };

    out.clear(rt.pc);
    out.set_const(rt.pc, block_value(ir.entry)?);
    out.clear(rt.running);
    out.add_const(rt.running, 1);
    out.clear(rt.call_depth);
    out.clear(rt.rv);
    for depth in 0..MAX_CALL_DEPTH {
        out.clear(rt.ret_base + depth);
    }

    out.goto(rt.running);
    out.code.push('[');
    out.clear(rt.dispatch_pc);
    out.copy_add(rt.pc, rt.dispatch_pc, T0);

    for (index, block) in ir.blocks.iter().enumerate() {
        out.clear(rt.matched);
        out.copy_add(rt.dispatch_pc, rt.matched, T0);
        out.clear(rt.expected);
        out.set_const(rt.expected, index as u8);
        out.eq(rt.matched, rt.expected);

        out.goto(rt.matched);
        out.code.push('[');
        out.clear(rt.matched);
        for op in &block.ops {
            emit_op(op, &mut out, rt)?;
        }
        emit_terminator(&block.terminator, &ir, program, &mut out, rt)?;
        out.goto(rt.matched);
        out.code.push(']');
    }

    out.goto(rt.running);
    out.code.push(']');

    Ok(optimize_bf(&out.code))
}

#[derive(Debug, Clone, Copy)]
struct PcRuntime {
    pc: usize,
    running: usize,
    dispatch_pc: usize,
    matched: usize,
    expected: usize,
    rv: usize,
    call_depth: usize,
    ret_base: usize,
}

fn emit_op(op: &Op, out: &mut BfOut, rt: PcRuntime) -> Result<(), String> {
    match op {
        Op::Let { cell, init } => emit_basic_stmt(
            &Stmt::Let {
                cell: *cell,
                init: init.clone(),
            },
            out,
        ),
        Op::LetArray { base, len, init } => emit_basic_stmt(
            &Stmt::LetArray {
                base: *base,
                len: *len,
                init: init.clone(),
            },
            out,
        ),
        Op::Assign { cell, expr } => emit_basic_stmt(
            &Stmt::Assign {
                cell: *cell,
                expr: expr.clone(),
            },
            out,
        ),
        Op::ArraySet {
            base,
            len,
            index,
            expr,
        } => emit_basic_stmt(
            &Stmt::ArraySet {
                base: *base,
                len: *len,
                index: index.clone(),
                expr: expr.clone(),
            },
            out,
        ),
        Op::Put(expr) => emit_basic_stmt(&Stmt::Put(expr.clone()), out),
        Op::Puts(bytes) => emit_basic_stmt(&Stmt::Puts(bytes.clone()), out),
        Op::Print(expr) => emit_basic_stmt(&Stmt::Print(expr.clone()), out),
        Op::Println(expr) => emit_basic_stmt(&Stmt::Println(expr.clone()), out),
        Op::Read(cell) => emit_basic_stmt(&Stmt::Read(*cell), out),
        Op::ReadArray { base, len, index } => emit_basic_stmt(
            &Stmt::ReadArray {
                base: *base,
                len: *len,
                index: index.clone(),
            },
            out,
        ),
        Op::StoreReturn(cell) => {
            out.clear(*cell);
            out.copy_add(rt.rv, *cell, T0);
            Ok(())
        }
        Op::PutReturn => {
            out.put(rt.rv);
            Ok(())
        }
        Op::PrintReturn => {
            out.print_byte_decimal(rt.rv);
            Ok(())
        }
        Op::PrintlnReturn => {
            out.print_byte_decimal(rt.rv);
            out.put_byte_const(b'\n');
            Ok(())
        }
    }
}

fn emit_terminator(
    terminator: &Terminator,
    ir: &crate::ir::Program,
    program: &ResolvedProgram,
    out: &mut BfOut,
    rt: PcRuntime,
) -> Result<(), String> {
    match terminator {
        Terminator::Jump(target) => {
            out.clear(rt.pc);
            out.set_const(rt.pc, block_value(*target)?);
        }
        Terminator::Branch {
            cond,
            then_target,
            else_target,
        } => {
            let cond_cell = out.alloc_control_cell();
            let else_flag = out.alloc_control_cell();
            emit_expr_to(cond, cond_cell, out, &[cond_cell, else_flag])?;
            out.boolify(cond_cell, T0);
            out.clear(rt.pc);
            out.clear(else_flag);
            out.add_const(else_flag, 1);

            out.goto(cond_cell);
            out.code.push('[');
            out.clear(cond_cell);
            out.set_const(rt.pc, block_value(*then_target)?);
            out.clear(else_flag);
            out.goto(cond_cell);
            out.code.push(']');

            out.goto(else_flag);
            out.code.push('[');
            out.clear(else_flag);
            out.set_const(rt.pc, block_value(*else_target)?);
            out.goto(else_flag);
            out.code.push(']');
            out.free_control_cells(2);
        }
        Terminator::Call {
            function,
            args,
            return_target,
        } => {
            let function_index = *function;
            set_ret_pc(out, rt, block_value(*return_target)?)?;
            out.add_const(rt.call_depth, 1);
            let function = program
                .functions
                .get(function_index)
                .ok_or_else(|| "internal error: invalid function index".to_string())?;
            if function.params.len() != args.len() {
                return Err("internal error: argument count mismatch".to_string());
            }
            for (arg, param) in args.iter().zip(&function.params) {
                emit_expr_to(arg, *param, out, &[*param])?;
            }
            let entry = ir
                .function_entries
                .get(function_index)
                .copied()
                .ok_or_else(|| "internal error: missing function entry".to_string())?;
            out.clear(rt.pc);
            out.set_const(rt.pc, block_value(entry)?);
        }
        Terminator::Return(expr) => {
            emit_expr_to(expr, rt.rv, out, &[rt.rv])?;
            out.sub_const(rt.call_depth, 1);
            load_ret_pc(out, rt)?;
        }
        Terminator::ReturnValue => {
            out.sub_const(rt.call_depth, 1);
            load_ret_pc(out, rt)?;
        }
        Terminator::Halt => {
            out.clear(rt.pc);
            out.clear(rt.running);
        }
    }
    Ok(())
}

fn set_ret_pc(out: &mut BfOut, rt: PcRuntime, value: u8) -> Result<(), String> {
    for depth in 0..MAX_CALL_DEPTH {
        out.clear(rt.matched);
        out.copy_add(rt.call_depth, rt.matched, T0);
        out.clear(rt.expected);
        out.set_const(rt.expected, depth as u8);
        out.eq(rt.matched, rt.expected);
        out.goto(rt.matched);
        out.code.push('[');
        out.clear(rt.matched);
        out.clear(rt.ret_base + depth);
        out.set_const(rt.ret_base + depth, value);
        out.goto(rt.matched);
        out.code.push(']');
    }
    Ok(())
}

fn load_ret_pc(out: &mut BfOut, rt: PcRuntime) -> Result<(), String> {
    out.clear(rt.pc);
    for depth in 0..MAX_CALL_DEPTH {
        out.clear(rt.matched);
        out.copy_add(rt.call_depth, rt.matched, T0);
        out.clear(rt.expected);
        out.set_const(rt.expected, depth as u8);
        out.eq(rt.matched, rt.expected);
        out.goto(rt.matched);
        out.code.push('[');
        out.clear(rt.matched);
        out.copy_add(rt.ret_base + depth, rt.pc, T0);
        out.goto(rt.matched);
        out.code.push(']');
    }
    Ok(())
}

fn block_value(block: BlockId) -> Result<u8, String> {
    u8::try_from(block.0).map_err(|_| "pc backend supports at most 256 basic blocks".to_string())
}

#[derive(Debug, Clone, Copy)]
struct LoopContext {
    guard: usize,
    active: usize,
    recheck: usize,
    continue_target: Option<usize>,
}

fn emit_stmts(
    stmts: &[Stmt],
    out: &mut BfOut,
    loop_ctx: Option<LoopContext>,
) -> Result<(), String> {
    for stmt in stmts {
        if let Some(ctx) = loop_ctx {
            emit_controlled_stmt(stmt, out, ctx)?;
        } else {
            emit_stmt(stmt, out, None)?;
        }
    }
    Ok(())
}

fn emit_controlled_stmt(stmt: &Stmt, out: &mut BfOut, ctx: LoopContext) -> Result<(), String> {
    let exec = out.alloc_control_cell();
    out.clear(exec);
    out.copy_add(ctx.active, exec, T0);
    out.goto(exec);
    out.code.push('[');
    out.clear(exec);
    emit_stmt(stmt, out, Some(ctx))?;
    out.goto(exec);
    out.code.push(']');
    out.free_control_cells(1);
    Ok(())
}

fn emit_stmt(stmt: &Stmt, out: &mut BfOut, loop_ctx: Option<LoopContext>) -> Result<(), String> {
    match stmt {
        Stmt::Let { cell, init } => {
            if let Some(expr) = init {
                emit_expr_to(expr, *cell, out, &[*cell])?;
            } else {
                out.clear(*cell);
            }
        }
        Stmt::LetArray { base, len, init } => {
            for cell in *base..*base + 4 + *len {
                out.clear(cell);
            }
            for (offset, value) in init.iter().copied().enumerate() {
                out.set_const(*base + 4 + offset, value);
            }
        }
        Stmt::Assign { cell, expr } => {
            emit_expr_to(expr, *cell, out, &[*cell])?;
        }
        Stmt::ArraySet {
            base,
            len,
            index,
            expr,
        } => emit_array_set(*base, *len, index, expr, out)?,
        Stmt::Put(expr) => {
            emit_expr_to(expr, T0, out, &[T0])?;
            out.put(T0);
        }
        Stmt::Puts(bytes) => out.put_bytes(bytes),
        Stmt::Print(expr) => {
            emit_expr_to(expr, T0, out, &[T0])?;
            out.print_byte_decimal(T0);
        }
        Stmt::Println(expr) => {
            emit_expr_to(expr, T0, out, &[T0])?;
            out.print_byte_decimal(T0);
            out.put_byte_const(b'\n');
        }
        Stmt::Read(cell) => out.read(*cell),
        Stmt::ReadArray { base, len, index } => emit_array_read(*base, *len, index, out)?,
        Stmt::Return(_) => {
            return Err("internal error: return reached structured backend".to_string());
        }
        Stmt::Break => {
            let ctx = loop_ctx.ok_or_else(|| "internal error: break outside loop".to_string())?;
            out.clear(ctx.active);
            out.clear(ctx.recheck);
            out.clear(ctx.guard);
            if let Some(continue_target) = ctx.continue_target {
                out.clear(continue_target);
            }
        }
        Stmt::Continue => {
            let ctx =
                loop_ctx.ok_or_else(|| "internal error: continue outside loop".to_string())?;
            out.clear(ctx.active);
            if let Some(continue_target) = ctx.continue_target {
                out.clear(continue_target);
                out.add_const(continue_target, 1);
            }
        }
        Stmt::Block(stmts) => emit_stmts(stmts, out, loop_ctx)?,
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => emit_if(cond, then_branch, else_branch, out, loop_ctx)?,
        Stmt::While { cond, body } => emit_while(cond, body, out)?,
        Stmt::Loop { body } => emit_for(None, None, None, body, out)?,
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => emit_for(init.as_deref(), cond.as_ref(), step.as_deref(), body, out)?,
    }
    Ok(())
}

fn emit_basic_stmt(stmt: &Stmt, out: &mut BfOut) -> Result<(), String> {
    match stmt {
        Stmt::Let { .. }
        | Stmt::LetArray { .. }
        | Stmt::Assign { .. }
        | Stmt::ArraySet { .. }
        | Stmt::Put(_)
        | Stmt::Puts(_)
        | Stmt::Print(_)
        | Stmt::Println(_)
        | Stmt::Read(_)
        | Stmt::ReadArray { .. } => emit_stmt(stmt, out, None),
        _ => Err("internal error: control-flow statement in basic block".to_string()),
    }
}

fn emit_if(
    cond: &Expr,
    then_branch: &[Stmt],
    else_branch: &[Stmt],
    out: &mut BfOut,
    loop_ctx: Option<LoopContext>,
) -> Result<(), String> {
    if else_branch.is_empty() {
        let guard = out.alloc_control_cell();
        emit_expr_to(cond, guard, out, &[guard])?;
        out.boolify(guard, T0);
        out.goto(guard);
        out.code.push('[');
        out.clear(guard);
        emit_stmts(then_branch, out, loop_ctx)?;
        out.goto(guard);
        out.code.push(']');
        out.free_control_cells(1);
        return Ok(());
    }

    let guard = out.alloc_control_cell();
    let else_flag = out.alloc_control_cell();
    emit_expr_to(cond, guard, out, &[guard, else_flag])?;
    out.boolify(guard, T0);
    out.clear(else_flag);
    out.add_const(else_flag, 1);

    out.goto(guard);
    out.code.push('[');
    out.clear(guard);
    emit_stmts(then_branch, out, loop_ctx)?;
    out.clear(else_flag);
    out.goto(guard);
    out.code.push(']');

    out.goto(else_flag);
    out.code.push('[');
    out.clear(else_flag);
    emit_stmts(else_branch, out, loop_ctx)?;
    out.goto(else_flag);
    out.code.push(']');

    out.free_control_cells(2);
    Ok(())
}

fn emit_while(cond: &Expr, body: &[Stmt], out: &mut BfOut) -> Result<(), String> {
    let guard = out.alloc_control_cell();
    let active = out.alloc_control_cell();
    let recheck = out.alloc_control_cell();
    let ctx = LoopContext {
        guard,
        active,
        recheck,
        continue_target: None,
    };

    emit_expr_to(cond, guard, out, &[guard])?;
    out.boolify(guard, T0);

    out.goto(guard);
    out.code.push('[');
    out.clear(active);
    out.add_const(active, 1);
    out.clear(recheck);
    out.add_const(recheck, 1);
    emit_stmts(body, out, Some(ctx))?;
    out.goto(recheck);
    out.code.push('[');
    out.clear(recheck);
    emit_expr_to(cond, guard, out, &[guard, active, recheck])?;
    out.boolify(guard, T0);
    out.goto(recheck);
    out.code.push(']');
    out.goto(guard);
    out.code.push(']');

    out.free_control_cells(3);
    Ok(())
}

fn emit_for(
    init: Option<&Stmt>,
    cond: Option<&Expr>,
    step: Option<&Stmt>,
    body: &[Stmt],
    out: &mut BfOut,
) -> Result<(), String> {
    if let Some(init) = init {
        emit_stmt(init, out, None)?;
    }

    let guard = out.alloc_control_cell();
    let active = out.alloc_control_cell();
    let recheck = out.alloc_control_cell();
    let step_flag = out.alloc_control_cell();
    let ctx = LoopContext {
        guard,
        active,
        recheck,
        continue_target: Some(step_flag),
    };

    emit_loop_cond(cond, guard, out, &[guard])?;

    out.goto(guard);
    out.code.push('[');
    out.clear(active);
    out.add_const(active, 1);
    out.clear(recheck);
    out.add_const(recheck, 1);
    out.clear(step_flag);

    emit_stmts(body, out, Some(ctx))?;

    out.copy_add(active, step_flag, T0);
    out.goto(step_flag);
    out.code.push('[');
    out.clear(step_flag);
    if let Some(step) = step {
        emit_stmt(step, out, None)?;
    }
    out.goto(step_flag);
    out.code.push(']');

    out.goto(recheck);
    out.code.push('[');
    out.clear(recheck);
    emit_loop_cond(cond, guard, out, &[guard, active, recheck, step_flag])?;
    out.goto(recheck);
    out.code.push(']');
    out.goto(guard);
    out.code.push(']');

    out.free_control_cells(4);
    Ok(())
}

fn emit_loop_cond(
    cond: Option<&Expr>,
    guard: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<(), String> {
    if let Some(cond) = cond {
        emit_expr_to(cond, guard, out, reserved)?;
        out.boolify(guard, T0);
    } else {
        out.clear(guard);
        out.add_const(guard, 1);
    }
    Ok(())
}

fn emit_expr_to(
    expr: &Expr,
    dst: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<(), String> {
    if let Some(value) = const_eval(expr) {
        out.clear(dst);
        out.set_const(dst, value);
        return Ok(());
    }

    match expr {
        Expr::Byte(value) => {
            out.clear(dst);
            out.set_const(dst, *value);
        }
        Expr::Var(name) => {
            let src = *name;
            if src != dst {
                let tmp = alloc_temp(reserved)?;
                out.clear(dst);
                out.copy_add(src, dst, tmp);
            }
        }
        Expr::ArrayGet { base, len, index } => {
            emit_array_get(*base, *len, index, dst, out, reserved)?;
        }
        Expr::Call { .. } => {
            return Err(
                "internal error: function call reached direct expression codegen".to_string(),
            );
        }
        Expr::Unary { op, expr } => {
            emit_expr_to(expr, dst, out, reserved)?;
            match op {
                UnOp::Not => {
                    let tmp = alloc_temp(reserved)?;
                    out.logical_not(dst, tmp);
                }
                UnOp::BitNot => {
                    let tmp = alloc_temp(reserved)?;
                    out.rsub_const(dst, 255, tmp);
                }
            }
        }
        Expr::Binary { left, op, right } => {
            if emit_const_binary_to(left, *op, right, dst, out, reserved)? {
                return Ok(());
            }

            emit_expr_to(left, dst, out, reserved)?;
            let mut rhs_reserved = reserved.to_vec();
            rhs_reserved.push(dst);
            let rhs_tmp = alloc_temp(&rhs_reserved)?;
            rhs_reserved.push(rhs_tmp);
            emit_expr_to(right, rhs_tmp, out, &rhs_reserved)?;
            match op {
                BinOp::Or => {
                    let tmp = alloc_temp(&rhs_reserved)?;
                    out.boolify(dst, tmp);
                    out.boolify(rhs_tmp, tmp);
                    out.move_add(rhs_tmp, dst);
                    out.boolify(dst, tmp);
                }
                BinOp::And => {
                    let tmp = alloc_temp(&rhs_reserved)?;
                    out.boolify(dst, tmp);
                    out.boolify(rhs_tmp, tmp);
                    out.bool_and(dst, rhs_tmp, tmp);
                }
                BinOp::BitOr => out.bitwise(dst, rhs_tmp, BitOp::Or),
                BinOp::BitXor => out.bitwise(dst, rhs_tmp, BitOp::Xor),
                BinOp::BitAnd => out.bitwise(dst, rhs_tmp, BitOp::And),
                BinOp::Shl => out.shift(dst, rhs_tmp, ShiftOp::Left),
                BinOp::Shr => out.shift(dst, rhs_tmp, ShiftOp::Right),
                BinOp::Add => {
                    out.move_add(rhs_tmp, dst);
                }
                BinOp::Sub => {
                    out.move_sub(rhs_tmp, dst);
                }
                BinOp::Mul => {
                    let tmp = alloc_temp(&rhs_reserved)?;
                    let mut mul_reserved = rhs_reserved.clone();
                    mul_reserved.push(tmp);
                    let tmp1 = alloc_temp(&mul_reserved)?;
                    out.mul(dst, rhs_tmp, tmp, tmp1);
                }
                BinOp::Div => out.divmod(dst, rhs_tmp, DivModResult::Quotient),
                BinOp::Mod => out.divmod(dst, rhs_tmp, DivModResult::Remainder),
                BinOp::Eq => {
                    out.eq(dst, rhs_tmp);
                }
                BinOp::Ne => {
                    let tmp = alloc_temp(&rhs_reserved)?;
                    out.eq(dst, rhs_tmp);
                    out.logical_not(dst, tmp);
                }
                BinOp::Lt => out.ordered_compare(dst, rhs_tmp, OrderedOp::Lt),
                BinOp::Le => out.ordered_compare(dst, rhs_tmp, OrderedOp::Le),
                BinOp::Gt => out.ordered_compare(dst, rhs_tmp, OrderedOp::Gt),
                BinOp::Ge => out.ordered_compare(dst, rhs_tmp, OrderedOp::Ge),
            }
        }
    }
    Ok(())
}

fn emit_array_get(
    base: usize,
    len: usize,
    index: &Expr,
    dst: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<(), String> {
    if let Some(index) = const_eval(index) {
        let cell = array_elem(base, len, usize::from(index))?;
        if cell != dst {
            let tmp = alloc_temp(reserved)?;
            out.clear(dst);
            out.copy_add(cell, dst, tmp);
        }
        return Ok(());
    }

    emit_array_index(index, base + 1, out, reserved)?;
    out.tritonio_array_get(base);
    if dst != base + 3 {
        out.clear(dst);
        out.move_add(base + 3, dst);
    }
    Ok(())
}

fn emit_array_set(
    base: usize,
    len: usize,
    index: &Expr,
    expr: &Expr,
    out: &mut BfOut,
) -> Result<(), String> {
    if let Some(index) = const_eval(index) {
        let cell = array_elem(base, len, usize::from(index))?;
        emit_expr_to(expr, cell, out, &[cell])?;
        return Ok(());
    }

    emit_expr_to(expr, T0, out, &[T0, base, base + 1, base + 2, base + 3])?;
    out.clear(base + 3);
    out.move_add(T0, base + 3);
    emit_array_index(index, base + 1, out, &[base, base + 1, base + 2, base + 3])?;
    out.tritonio_array_set(base);
    Ok(())
}

fn emit_array_read(base: usize, len: usize, index: &Expr, out: &mut BfOut) -> Result<(), String> {
    if let Some(index) = const_eval(index) {
        let cell = array_elem(base, len, usize::from(index))?;
        out.read(cell);
        return Ok(());
    }

    emit_array_index(index, base + 1, out, &[base, base + 1, base + 2, base + 3])?;
    out.read(base + 3);
    out.tritonio_array_set(base);
    Ok(())
}

fn emit_array_index(
    index: &Expr,
    index_cell: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<(), String> {
    emit_expr_to(index, index_cell, out, reserved)?;
    out.clear(index_cell + 1);
    out.copy_add(index_cell, index_cell + 1, index_cell - 1);
    Ok(())
}

fn array_elem(base: usize, len: usize, index: usize) -> Result<usize, String> {
    if index < len {
        Ok(base + 4 + index)
    } else {
        Err(format!(
            "internal error: array index {index} out of bounds for length {len}"
        ))
    }
}

fn emit_const_binary_to(
    left: &Expr,
    op: BinOp,
    right: &Expr,
    dst: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<bool, String> {
    if let Some(value) = const_eval(right) {
        match op {
            BinOp::Add => {
                emit_expr_to(left, dst, out, reserved)?;
                out.add_const(dst, value);
                return Ok(true);
            }
            BinOp::Sub => {
                emit_expr_to(left, dst, out, reserved)?;
                out.sub_const(dst, value);
                return Ok(true);
            }
            BinOp::Mul => {
                emit_expr_to(left, dst, out, reserved)?;
                out.mul_const(dst, value, alloc_temp(reserved)?);
                return Ok(true);
            }
            BinOp::Div => {
                emit_expr_to(left, dst, out, reserved)?;
                out.divmod_const(dst, value, DivModResult::Quotient);
                return Ok(true);
            }
            BinOp::Mod => {
                emit_expr_to(left, dst, out, reserved)?;
                out.divmod_const(dst, value, DivModResult::Remainder);
                return Ok(true);
            }
            _ => {}
        }
    }

    if let Some(value) = const_eval(left) {
        match op {
            BinOp::Add => {
                emit_expr_to(right, dst, out, reserved)?;
                out.add_const(dst, value);
                return Ok(true);
            }
            BinOp::Mul => {
                emit_expr_to(right, dst, out, reserved)?;
                out.mul_const(dst, value, alloc_temp(reserved)?);
                return Ok(true);
            }
            BinOp::Sub => {
                emit_expr_to(right, dst, out, reserved)?;
                out.rsub_const(dst, value, alloc_temp(reserved)?);
                return Ok(true);
            }
            _ => {}
        }
    }

    Ok(false)
}

fn const_eval(expr: &Expr) -> Option<u8> {
    match expr {
        Expr::Byte(value) => Some(*value),
        Expr::Var(_) => None,
        Expr::ArrayGet { .. } => None,
        Expr::Call { .. } => None,
        Expr::Unary { op, expr } => {
            let value = const_eval(expr)?;
            match op {
                UnOp::Not => Some(u8::from(value == 0)),
                UnOp::BitNot => Some(!value),
            }
        }
        Expr::Binary { left, op, right } => {
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

fn alloc_temp(reserved: &[usize]) -> Result<usize, String> {
    (0..GENERAL_TEMP_COUNT)
        .find(|cell| !reserved.contains(cell))
        .ok_or_else(|| "internal error: out of temporary cells".to_string())
}

fn optimize_bf(code: &str) -> String {
    let mut optimized = code.to_string();
    while optimized.contains("[-][-]") {
        optimized = optimized.replace("[-][-]", "[-]");
    }
    optimized
}

#[derive(Debug, Clone, Copy)]
enum OrderedOp {
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy)]
enum DivModResult {
    Quotient,
    Remainder,
}

#[derive(Debug, Clone, Copy)]
enum BitOp {
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy)]
enum ShiftOp {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
struct ConstPlan {
    count: u8,
    factor: u8,
    remainder: Remainder,
}

#[derive(Debug, Clone, Copy)]
enum Remainder {
    Add(u8),
    Sub(u8),
}

impl ConstPlan {
    fn best(value: u8, distance: usize) -> Option<Self> {
        let direct_len = direct_const_len(value);
        let mut best = None;
        let mut best_len = direct_len;

        for count in 2u16..=16 {
            for factor in 2u16..=16 {
                let product = count * factor;
                let delta = value as i16 - product as i16;
                let abs_delta = delta.unsigned_abs();
                if abs_delta > 16 {
                    continue;
                }

                let remainder = if delta >= 0 {
                    Remainder::Add(delta as u8)
                } else {
                    Remainder::Sub(abs_delta as u8)
                };
                let plan = Self {
                    count: count as u8,
                    factor: factor as u8,
                    remainder,
                };
                let len = plan.len(distance);
                if len < best_len {
                    best = Some(plan);
                    best_len = len;
                }
            }
        }

        best
    }

    fn len(self, distance: usize) -> usize {
        let rem = match self.remainder {
            Remainder::Add(value) | Remainder::Sub(value) => value as usize,
        };

        distance
            + 3
            + distance
            + self.count as usize
            + 1
            + distance
            + self.factor as usize
            + distance
            + 2
            + distance
            + rem
            + distance
            + 1
            + distance
            + 1
            + distance
            + 1
            + 2
            + distance
            + 1
            + distance
            + 1
            + distance
            + 2
    }
}

fn direct_const_len(value: u8) -> usize {
    usize::from(value.min(0u8.wrapping_sub(value)))
}

struct BfOut {
    code: String,
    ptr: usize,
    scratch_base: usize,
    control_depth: usize,
}

impl BfOut {
    fn new(scratch_base: usize) -> Self {
        Self::with_control_depth(scratch_base, 0)
    }

    fn with_control_depth(scratch_base: usize, control_depth: usize) -> Self {
        Self {
            code: String::new(),
            ptr: 0,
            scratch_base,
            control_depth,
        }
    }

    fn w(&self, offset: usize) -> usize {
        debug_assert!(offset < WORK_CELL_COUNT);
        self.scratch_base + offset
    }

    fn const_tmp(&self, cell: usize) -> usize {
        let high = self.w(WORK_CELL_COUNT - 1);
        if cell > high { high } else { self.w(0) }
    }

    fn control_base(&self) -> usize {
        self.scratch_base + WORK_CELL_COUNT
    }

    fn alloc_control_cell(&mut self) -> usize {
        let cell = self.control_base() + self.control_depth;
        self.control_depth += 1;
        self.clear(cell);
        cell
    }

    fn free_control_cells(&mut self, count: usize) {
        debug_assert!(self.control_depth >= count);
        self.control_depth -= count;
    }

    fn goto(&mut self, cell: usize) {
        if cell > self.ptr {
            self.code.push_str(&">".repeat(cell - self.ptr));
        } else {
            self.code.push_str(&"<".repeat(self.ptr - cell));
        }
        self.ptr = cell;
    }

    fn clear(&mut self, cell: usize) {
        self.goto(cell);
        self.code.push_str("[-]");
    }

    fn add_const(&mut self, cell: usize, value: u8) {
        self.goto(cell);
        self.code.push_str(&"+".repeat(value as usize));
    }

    fn sub_const(&mut self, cell: usize, value: u8) {
        self.goto(cell);
        self.code.push_str(&"-".repeat(value as usize));
    }

    fn set_const(&mut self, cell: usize, value: u8) {
        if value == 0 {
            return;
        }
        let tmp = self.const_tmp(cell);
        let distance = cell.abs_diff(tmp);
        if let Some(plan) = ConstPlan::best(value, distance) {
            self.clear(tmp);
            self.build_const(cell, tmp, plan);
        } else if value <= 128 {
            self.add_const(cell, value);
        } else {
            self.sub_const(cell, 0u8.wrapping_sub(value));
        }
    }

    fn build_const(&mut self, cell: usize, tmp: usize, plan: ConstPlan) {
        self.goto(cell);
        self.code.push_str(&"+".repeat(plan.count as usize));
        self.code.push('[');
        self.goto(tmp);
        self.code.push_str(&"+".repeat(plan.factor as usize));
        self.goto(cell);
        self.code.push_str("-]");
        self.goto(tmp);
        match plan.remainder {
            Remainder::Add(value) => self.code.push_str(&"+".repeat(value as usize)),
            Remainder::Sub(value) => self.code.push_str(&"-".repeat(value as usize)),
        }
        self.goto(cell);
        self.code.push('[');
        self.goto(tmp);
        self.code.push('+');
        self.goto(cell);
        self.code.push_str("-]");
        self.goto(tmp);
        self.code.push('[');
        self.goto(cell);
        self.code.push('+');
        self.goto(tmp);
        self.code.push_str("-]");
    }

    fn copy_add(&mut self, src: usize, dst: usize, tmp: usize) {
        self.clear(tmp);
        self.goto(src);
        self.code.push('[');
        self.goto(dst);
        self.code.push('+');
        self.goto(tmp);
        self.code.push('+');
        self.goto(src);
        self.code.push_str("-]");
        self.goto(tmp);
        self.code.push('[');
        self.goto(src);
        self.code.push('+');
        self.goto(tmp);
        self.code.push_str("-]");
    }

    fn move_add(&mut self, src: usize, dst: usize) {
        self.goto(src);
        self.code.push('[');
        self.goto(dst);
        self.code.push('+');
        self.goto(src);
        self.code.push_str("-]");
    }

    fn move_sub(&mut self, src: usize, dst: usize) {
        self.goto(src);
        self.code.push('[');
        self.goto(dst);
        self.code.push('-');
        self.goto(src);
        self.code.push_str("-]");
    }

    fn bool_and(&mut self, left: usize, right: usize, tmp: usize) {
        self.clear(tmp);
        self.goto(left);
        self.code.push('[');
        self.code.push('-');
        self.goto(right);
        self.code.push('[');
        self.goto(tmp);
        self.code.push('+');
        self.goto(right);
        self.code.push_str("-]");
        self.goto(left);
        self.code.push(']');
        self.goto(right);
        self.code.push_str("[-]");
        self.move_add(tmp, left);
    }

    fn logical_not(&mut self, cell: usize, tmp: usize) {
        self.clear(tmp);
        self.goto(cell);
        self.code.push('[');
        self.goto(tmp);
        self.code.push('+');
        self.clear(cell);
        self.code.push(']');
        self.code.push('+');
        self.goto(tmp);
        self.code.push('[');
        self.goto(cell);
        self.code.push('-');
        self.goto(tmp);
        self.code.push_str("-]");
    }

    fn boolify(&mut self, cell: usize, tmp: usize) {
        self.clear(tmp);
        self.goto(cell);
        self.code.push('[');
        self.goto(tmp);
        self.code.push('+');
        self.clear(cell);
        self.code.push(']');
        self.goto(tmp);
        self.code.push('[');
        self.goto(cell);
        self.code.push('+');
        self.goto(tmp);
        self.code.push_str("-]");
    }

    fn eq(&mut self, left: usize, right: usize) {
        self.goto(left);
        self.code.push('[');
        self.goto(right);
        self.code.push('-');
        self.goto(left);
        self.code.push_str("-]+");
        self.goto(right);
        self.code.push('[');
        self.goto(left);
        self.code.push('-');
        self.clear(right);
        self.code.push(']');
    }

    fn mul(&mut self, left: usize, right: usize, tmp0: usize, tmp1: usize) {
        self.clear(tmp0);
        self.clear(tmp1);
        self.goto(left);
        self.code.push('[');
        self.goto(tmp1);
        self.code.push('+');
        self.goto(left);
        self.code.push_str("-]");
        self.goto(tmp1);
        self.code.push('[');
        self.goto(right);
        self.code.push('[');
        self.goto(left);
        self.code.push('+');
        self.goto(tmp0);
        self.code.push('+');
        self.goto(right);
        self.code.push_str("-]");
        self.goto(tmp0);
        self.code.push('[');
        self.goto(right);
        self.code.push('+');
        self.goto(tmp0);
        self.code.push_str("-]");
        self.goto(tmp1);
        self.code.push_str("-]");
    }

    fn mul_const(&mut self, cell: usize, value: u8, tmp: usize) {
        match value {
            0 => self.clear(cell),
            1 => {}
            value => {
                self.clear(tmp);
                self.goto(cell);
                self.code.push('[');
                self.goto(tmp);
                self.code.push_str(&"+".repeat(value as usize));
                self.goto(cell);
                self.code.push_str("-]");
                self.move_add(tmp, cell);
            }
        }
    }

    fn rsub_const(&mut self, cell: usize, value: u8, tmp: usize) {
        self.clear(tmp);
        self.add_const(tmp, value);
        self.goto(cell);
        self.code.push('[');
        self.goto(tmp);
        self.code.push('-');
        self.goto(cell);
        self.code.push_str("-]");
        self.move_add(tmp, cell);
    }

    fn bitwise(&mut self, left: usize, right: usize, op: BitOp) {
        let a = self.alloc_control_cell();
        let b = self.alloc_control_cell();
        let result = self.alloc_control_cell();
        let place = self.alloc_control_cell();
        let abit = self.alloc_control_cell();
        let bbit = self.alloc_control_cell();
        let outbit = self.alloc_control_cell();
        let tmp = self.alloc_control_cell();

        self.move_add(left, a);
        self.move_add(right, b);
        self.add_const(place, 1);

        for _ in 0..8 {
            self.clear(abit);
            self.copy_add(a, abit, tmp);
            self.divmod_const(abit, 2, DivModResult::Remainder);
            self.divmod_const(a, 2, DivModResult::Quotient);

            self.clear(bbit);
            self.copy_add(b, bbit, tmp);
            self.divmod_const(bbit, 2, DivModResult::Remainder);
            self.divmod_const(b, 2, DivModResult::Quotient);

            self.clear(outbit);
            match op {
                BitOp::And => {
                    self.move_add(abit, outbit);
                    self.bool_and(outbit, bbit, tmp);
                }
                BitOp::Or => {
                    self.move_add(abit, outbit);
                    self.move_add(bbit, outbit);
                    self.boolify(outbit, tmp);
                }
                BitOp::Xor => {
                    self.move_add(abit, outbit);
                    self.eq(outbit, bbit);
                    self.logical_not(outbit, tmp);
                }
            }

            self.goto(outbit);
            self.code.push('[');
            self.copy_add(place, result, tmp);
            self.clear(outbit);
            self.goto(outbit);
            self.code.push(']');

            self.mul_const(place, 2, tmp);
        }

        self.clear(left);
        self.move_add(result, left);
        self.free_control_cells(8);
    }

    fn shift(&mut self, left: usize, right: usize, op: ShiftOp) {
        let count = self.alloc_control_cell();
        let tmp = self.alloc_control_cell();
        self.move_add(right, count);

        self.goto(count);
        self.code.push('[');
        self.goto(count);
        self.code.push('-');
        match op {
            ShiftOp::Left => self.mul_const(left, 2, tmp),
            ShiftOp::Right => self.divmod_const(left, 2, DivModResult::Quotient),
        }
        self.goto(count);
        self.code.push(']');

        self.free_control_cells(2);
    }

    fn ordered_compare(&mut self, left: usize, right: usize, op: OrderedOp) {
        match op {
            OrderedOp::Lt => self.less_cells(left, right, left),
            OrderedOp::Le => {
                self.less_cells(right, left, left);
                self.logical_not(left, self.w(7));
            }
            OrderedOp::Gt => self.less_cells(right, left, left),
            OrderedOp::Ge => {
                self.less_cells(left, right, left);
                self.logical_not(left, self.w(7));
            }
        }
    }

    fn less_cells(&mut self, left: usize, right: usize, out: usize) {
        self.clear(self.w(8));
        self.move_add(left, self.w(8));
        self.clear(self.w(9));
        self.move_add(right, self.w(9));
        self.clear(out);

        self.goto(self.w(9));
        self.code.push('[');

        self.clear(self.w(10));
        self.copy_add(self.w(8), self.w(10), self.w(7));
        self.logical_not(self.w(10), self.w(7));
        self.goto(self.w(10));
        self.code.push('[');
        self.clear(out);
        self.add_const(out, 1);
        self.clear(self.w(9));
        self.clear(self.w(10));
        self.goto(self.w(10));
        self.code.push(']');

        self.clear(self.w(11));
        self.copy_add(self.w(9), self.w(11), self.w(7));
        self.boolify(self.w(11), self.w(7));
        self.goto(self.w(11));
        self.code.push('[');
        self.goto(self.w(8));
        self.code.push('-');
        self.goto(self.w(9));
        self.code.push('-');
        self.clear(self.w(11));
        self.goto(self.w(11));
        self.code.push(']');

        self.goto(self.w(9));
        self.code.push(']');
    }

    fn divmod(&mut self, left: usize, right: usize, result: DivModResult) {
        self.clear(self.w(1));
        self.copy_add(left, self.w(1), self.w(5));
        self.clear(self.w(2));
        self.copy_add(right, self.w(2), self.w(5));
        self.clear(self.w(7));
        self.copy_add(right, self.w(7), self.w(5));
        self.boolify(self.w(7), self.w(5));

        self.clear(self.w(6));
        self.copy_add(right, self.w(6), self.w(5));
        self.sub_const(self.w(6), 1);
        self.logical_not(self.w(6), self.w(5));

        if matches!(result, DivModResult::Quotient) {
            self.clear(left);
        }

        self.goto(self.w(6));
        self.code.push('[');
        match result {
            DivModResult::Quotient => self.copy_add(self.w(1), left, self.w(5)),
            DivModResult::Remainder => self.clear(left),
        }
        self.clear(self.w(7));
        self.clear(self.w(6));
        self.goto(self.w(6));
        self.code.push(']');

        self.clear(self.w(3));
        self.clear(self.w(4));
        self.clear(self.w(5));
        self.clear(self.w(6));

        self.goto(self.w(7));
        self.code.push('[');

        self.goto(self.w(1));
        self.code.push_str("[->-[>+>>]>[+[-<+>]>+>>]<<<<<]");

        match result {
            DivModResult::Quotient => self.move_add(self.w(4), left),
            DivModResult::Remainder => {
                self.clear(left);
                self.move_add(self.w(3), left);
            }
        }

        self.clear(self.w(7));
        self.goto(self.w(7));
        self.code.push(']');
    }

    fn divmod_const(&mut self, cell: usize, divisor: u8, result: DivModResult) {
        match (divisor, result) {
            (0, DivModResult::Quotient) => self.clear(cell),
            (0, DivModResult::Remainder) => {}
            (1, DivModResult::Quotient) => {}
            (1, DivModResult::Remainder) => self.clear(cell),
            (divisor, result) => self.divmod_const_nonzero(cell, divisor, result),
        }
    }

    fn divmod_const_nonzero(&mut self, cell: usize, divisor: u8, result: DivModResult) {
        self.clear(self.w(0));
        self.clear(self.w(1));
        self.clear(self.w(2));
        self.add_const(self.w(2), divisor);

        self.goto(cell);
        self.code.push('[');
        self.goto(cell);
        self.code.push('-');
        self.goto(self.w(1));
        self.code.push('+');
        self.goto(self.w(2));
        self.code.push('-');

        self.clear(self.w(3));
        self.copy_add(self.w(2), self.w(3), self.w(5));
        self.logical_not(self.w(3), self.w(5));
        self.goto(self.w(3));
        self.code.push('[');
        self.goto(self.w(0));
        self.code.push('+');
        self.clear(self.w(1));
        self.clear(self.w(2));
        self.add_const(self.w(2), divisor);
        self.clear(self.w(3));
        self.goto(self.w(3));
        self.code.push(']');

        self.goto(cell);
        self.code.push(']');

        match result {
            DivModResult::Quotient => {
                self.clear(cell);
                self.move_add(self.w(0), cell);
            }
            DivModResult::Remainder => {
                self.clear(cell);
                self.move_add(self.w(1), cell);
            }
        }
    }

    fn put(&mut self, cell: usize) {
        self.goto(cell);
        self.code.push('.');
    }

    fn put_byte_const(&mut self, value: u8) {
        self.clear(T0);
        self.set_const(T0, value);
        self.put(T0);
    }

    fn put_bytes(&mut self, bytes: &[u8]) {
        self.clear(T0);
        let mut current = 0u8;
        for &byte in bytes {
            let up = byte.wrapping_sub(current);
            let down = current.wrapping_sub(byte);
            if up <= down {
                self.add_const(T0, up);
            } else {
                self.sub_const(T0, down);
            }
            self.put(T0);
            current = byte;
        }
    }

    fn print_byte_decimal(&mut self, cell: usize) {
        let hundreds = self.alloc_control_cell();
        let rem = self.alloc_control_cell();
        let tens = self.alloc_control_cell();
        let ones = self.alloc_control_cell();
        let flag = self.alloc_control_cell();
        let tmp = self.alloc_control_cell();
        let printed_hundreds = self.alloc_control_cell();

        self.copy_add(cell, hundreds, tmp);
        self.divmod_const(hundreds, 100, DivModResult::Quotient);

        self.copy_add(cell, rem, tmp);
        self.divmod_const(rem, 100, DivModResult::Remainder);

        self.copy_add(rem, tens, tmp);
        self.divmod_const(tens, 10, DivModResult::Quotient);

        self.copy_add(rem, ones, tmp);
        self.divmod_const(ones, 10, DivModResult::Remainder);

        self.clear(flag);
        self.copy_add(hundreds, flag, tmp);
        self.boolify(flag, tmp);
        self.goto(flag);
        self.code.push('[');
        self.clear(printed_hundreds);
        self.add_const(printed_hundreds, 1);
        self.add_const(hundreds, b'0');
        self.put(hundreds);
        self.clear(flag);
        self.goto(flag);
        self.code.push(']');

        self.clear(flag);
        self.copy_add(printed_hundreds, flag, tmp);
        self.copy_add(tens, flag, tmp);
        self.boolify(flag, tmp);
        self.goto(flag);
        self.code.push('[');
        self.add_const(tens, b'0');
        self.put(tens);
        self.clear(flag);
        self.goto(flag);
        self.code.push(']');

        self.add_const(ones, b'0');
        self.put(ones);

        self.free_control_cells(7);
    }

    fn read(&mut self, cell: usize) {
        self.goto(cell);
        self.code.push(',');
    }

    fn tritonio_array_set(&mut self, base: usize) {
        self.goto(base);
        self.code.push_str(
            ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<",
        );
    }

    fn tritonio_array_get(&mut self, base: usize) {
        self.goto(base);
        self.code
            .push_str(">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<");
    }
}
