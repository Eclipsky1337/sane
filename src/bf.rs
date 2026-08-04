use crate::ast::{BinOp, FormatSpec, ReturnType, UnOp, ValueType};
use crate::ir::{BlockId, Op, Terminator};
use crate::sema::{
    ArrayLayout, MAX_CALL_DEPTH, ResolvedExpr as Expr, ResolvedInputTarget, ResolvedProgram,
    ResolvedStmt as Stmt, ValueLayout, WORK_CELL_COUNT,
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
    compile_pc_inner(program, false)
}

#[doc(hidden)]
pub fn compile_pc_with_profiling(program: &ResolvedProgram) -> Result<String, String> {
    compile_pc_inner(program, true)
}

fn compile_pc_inner(program: &ResolvedProgram, profile_dispatch: bool) -> Result<String, String> {
    let ir = crate::ir::lower(program);
    if ir.blocks.len() > u8::MAX as usize + 1 {
        return Err("pc backend supports at most 256 basic blocks".to_string());
    }

    let wide_return = program
        .functions
        .iter()
        .any(|function| matches!(function.return_type, ReturnType::Int | ReturnType::Fixed));
    let reserved_cells = if wide_return { 10 } else { 7 } + MAX_CALL_DEPTH;
    let mut out = BfOut::with_control_depth(program.symbols.scratch_base(), reserved_cells);
    let call_depth = out.control_base() + if wide_return { 9 } else { 6 };
    let ret_base = call_depth + 1;
    let rt = PcRuntime {
        pc: out.control_base(),
        running: out.control_base() + 1,
        dispatch_pc: out.control_base() + 2,
        matched: out.control_base() + 3,
        expected: out.control_base() + 4,
        rv: out.control_base() + 5,
        call_depth,
        ret_base,
    };

    out.clear(rt.pc);
    out.set_const(rt.pc, block_value(ir.entry)?);
    out.clear(rt.running);
    out.add_const(rt.running, 1);
    out.clear(rt.call_depth);
    for cell in rt.rv..rt.rv + if wide_return { 4 } else { 1 } {
        out.clear(cell);
    }
    for depth in 0..MAX_CALL_DEPTH {
        out.clear(rt.ret_base + depth);
    }

    out.goto(rt.running);
    out.code.push('[');
    if profile_dispatch {
        out.code.push_str(crate::interpreter::DISPATCH_MARKER);
    }
    out.clear(rt.dispatch_pc);
    out.copy_add(rt.pc, rt.dispatch_pc, T0);

    for superblock in ir.superblocks() {
        let head = superblock.blocks[0];
        out.clear(rt.matched);
        out.copy_add(rt.dispatch_pc, rt.matched, T0);
        out.clear(rt.expected);
        out.set_const(rt.expected, block_value(head)?);
        out.eq(rt.matched, rt.expected);

        out.goto(rt.matched);
        out.code.push('[');
        out.clear(rt.matched);
        for (position, block_id) in superblock.blocks.iter().copied().enumerate() {
            let block = &ir.blocks[block_id.0];
            for op in &block.ops {
                emit_op(op, &mut out, rt)?;
            }
            if let Some(next) = superblock.blocks.get(position + 1) {
                debug_assert_eq!(block.terminator, Terminator::Jump(*next));
            } else {
                emit_terminator(&block.terminator, &ir, program, &mut out, rt)?;
            }
        }
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
        Op::Let { layout, init } => emit_basic_stmt(
            &Stmt::Let {
                layout: *layout,
                init: init.clone(),
            },
            out,
        ),
        Op::LetArray { layout, init } => emit_basic_stmt(
            &Stmt::LetArray {
                layout: *layout,
                init: init.clone(),
            },
            out,
        ),
        Op::Assign { layout, expr } => emit_basic_stmt(
            &Stmt::Assign {
                layout: *layout,
                expr: expr.clone(),
            },
            out,
        ),
        Op::ArraySet {
            layout,
            index,
            expr,
        } => emit_basic_stmt(
            &Stmt::ArraySet {
                layout: *layout,
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
        Op::ReadArray { layout, index } => emit_basic_stmt(
            &Stmt::ReadArray {
                layout: *layout,
                index: index.clone(),
            },
            out,
        ),
        Op::StoreReturn(layout) => {
            match layout.value_type {
                ValueType::Byte => {
                    out.clear(layout.base);
                    out.copy_add(rt.rv, layout.base, T0);
                }
                ValueType::Int | ValueType::Fixed => copy_int_layout(
                    ValueLayout {
                        base: rt.rv,
                        value_type: layout.value_type,
                    },
                    *layout,
                    out,
                ),
            }
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
                emit_value_to(arg, *param, out)?;
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
            emit_value_to(
                expr,
                ValueLayout {
                    base: rt.rv,
                    value_type: expr.value_type(),
                },
                out,
            )?;
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
        Stmt::Const => {}
        Stmt::Let { layout, init } => {
            if let Some(expr) = init {
                emit_value_to(expr, *layout, out)?;
            } else {
                for cell in layout.cells() {
                    out.clear(cell);
                }
            }
        }
        Stmt::LetArray { layout, init } => {
            for cell in layout.base..layout.base + layout.storage_len() {
                out.clear(cell);
            }
            for (offset, value) in init.iter().copied().enumerate() {
                let element = layout.element(offset);
                match layout.value_type {
                    ValueType::Byte => out.set_const(element.base, value as u8),
                    ValueType::Int | ValueType::Fixed => {
                        for (byte_offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                            out.set_const(element.base + byte_offset, byte);
                        }
                    }
                }
            }
        }
        Stmt::Assign { layout, expr } => {
            emit_value_to(expr, *layout, out)?;
        }
        Stmt::ArraySet {
            layout,
            index,
            expr,
        } => emit_array_set(*layout, index, expr, out)?,
        Stmt::Put(expr) => {
            emit_expr_to(expr, T0, out, &[T0])?;
            out.put(T0);
        }
        Stmt::Puts(bytes) => out.put_bytes(bytes),
        Stmt::Print(expr) => {
            emit_decimal(expr, out)?;
        }
        Stmt::Println(expr) => {
            emit_decimal(expr, out)?;
            out.put_byte_const(b'\n');
        }
        Stmt::Read(layout) => {
            debug_assert_eq!(layout.value_type, ValueType::Byte);
            out.read(layout.base);
        }
        Stmt::ReadArray { layout, index } => emit_array_read(*layout, index, out)?,
        Stmt::Call { .. } | Stmt::Return(_) => {
            return Err(
                "internal error: function statement reached structured backend".to_string(),
            );
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

fn emit_decimal(expr: &Expr, out: &mut BfOut) -> Result<(), String> {
    match expr.value_type() {
        ValueType::Byte => {
            emit_expr_to(expr, T0, out, &[T0])?;
            out.print_byte_decimal(T0);
        }
        ValueType::Int => {
            let base = out.alloc_control_cell();
            for _ in 1..4 {
                out.alloc_control_cell();
            }
            let layout = ValueLayout {
                base,
                value_type: ValueType::Int,
            };
            emit_int_to(expr, layout, out)?;
            out.print_int_decimal(layout)?;
            out.free_control_cells(4);
        }
        ValueType::Fixed => {
            let base = out.alloc_control_cell();
            for _ in 1..4 {
                out.alloc_control_cell();
            }
            let layout = ValueLayout {
                base,
                value_type: ValueType::Fixed,
            };
            emit_fixed_to(expr, layout, out)?;
            out.print_fixed_decimal(layout)?;
            out.free_control_cells(4);
        }
    }
    Ok(())
}

fn emit_basic_stmt(stmt: &Stmt, out: &mut BfOut) -> Result<(), String> {
    match stmt {
        Stmt::Const
        | Stmt::Let { .. }
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
    if matches!(expr.value_type(), ValueType::Int | ValueType::Fixed) {
        let base = out.alloc_control_cell();
        for _ in 1..4 {
            out.alloc_control_cell();
        }
        let layout = ValueLayout {
            base,
            value_type: expr.value_type(),
        };
        emit_value_to(expr, layout, out)?;
        let tmp = alloc_temp(&[dst])?;
        let value_tmp = alloc_temp(&[dst, tmp])?;
        out.clear(dst);
        for cell in layout.cells() {
            out.clear(tmp);
            out.copy_add(cell, tmp, value_tmp);
            out.boolify(tmp, value_tmp);
            out.move_add(tmp, dst);
            out.boolify(dst, value_tmp);
        }
        out.free_control_cells(4);
        return Ok(());
    }

    if let Expr::Binary { left, op, right } = expr
        && matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
        && (matches!(left.value_type(), ValueType::Int | ValueType::Fixed)
            || matches!(right.value_type(), ValueType::Int | ValueType::Fixed))
    {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            emit_int_equality(left, right, *op, dst, out)?;
        } else {
            emit_int_ordering(left, right, *op, dst, out)?;
        }
        return Ok(());
    }

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
        Expr::Var(layout) => {
            let src = layout.base;
            if src != dst {
                let tmp = alloc_temp(reserved)?;
                out.clear(dst);
                out.copy_add(src, dst, tmp);
            }
        }
        Expr::ArrayGet { layout, index } => {
            emit_array_get_byte(*layout, index, dst, out, reserved)?;
        }
        Expr::Call { .. } => {
            return Err(
                "internal error: function call reached direct expression codegen".to_string(),
            );
        }
        Expr::Input {
            parts,
            specs,
            targets,
        } => emit_input(parts, specs, targets, dst, out)?,
        Expr::Int(_) => unreachable!("int values are handled before byte codegen"),
        Expr::Fixed(_) => unreachable!("fixed values are handled before byte codegen"),
        Expr::Cast { expr, target } => {
            if *target != ValueType::Byte {
                return Err("internal error: int cast reached byte codegen".to_string());
            }
            match expr.as_ref() {
                Expr::Int(value) => {
                    out.clear(dst);
                    out.set_const(dst, *value as u8);
                }
                Expr::Var(layout) if layout.value_type == ValueType::Int => {
                    if layout.base != dst {
                        let tmp = alloc_temp(reserved)?;
                        out.clear(dst);
                        out.copy_add(layout.base, dst, tmp);
                    }
                }
                _ if expr.value_type() == ValueType::Fixed => {
                    let base = out.alloc_control_cell();
                    for _ in 1..4 {
                        out.alloc_control_cell();
                    }
                    let int_layout = ValueLayout {
                        base,
                        value_type: ValueType::Int,
                    };
                    emit_int_to(
                        &Expr::Cast {
                            expr: expr.clone(),
                            target: ValueType::Int,
                        },
                        int_layout,
                        out,
                    )?;
                    let tmp = alloc_temp(&[dst])?;
                    out.clear(dst);
                    out.copy_add(int_layout.base, dst, tmp);
                    out.free_control_cells(4);
                }
                _ => emit_expr_to(expr, dst, out, reserved)?,
            }
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
                UnOp::Neg => {
                    return Err("internal error: int negation reached byte codegen".to_string());
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

#[derive(Debug, Clone, Copy)]
enum PreparedInputTarget {
    Scalar(ValueLayout),
    ArrayConst(ValueLayout),
    ArrayDynamic { layout: ArrayLayout, index: usize },
}

fn emit_input(
    parts: &[Vec<u8>],
    specs: &[FormatSpec],
    targets: &[ResolvedInputTarget],
    dst: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    debug_assert_eq!(parts.len(), specs.len() + 1);
    debug_assert_eq!(specs.len(), targets.len());
    let lookahead = out.alloc_control_cell();
    let has_lookahead = out.alloc_control_cell();
    let active = out.alloc_control_cell();
    let count = out.alloc_control_cell();
    out.clear(lookahead);
    out.clear(has_lookahead);
    out.clear(active);
    out.add_const(active, 1);
    out.clear(count);

    for index in 0..specs.len() {
        emit_input_literal(&parts[index], lookahead, has_lookahead, active, out)?;
        let trailing = index + 1 == specs.len() && parts[index + 1].is_empty();
        match specs[index] {
            FormatSpec::Byte => emit_input_byte_field(
                &targets[index],
                lookahead,
                has_lookahead,
                active,
                count,
                out,
            )?,
            FormatSpec::Decimal => emit_input_decimal_field(
                &targets[index],
                trailing,
                lookahead,
                has_lookahead,
                active,
                count,
                out,
            )?,
        }
    }
    emit_input_literal(
        parts.last().expect("input format has a trailing part"),
        lookahead,
        has_lookahead,
        active,
        out,
    )?;

    let tmp = alloc_temp(&[dst])?;
    out.clear(dst);
    out.copy_add(count, dst, tmp);
    out.free_control_cells(4);
    Ok(())
}

fn emit_input_literal(
    bytes: &[u8],
    lookahead: usize,
    has_lookahead: usize,
    active: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    for &byte in bytes {
        emit_when(active, out, |out| {
            emit_input_take(lookahead, has_lookahead, out)?;
            let matched = out.alloc_control_cell();
            emit_cell_eq_const(lookahead, byte, matched, out);
            out.bool_and(active, matched, T0);
            out.free_control_cells(1);
            Ok(())
        })?;
    }
    Ok(())
}

fn emit_input_byte_field(
    target: &ResolvedInputTarget,
    lookahead: usize,
    has_lookahead: usize,
    active: usize,
    count: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let prepared = prepare_input_target(target, active, out)?;
    emit_when(active, out, |out| {
        emit_input_take(lookahead, has_lookahead, out)?;
        let nonzero = out.alloc_control_cell();
        out.clear(nonzero);
        out.copy_add(lookahead, nonzero, T0);
        out.boolify(nonzero, T0);
        out.bool_and(active, nonzero, T0);
        out.free_control_cells(1);
        Ok(())
    })?;
    emit_when(active, out, |out| {
        let source = ValueLayout::byte(lookahead);
        emit_store_input_value(prepared, source, out)?;
        out.add_const(count, 1);
        Ok(())
    })?;
    free_prepared_input_target(prepared, out);
    Ok(())
}

fn emit_input_decimal_field(
    target: &ResolvedInputTarget,
    trailing: bool,
    lookahead: usize,
    has_lookahead: usize,
    active: usize,
    count: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let prepared = prepare_input_target(target, active, out)?;
    let base = out.alloc_control_cell();
    for _ in 1..11 {
        out.alloc_control_cell();
    }
    let candidate = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let sign = base + 4;
    let digits = base + 5;
    let parsing = base + 6;
    let digit_flag = base + 7;
    let digit = base + 8;
    let allowed = base + 9;
    let delimiter_ok = base + 10;
    for cell in candidate.cells() {
        out.clear(cell);
    }
    for cell in sign..=delimiter_ok {
        out.clear(cell);
    }

    emit_when(active, out, |out| {
        emit_input_take(lookahead, has_lookahead, out)?;
        if target.value_type() == ValueType::Int {
            let minus = out.alloc_control_cell();
            emit_cell_eq_const(lookahead, b'-', minus, out);
            emit_when(minus, out, |out| {
                out.clear(sign);
                out.add_const(sign, 1);
                emit_input_take(lookahead, has_lookahead, out)
            })?;
            out.free_control_cells(1);
        }

        out.clear(parsing);
        out.add_const(parsing, 1);
        out.goto(parsing);
        out.code.push('[');
        out.clear(parsing);
        emit_cell_is_digit(lookahead, digit_flag, out);
        emit_when(digit_flag, out, |out| {
            out.clear(digit);
            out.copy_add(lookahead, digit, T0);
            out.sub_const(digit, b'0');
            emit_decimal_step_allowed(candidate, digit, sign, target.value_type(), allowed, out)?;
            out.bool_and(active, allowed, T0);
            emit_when(active, out, |out| {
                emit_int_mul(&Expr::Var(candidate), &Expr::Int(10), candidate, out)?;
                emit_int_add_sub(
                    &Expr::Var(candidate),
                    &Expr::Var(ValueLayout::byte(digit)),
                    BinOp::Add,
                    candidate,
                    out,
                )?;
                out.clear(digits);
                out.add_const(digits, 1);
                emit_input_take(lookahead, has_lookahead, out)?;
                out.clear(parsing);
                out.add_const(parsing, 1);
                Ok(())
            })?;
            Ok(())
        })?;
        out.goto(parsing);
        out.code.push(']');
        Ok(())
    })?;

    out.bool_and(active, digits, T0);
    if trailing {
        emit_input_trailing_delimiter_ok(lookahead, delimiter_ok, out);
        out.bool_and(active, delimiter_ok, T0);
    } else {
        emit_when(active, out, |out| {
            out.clear(has_lookahead);
            out.add_const(has_lookahead, 1);
            Ok(())
        })?;
    }

    emit_when(active, out, |out| {
        if target.value_type() == ValueType::Int {
            negate_layout_if(candidate, sign, out)?;
        }
        emit_store_input_value(prepared, candidate, out)?;
        out.add_const(count, 1);
        Ok(())
    })?;

    out.free_control_cells(11);
    free_prepared_input_target(prepared, out);
    Ok(())
}

fn emit_decimal_step_allowed(
    candidate: ValueLayout,
    digit: usize,
    sign: usize,
    target_type: ValueType,
    dst: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let base = out.alloc_control_cell();
    for _ in 1..9 {
        out.alloc_control_cell();
    }
    let threshold = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let less = base + 4;
    let equal = base + 5;
    let digit_limit = base + 6;
    let digit_ok = base + 7;
    let equal_digit = base + 8;
    let (threshold_value, digit_limit_value) = match target_type {
        ValueType::Byte => (25, 6),
        ValueType::Int => (214_748_364, 8),
        ValueType::Fixed => unreachable!("fixed input rejected during semantic analysis"),
    };
    emit_int_to(&Expr::Int(threshold_value), threshold, out)?;
    emit_int_unsigned_less(candidate, threshold, less, out)?;
    emit_int_equality(
        &Expr::Var(candidate),
        &Expr::Var(threshold),
        BinOp::Eq,
        equal,
        out,
    )?;
    out.clear(digit_limit);
    out.add_const(digit_limit, digit_limit_value);
    if target_type == ValueType::Int {
        out.copy_add(sign, digit_limit, T0);
    }
    emit_cells_less_preserve(digit, digit_limit, digit_ok, out);
    out.clear(equal_digit);
    out.copy_add(equal, equal_digit, T0);
    out.clear(out.w(6));
    out.copy_add(digit_ok, out.w(6), T0);
    out.bool_and(equal_digit, out.w(6), T0);
    out.clear(dst);
    out.copy_add(less, dst, T0);
    out.move_add(equal_digit, dst);
    out.boolify(dst, T0);
    out.free_control_cells(9);
    Ok(())
}

fn emit_input_trailing_delimiter_ok(cell: usize, dst: usize, out: &mut BfOut) {
    let flag = out.alloc_control_cell();
    out.clear(dst);
    for value in [0, b'\n', b'\r'] {
        emit_cell_eq_const(cell, value, flag, out);
        out.move_add(flag, dst);
        out.boolify(dst, T0);
    }
    out.free_control_cells(1);
}

fn prepare_input_target(
    target: &ResolvedInputTarget,
    active: usize,
    out: &mut BfOut,
) -> Result<PreparedInputTarget, String> {
    match target {
        ResolvedInputTarget::Scalar(layout) => Ok(PreparedInputTarget::Scalar(*layout)),
        ResolvedInputTarget::Array { layout, index } => {
            if let Some(index) = const_eval(index) {
                Ok(PreparedInputTarget::ArrayConst(array_elem(
                    *layout,
                    usize::from(index),
                )?))
            } else {
                let index_cell = out.alloc_control_cell();
                emit_when(active, out, |out| {
                    emit_expr_to(index, index_cell, out, &[index_cell])
                })?;
                Ok(PreparedInputTarget::ArrayDynamic {
                    layout: *layout,
                    index: index_cell,
                })
            }
        }
    }
}

fn free_prepared_input_target(target: PreparedInputTarget, out: &mut BfOut) {
    if matches!(target, PreparedInputTarget::ArrayDynamic { .. }) {
        out.free_control_cells(1);
    }
}

fn emit_store_input_value(
    target: PreparedInputTarget,
    source: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    match target {
        PreparedInputTarget::Scalar(dst) | PreparedInputTarget::ArrayConst(dst) => {
            copy_input_value(source, dst, out);
        }
        PreparedInputTarget::ArrayDynamic { layout, index } => {
            let flag = out.alloc_control_cell();
            let expected = out.alloc_control_cell();
            for element_index in 0..layout.len {
                out.clear(flag);
                out.copy_add(index, flag, T0);
                out.clear(expected);
                out.set_const(expected, element_index as u8);
                out.eq(flag, expected);
                out.goto(flag);
                out.code.push('[');
                out.clear(flag);
                copy_input_value(source, layout.element(element_index), out);
                out.goto(flag);
                out.code.push(']');
            }
            out.free_control_cells(2);
        }
    }
    Ok(())
}

fn copy_input_value(source: ValueLayout, dst: ValueLayout, out: &mut BfOut) {
    match dst.value_type {
        ValueType::Byte => {
            if source.base != dst.base {
                out.clear(dst.base);
                out.copy_add(source.base, dst.base, T0);
            }
        }
        ValueType::Int => copy_int_layout(source, dst, out),
        ValueType::Fixed => unreachable!("fixed input rejected during semantic analysis"),
    }
}

fn emit_input_take(lookahead: usize, has_lookahead: usize, out: &mut BfOut) -> Result<(), String> {
    let need_read = out.alloc_control_cell();
    out.clear(need_read);
    out.copy_add(has_lookahead, need_read, T0);
    out.logical_not(need_read, T0);
    emit_when(need_read, out, |out| {
        out.read(lookahead);
        Ok(())
    })?;
    out.clear(has_lookahead);
    out.free_control_cells(1);
    Ok(())
}

fn emit_when<F>(flag: usize, out: &mut BfOut, body: F) -> Result<(), String>
where
    F: FnOnce(&mut BfOut) -> Result<(), String>,
{
    let guard = out.alloc_control_cell();
    out.clear(guard);
    out.copy_add(flag, guard, T0);
    out.boolify(guard, T0);
    out.goto(guard);
    out.code.push('[');
    out.clear(guard);
    body(out)?;
    out.goto(guard);
    out.code.push(']');
    out.free_control_cells(1);
    Ok(())
}

fn emit_cell_eq_const(cell: usize, value: u8, dst: usize, out: &mut BfOut) {
    let left = out.alloc_control_cell();
    let right = out.alloc_control_cell();
    out.clear(left);
    out.copy_add(cell, left, T0);
    out.clear(right);
    out.set_const(right, value);
    out.eq(left, right);
    out.clear(dst);
    out.move_add(left, dst);
    out.free_control_cells(2);
}

fn emit_cell_is_digit(cell: usize, dst: usize, out: &mut BfOut) {
    let below_zero = out.alloc_control_cell();
    let below_ten = out.alloc_control_cell();
    emit_cell_less_const(cell, b'0', below_zero, out);
    out.logical_not(below_zero, T0);
    emit_cell_less_const(cell, b':', below_ten, out);
    out.bool_and(below_zero, below_ten, T0);
    out.clear(dst);
    out.move_add(below_zero, dst);
    out.free_control_cells(2);
}

fn emit_cell_less_const(cell: usize, value: u8, dst: usize, out: &mut BfOut) {
    let left = out.alloc_control_cell();
    let right = out.alloc_control_cell();
    out.clear(left);
    out.copy_add(cell, left, T0);
    out.clear(right);
    out.set_const(right, value);
    out.less_cells(left, right, dst);
    out.free_control_cells(2);
}

fn emit_cells_less_preserve(left: usize, right: usize, dst: usize, out: &mut BfOut) {
    let left_copy = out.alloc_control_cell();
    let right_copy = out.alloc_control_cell();
    out.clear(left_copy);
    out.copy_add(left, left_copy, T0);
    out.clear(right_copy);
    out.copy_add(right, right_copy, T0);
    out.less_cells(left_copy, right_copy, dst);
    out.free_control_cells(2);
}

fn emit_array_get_byte(
    layout: ArrayLayout,
    index: &Expr,
    dst: usize,
    out: &mut BfOut,
    reserved: &[usize],
) -> Result<(), String> {
    debug_assert_eq!(layout.value_type, ValueType::Byte);
    let base = layout.base;
    if let Some(index) = const_eval(index) {
        let cell = array_elem(layout, usize::from(index))?.base;
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
    layout: ArrayLayout,
    index: &Expr,
    expr: &Expr,
    out: &mut BfOut,
) -> Result<(), String> {
    if let Some(index) = const_eval(index) {
        emit_value_to(expr, array_elem(layout, usize::from(index))?, out)?;
        return Ok(());
    }

    if matches!(layout.value_type, ValueType::Int | ValueType::Fixed) {
        return emit_int_array_set_dynamic(layout, index, expr, out);
    }

    let base = layout.base;
    emit_expr_to(expr, T0, out, &[T0, base, base + 1, base + 2, base + 3])?;
    out.clear(base + 3);
    out.move_add(T0, base + 3);
    emit_array_index(index, base + 1, out, &[base, base + 1, base + 2, base + 3])?;
    out.tritonio_array_set(base);
    Ok(())
}

fn emit_array_read(layout: ArrayLayout, index: &Expr, out: &mut BfOut) -> Result<(), String> {
    debug_assert_eq!(layout.value_type, ValueType::Byte);
    if let Some(index) = const_eval(index) {
        let element = array_elem(layout, usize::from(index))?;
        out.read(element.base);
        return Ok(());
    }

    let base = layout.base;
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

fn array_elem(layout: ArrayLayout, index: usize) -> Result<ValueLayout, String> {
    if index < layout.len {
        Ok(layout.element(index))
    } else {
        Err(format!(
            "internal error: array index {index} out of bounds for length {}",
            layout.len
        ))
    }
}

fn emit_int_array_get_dynamic(
    layout: ArrayLayout,
    index: &Expr,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let index_cell = out.alloc_control_cell();
    let flag = out.alloc_control_cell();
    let expected = out.alloc_control_cell();
    emit_expr_to(index, index_cell, out, &[index_cell])?;
    for cell in dst.cells() {
        out.clear(cell);
    }
    for element_index in 0..layout.len {
        out.clear(flag);
        out.copy_add(index_cell, flag, T0);
        out.clear(expected);
        out.set_const(expected, element_index as u8);
        out.eq(flag, expected);
        out.goto(flag);
        out.code.push('[');
        out.clear(flag);
        copy_int_layout(layout.element(element_index), dst, out);
        out.goto(flag);
        out.code.push(']');
    }
    out.free_control_cells(3);
    Ok(())
}

fn emit_int_array_set_dynamic(
    layout: ArrayLayout,
    index: &Expr,
    expr: &Expr,
    out: &mut BfOut,
) -> Result<(), String> {
    let value_base = out.alloc_control_cell();
    for _ in 1..4 {
        out.alloc_control_cell();
    }
    let index_cell = out.alloc_control_cell();
    let flag = out.alloc_control_cell();
    let expected = out.alloc_control_cell();
    let value = ValueLayout {
        base: value_base,
        value_type: ValueType::Int,
    };
    emit_int_to(expr, value, out)?;
    emit_expr_to(index, index_cell, out, &[index_cell])?;
    for element_index in 0..layout.len {
        out.clear(flag);
        out.copy_add(index_cell, flag, T0);
        out.clear(expected);
        out.set_const(expected, element_index as u8);
        out.eq(flag, expected);
        out.goto(flag);
        out.code.push('[');
        out.clear(flag);
        copy_int_layout(value, layout.element(element_index), out);
        out.goto(flag);
        out.code.push(']');
    }
    out.free_control_cells(7);
    Ok(())
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
        Expr::Int(_) => None,
        Expr::Fixed(_) => None,
        Expr::Var(_) => None,
        Expr::ArrayGet { .. } => None,
        Expr::Call { .. } => None,
        Expr::Input { .. } => None,
        Expr::Unary { op, expr } => {
            let value = const_eval(expr)?;
            match op {
                UnOp::Not => Some(u8::from(value == 0)),
                UnOp::BitNot => Some(!value),
                UnOp::Neg => None,
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
        Expr::Cast { expr, target } => match target {
            ValueType::Byte => match expr.as_ref() {
                Expr::Int(value) => Some(*value as u8),
                _ => const_eval(expr),
            },
            ValueType::Int => None,
            ValueType::Fixed => None,
        },
    }
}

fn emit_value_to(expr: &Expr, dst: ValueLayout, out: &mut BfOut) -> Result<(), String> {
    match dst.value_type {
        ValueType::Byte => emit_expr_to(expr, dst.base, out, &[dst.base]),
        ValueType::Int => emit_int_to(expr, dst, out),
        ValueType::Fixed => emit_fixed_to(expr, dst, out),
    }
}

fn emit_int_to(expr: &Expr, dst: ValueLayout, out: &mut BfOut) -> Result<(), String> {
    debug_assert_eq!(dst.cell_width(), 4);
    if expr.value_type() == ValueType::Byte {
        emit_expr_to(expr, dst.base, out, &[dst.base])?;
        for cell in dst.base + 1..dst.base + 4 {
            out.clear(cell);
        }
        return Ok(());
    }
    if expr.value_type() == ValueType::Fixed {
        return emit_fixed_raw_to(expr, dst, out);
    }

    match expr {
        Expr::Int(value) => {
            for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                out.clear(dst.base + offset);
                out.set_const(dst.base + offset, byte);
            }
        }
        Expr::Var(src) if src.value_type == ValueType::Int => {
            if *src != dst {
                for offset in 0..4 {
                    out.clear(dst.base + offset);
                    out.copy_add(src.base + offset, dst.base + offset, T0);
                }
            }
        }
        Expr::ArrayGet { layout, index } => {
            if let Some(index) = const_eval(index) {
                copy_int_layout(array_elem(*layout, usize::from(index))?, dst, out);
            } else {
                emit_int_array_get_dynamic(*layout, index, dst, out)?;
            }
        }
        Expr::Cast { expr, target } if *target == ValueType::Int => {
            if expr.value_type() == ValueType::Fixed {
                emit_int_divmod(expr, &Expr::Int(65_536), BinOp::Div, dst, out)?;
            } else {
                emit_int_to(expr, dst, out)?;
            }
        }
        Expr::Binary { left, op, right } if matches!(op, BinOp::Add | BinOp::Sub) => {
            emit_int_add_sub(left, right, *op, dst, out)?;
        }
        Expr::Binary {
            left,
            op: BinOp::Mul,
            right,
        } => {
            emit_int_mul(left, right, dst, out)?;
        }
        Expr::Binary { left, op, right } if matches!(op, BinOp::Div | BinOp::Mod) => {
            emit_int_divmod(left, right, *op, dst, out)?;
        }
        Expr::Unary {
            op: UnOp::Neg,
            expr,
        } => {
            emit_int_add_sub(&Expr::Int(0), expr, BinOp::Sub, dst, out)?;
        }
        Expr::Unary {
            op: UnOp::BitNot,
            expr,
        } => {
            emit_int_to(expr, dst, out)?;
            for cell in dst.cells() {
                out.rsub_const(cell, 255, T0);
            }
        }
        Expr::Binary { left, op, right }
            if matches!(op, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) =>
        {
            emit_int_bitwise(left, right, *op, dst, out)?;
        }
        Expr::Binary { left, op, right } if matches!(op, BinOp::Shl | BinOp::Shr) => {
            emit_int_shift(left, right, *op, dst, out)?;
        }
        _ => return Err("internal error: unsupported int expression reached codegen".to_string()),
    }
    Ok(())
}

fn emit_fixed_to(expr: &Expr, dst: ValueLayout, out: &mut BfOut) -> Result<(), String> {
    debug_assert_eq!(dst.value_type, ValueType::Fixed);
    emit_fixed_raw_to(expr, dst, out)
}

fn emit_fixed_raw_to(expr: &Expr, dst: ValueLayout, out: &mut BfOut) -> Result<(), String> {
    if expr.value_type() == ValueType::Byte {
        for offset in 0..4 {
            out.clear(dst.base + offset);
        }
        emit_expr_to(expr, dst.base + 2, out, &[dst.base + 2])?;
        return Ok(());
    }

    match expr {
        Expr::Fixed(value) => {
            for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
                out.clear(dst.base + offset);
                out.set_const(dst.base + offset, byte);
            }
        }
        Expr::Var(src) if src.value_type == ValueType::Fixed => {
            copy_int_layout(*src, dst, out);
        }
        Expr::ArrayGet { layout, index } if layout.value_type == ValueType::Fixed => {
            if let Some(index) = const_eval(index) {
                copy_int_layout(array_elem(*layout, usize::from(index))?, dst, out);
            } else {
                emit_int_array_get_dynamic(*layout, index, dst, out)?;
            }
        }
        Expr::Cast { expr, target } if *target == ValueType::Fixed => match expr.value_type() {
            ValueType::Byte => {
                for offset in 0..4 {
                    out.clear(dst.base + offset);
                }
                emit_expr_to(expr, dst.base + 2, out, &[dst.base + 2])?;
            }
            ValueType::Int => {
                let base = out.alloc_control_cell();
                for _ in 1..4 {
                    out.alloc_control_cell();
                }
                let source = ValueLayout {
                    base,
                    value_type: ValueType::Int,
                };
                emit_int_to(expr, source, out)?;
                out.clear(dst.base);
                out.clear(dst.base + 1);
                for offset in 0..2 {
                    out.clear(dst.base + offset + 2);
                    out.copy_add(source.base + offset, dst.base + offset + 2, T0);
                }
                out.free_control_cells(4);
            }
            ValueType::Fixed => emit_fixed_raw_to(expr, dst, out)?,
        },
        Expr::Binary { left, op, right } if matches!(op, BinOp::Add | BinOp::Sub) => {
            emit_int_add_sub(left, right, *op, dst, out)?;
        }
        Expr::Binary {
            left,
            op: BinOp::Mul,
            right,
        } => emit_fixed_mul(left, right, dst, out)?,
        Expr::Binary {
            left,
            op: BinOp::Div,
            right,
        } => emit_fixed_div(left, right, dst, out)?,
        Expr::Unary {
            op: UnOp::Neg,
            expr,
        } => emit_int_add_sub(&Expr::Fixed(0), expr, BinOp::Sub, dst, out)?,
        _ => {
            return Err("internal error: unsupported fixed expression reached codegen".to_string());
        }
    }
    Ok(())
}

fn emit_int_add_sub(
    left: &Expr,
    right: &Expr,
    op: BinOp,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let rhs_base = out.alloc_control_cell();
    for _ in 1..4 {
        out.alloc_control_cell();
    }
    let carry = out.alloc_control_cell();
    let next_carry = out.alloc_control_cell();
    let rhs = ValueLayout {
        base: rhs_base,
        value_type: ValueType::Int,
    };
    emit_int_to(right, rhs, out)?;
    emit_int_to(left, dst, out)?;
    out.clear(carry);

    for offset in 0..4 {
        out.clear(next_carry);
        out.goto(carry);
        out.code.push('[');
        out.clear(carry);
        match op {
            BinOp::Add => {
                out.add_const(dst.base + offset, 1);
                mark_zero(dst.base + offset, next_carry, out);
            }
            BinOp::Sub => {
                mark_zero(dst.base + offset, next_carry, out);
                out.sub_const(dst.base + offset, 1);
            }
            _ => unreachable!(),
        }
        out.goto(carry);
        out.code.push(']');

        out.goto(rhs.base + offset);
        out.code.push('[');
        out.sub_const(rhs.base + offset, 1);
        match op {
            BinOp::Add => {
                out.add_const(dst.base + offset, 1);
                mark_zero(dst.base + offset, next_carry, out);
            }
            BinOp::Sub => {
                mark_zero(dst.base + offset, next_carry, out);
                out.sub_const(dst.base + offset, 1);
            }
            _ => unreachable!(),
        }
        out.goto(rhs.base + offset);
        out.code.push(']');

        out.boolify(next_carry, T0);
        out.move_add(next_carry, carry);
    }

    out.free_control_cells(6);
    Ok(())
}

fn mark_zero(cell: usize, flag: usize, out: &mut BfOut) {
    out.clear(out.w(6));
    out.copy_add(cell, out.w(6), T0);
    out.logical_not(out.w(6), T0);
    out.move_add(out.w(6), flag);
}

fn emit_int_equality(
    left: &Expr,
    right: &Expr,
    op: BinOp,
    dst: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let tmp = alloc_temp(&[dst])?;
    let left_base = out.alloc_control_cell();
    for _ in 1..8 {
        out.alloc_control_cell();
    }
    let left_layout = ValueLayout {
        base: left_base,
        value_type: ValueType::Int,
    };
    let right_layout = ValueLayout {
        base: left_base + 4,
        value_type: ValueType::Int,
    };
    emit_int_to(left, left_layout, out)?;
    emit_int_to(right, right_layout, out)?;
    out.clear(dst);
    out.add_const(dst, 1);
    for offset in 0..4 {
        out.clear(out.w(6));
        out.clear(out.w(7));
        out.copy_add(left_layout.base + offset, out.w(6), tmp);
        out.copy_add(right_layout.base + offset, out.w(7), tmp);
        out.eq(out.w(6), out.w(7));
        out.bool_and(dst, out.w(6), tmp);
    }
    if op == BinOp::Ne {
        out.logical_not(dst, tmp);
    }
    out.free_control_cells(8);
    Ok(())
}

fn emit_int_bitwise(
    left: &Expr,
    right: &Expr,
    op: BinOp,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let rhs_base = out.alloc_control_cell();
    for _ in 1..4 {
        out.alloc_control_cell();
    }
    let rhs = ValueLayout {
        base: rhs_base,
        value_type: ValueType::Int,
    };
    emit_int_to(right, rhs, out)?;
    emit_int_to(left, dst, out)?;
    let bit_op = match op {
        BinOp::BitAnd => BitOp::And,
        BinOp::BitOr => BitOp::Or,
        BinOp::BitXor => BitOp::Xor,
        _ => unreachable!(),
    };
    for offset in 0..4 {
        out.bitwise(dst.base + offset, rhs.base + offset, bit_op);
    }
    out.free_control_cells(4);
    Ok(())
}

fn emit_int_ordering(
    left: &Expr,
    right: &Expr,
    op: BinOp,
    dst: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let reverse = matches!(op, BinOp::Gt | BinOp::Le);
    let negate = matches!(op, BinOp::Le | BinOp::Ge);
    let (left, right) = if reverse {
        (right, left)
    } else {
        (left, right)
    };

    let left_base = out.alloc_control_cell();
    for _ in 1..9 {
        out.alloc_control_cell();
    }
    let left_layout = ValueLayout {
        base: left_base,
        value_type: ValueType::Int,
    };
    let right_layout = ValueLayout {
        base: left_base + 4,
        value_type: ValueType::Int,
    };
    let tmp = alloc_temp(&[dst])?;
    let undecided = left_base + 8;
    emit_int_to(left, left_layout, out)?;
    emit_int_to(right, right_layout, out)?;
    out.clear(dst);
    out.clear(undecided);
    out.add_const(undecided, 1);

    for offset in (0..4).rev() {
        out.clear(out.w(4));
        out.clear(out.w(5));
        out.copy_add(left_layout.base + offset, out.w(4), tmp);
        out.copy_add(right_layout.base + offset, out.w(5), tmp);
        if offset == 3 {
            out.add_const(out.w(4), 128);
            out.add_const(out.w(5), 128);
        }
        out.less_cells(out.w(4), out.w(5), out.w(6));
        out.clear(out.w(7));
        out.copy_add(undecided, out.w(7), tmp);
        out.bool_and(out.w(7), out.w(6), tmp);
        out.move_add(out.w(7), dst);
        out.boolify(dst, tmp);

        out.clear(out.w(4));
        out.clear(out.w(5));
        out.copy_add(left_layout.base + offset, out.w(4), tmp);
        out.copy_add(right_layout.base + offset, out.w(5), tmp);
        out.eq(out.w(4), out.w(5));
        out.bool_and(undecided, out.w(4), tmp);
    }

    if negate {
        out.logical_not(dst, tmp);
    }
    out.free_control_cells(9);
    Ok(())
}

fn emit_int_shift(
    value: &Expr,
    count: &Expr,
    op: BinOp,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let count_cell = out.alloc_control_cell();
    let carry = out.alloc_control_cell();
    let next_carry = out.alloc_control_cell();
    emit_expr_to(count, count_cell, out, &[count_cell])?;
    emit_int_to(value, dst, out)?;

    out.goto(count_cell);
    out.code.push('[');
    out.sub_const(count_cell, 1);
    emit_int_shift_once(dst, op, true, carry, next_carry, out);
    out.goto(count_cell);
    out.code.push(']');
    out.free_control_cells(3);
    Ok(())
}

fn emit_int_shift_once(
    dst: ValueLayout,
    op: BinOp,
    arithmetic_right: bool,
    carry: usize,
    next_carry: usize,
    out: &mut BfOut,
) {
    match op {
        BinOp::Shl => {
            out.clear(carry);
            for offset in 0..4 {
                out.clear(next_carry);
                out.clear(out.w(6));
                out.copy_add(dst.base + offset, out.w(6), T0);
                out.divmod_const(out.w(6), 128, DivModResult::Quotient);
                out.move_add(out.w(6), next_carry);
                out.mul_const(dst.base + offset, 2, T0);
                out.move_add(carry, dst.base + offset);
                out.move_add(next_carry, carry);
            }
        }
        BinOp::Shr => {
            out.clear(carry);
            if arithmetic_right {
                out.clear(out.w(6));
                out.copy_add(dst.base + 3, out.w(6), T0);
                out.divmod_const(out.w(6), 128, DivModResult::Quotient);
                out.mul_const(out.w(6), 128, T0);
                out.move_add(out.w(6), carry);
            }
            for offset in (0..4).rev() {
                out.clear(next_carry);
                out.clear(out.w(6));
                out.copy_add(dst.base + offset, out.w(6), T0);
                out.divmod_const(out.w(6), 2, DivModResult::Remainder);
                out.mul_const(out.w(6), 128, T0);
                out.move_add(out.w(6), next_carry);
                out.divmod_const(dst.base + offset, 2, DivModResult::Quotient);
                out.move_add(carry, dst.base + offset);
                out.move_add(next_carry, carry);
            }
        }
        _ => unreachable!(),
    }
}

fn emit_int_mul(
    left: &Expr,
    right: &Expr,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let base = out.alloc_control_cell();
    for _ in 1..12 {
        out.alloc_control_cell();
    }
    let multiplicand = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let multiplier = ValueLayout {
        base: base + 4,
        value_type: ValueType::Int,
    };
    let condition = base + 8;
    let bit = base + 9;
    let carry = base + 10;
    let next_carry = base + 11;
    emit_int_to(right, multiplier, out)?;
    emit_int_to(left, multiplicand, out)?;
    for cell in dst.cells() {
        out.clear(cell);
    }
    int_layout_bool(multiplier, condition, out);

    out.goto(condition);
    out.code.push('[');
    out.clear(condition);
    out.clear(bit);
    out.copy_add(multiplier.base, bit, T0);
    out.divmod_const(bit, 2, DivModResult::Remainder);
    out.goto(bit);
    out.code.push('[');
    out.clear(bit);
    emit_int_add_sub(
        &Expr::Var(dst),
        &Expr::Var(multiplicand),
        BinOp::Add,
        dst,
        out,
    )?;
    out.goto(bit);
    out.code.push(']');
    emit_int_shift_once(multiplicand, BinOp::Shl, false, carry, next_carry, out);
    emit_int_shift_once(multiplier, BinOp::Shr, false, carry, next_carry, out);
    int_layout_bool(multiplier, condition, out);
    out.goto(condition);
    out.code.push(']');
    out.free_control_cells(12);
    Ok(())
}

fn emit_fixed_mul(
    left: &Expr,
    right: &Expr,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let base = out.alloc_control_cell();
    for _ in 1..33 {
        out.alloc_control_cell();
    }
    let left_value = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let right_value = ValueLayout {
        base: base + 4,
        value_type: ValueType::Int,
    };
    let left_high = ValueLayout {
        base: base + 8,
        value_type: ValueType::Int,
    };
    let left_low = ValueLayout {
        base: base + 12,
        value_type: ValueType::Int,
    };
    let right_high = ValueLayout {
        base: base + 16,
        value_type: ValueType::Int,
    };
    let right_low = ValueLayout {
        base: base + 20,
        value_type: ValueType::Int,
    };
    let term = ValueLayout {
        base: base + 24,
        value_type: ValueType::Int,
    };
    let left_sign = base + 28;
    let right_sign = base + 29;
    let result_sign = base + 30;
    let carry = base + 31;
    let next_carry = base + 32;

    emit_fixed_raw_to(left, left_value, out)?;
    emit_fixed_raw_to(right, right_value, out)?;
    int_sign_bit(left_value, left_sign, out);
    int_sign_bit(right_value, right_sign, out);
    out.clear(result_sign);
    out.copy_add(left_sign, result_sign, T0);
    out.clear(out.w(6));
    out.copy_add(right_sign, out.w(6), T0);
    out.bitwise(result_sign, out.w(6), BitOp::Xor);
    negate_layout_if(left_value, left_sign, out)?;
    negate_layout_if(right_value, right_sign, out)?;

    for layout in [left_high, left_low, right_high, right_low] {
        for cell in layout.cells() {
            out.clear(cell);
        }
    }
    for offset in 0..2 {
        out.copy_add(left_value.base + offset, left_low.base + offset, T0);
        out.copy_add(left_value.base + offset + 2, left_high.base + offset, T0);
        out.copy_add(right_value.base + offset, right_low.base + offset, T0);
        out.copy_add(right_value.base + offset + 2, right_high.base + offset, T0);
    }

    emit_int_mul(&Expr::Var(left_high), &Expr::Var(right_value), dst, out)?;
    emit_int_mul(&Expr::Var(left_low), &Expr::Var(right_high), term, out)?;
    emit_int_add_sub(&Expr::Var(dst), &Expr::Var(term), BinOp::Add, dst, out)?;
    emit_int_mul(&Expr::Var(left_low), &Expr::Var(right_low), term, out)?;
    for _ in 0..16 {
        emit_int_shift_once(term, BinOp::Shr, false, carry, next_carry, out);
    }
    emit_int_add_sub(&Expr::Var(dst), &Expr::Var(term), BinOp::Add, dst, out)?;
    negate_layout_if(dst, result_sign, out)?;
    out.free_control_cells(33);
    Ok(())
}

fn int_layout_bool(layout: ValueLayout, dst: usize, out: &mut BfOut) {
    out.clear(dst);
    for cell in layout.cells() {
        out.clear(out.w(6));
        out.copy_add(cell, out.w(6), T0);
        out.boolify(out.w(6), T0);
        out.move_add(out.w(6), dst);
        out.boolify(dst, T0);
    }
}

fn emit_int_divmod(
    left: &Expr,
    right: &Expr,
    op: BinOp,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let base = out.alloc_control_cell();
    for _ in 1..25 {
        out.alloc_control_cell();
    }
    let dividend = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let divisor = ValueLayout {
        base: base + 4,
        value_type: ValueType::Int,
    };
    let quotient = ValueLayout {
        base: base + 8,
        value_type: ValueType::Int,
    };
    let remainder = ValueLayout {
        base: base + 12,
        value_type: ValueType::Int,
    };
    let nonzero = base + 16;
    let left_sign = base + 17;
    let right_sign = base + 18;
    let quotient_sign = base + 19;
    let remainder_sign = base + 20;
    let count = base + 21;
    let carry = base + 22;
    let next_carry = base + 23;
    let compare = base + 24;

    emit_int_to(right, divisor, out)?;
    emit_int_to(left, dividend, out)?;
    copy_int_layout(dividend, remainder, out);
    for cell in quotient.cells() {
        out.clear(cell);
    }
    int_layout_bool(divisor, nonzero, out);
    int_sign_bit(dividend, left_sign, out);
    int_sign_bit(divisor, right_sign, out);
    out.clear(quotient_sign);
    out.copy_add(left_sign, quotient_sign, T0);
    out.clear(out.w(6));
    out.copy_add(right_sign, out.w(6), T0);
    out.bitwise(quotient_sign, out.w(6), BitOp::Xor);
    out.clear(remainder_sign);
    out.copy_add(left_sign, remainder_sign, T0);

    out.goto(nonzero);
    out.code.push('[');
    out.clear(nonzero);
    negate_layout_if(dividend, left_sign, out)?;
    negate_layout_if(divisor, right_sign, out)?;
    for cell in quotient.cells().chain(remainder.cells()) {
        out.clear(cell);
    }
    out.clear(count);
    out.add_const(count, 32);
    out.goto(count);
    out.code.push('[');
    out.sub_const(count, 1);

    emit_int_shift_once(quotient, BinOp::Shl, false, carry, next_carry, out);
    emit_int_shift_once(remainder, BinOp::Shl, false, carry, next_carry, out);
    out.clear(out.w(6));
    out.copy_add(dividend.base + 3, out.w(6), T0);
    out.divmod_const(out.w(6), 128, DivModResult::Quotient);
    out.move_add(out.w(6), remainder.base);
    emit_int_shift_once(dividend, BinOp::Shl, false, carry, next_carry, out);

    emit_int_unsigned_less(remainder, divisor, compare, out)?;
    out.logical_not(compare, T0);
    out.goto(compare);
    out.code.push('[');
    out.clear(compare);
    emit_int_add_sub(
        &Expr::Var(remainder),
        &Expr::Var(divisor),
        BinOp::Sub,
        remainder,
        out,
    )?;
    out.add_const(quotient.base, 1);
    out.goto(compare);
    out.code.push(']');

    out.goto(count);
    out.code.push(']');
    negate_layout_if(quotient, quotient_sign, out)?;
    negate_layout_if(remainder, remainder_sign, out)?;
    out.goto(nonzero);
    out.code.push(']');

    match op {
        BinOp::Div => copy_int_layout(quotient, dst, out),
        BinOp::Mod => copy_int_layout(remainder, dst, out),
        _ => unreachable!(),
    }
    out.free_control_cells(25);
    Ok(())
}

fn emit_fixed_div(
    left: &Expr,
    right: &Expr,
    dst: ValueLayout,
    out: &mut BfOut,
) -> Result<(), String> {
    let base = out.alloc_control_cell();
    for _ in 1..24 {
        out.alloc_control_cell();
    }
    let dividend = ValueLayout {
        base,
        value_type: ValueType::Int,
    };
    let divisor = ValueLayout {
        base: base + 4,
        value_type: ValueType::Int,
    };
    let quotient = ValueLayout {
        base: base + 8,
        value_type: ValueType::Int,
    };
    let remainder = ValueLayout {
        base: base + 12,
        value_type: ValueType::Int,
    };
    let nonzero = base + 16;
    let left_sign = base + 17;
    let right_sign = base + 18;
    let quotient_sign = base + 19;
    let count = base + 20;
    let carry = base + 21;
    let next_carry = base + 22;
    let compare = base + 23;

    emit_fixed_raw_to(right, divisor, out)?;
    emit_fixed_raw_to(left, dividend, out)?;
    for cell in quotient.cells().chain(remainder.cells()) {
        out.clear(cell);
    }
    int_layout_bool(divisor, nonzero, out);
    int_sign_bit(dividend, left_sign, out);
    int_sign_bit(divisor, right_sign, out);
    out.clear(quotient_sign);
    out.copy_add(left_sign, quotient_sign, T0);
    out.clear(out.w(6));
    out.copy_add(right_sign, out.w(6), T0);
    out.bitwise(quotient_sign, out.w(6), BitOp::Xor);

    out.goto(nonzero);
    out.code.push('[');
    out.clear(nonzero);
    negate_layout_if(dividend, left_sign, out)?;
    negate_layout_if(divisor, right_sign, out)?;
    out.clear(count);
    out.add_const(count, 48);
    out.goto(count);
    out.code.push('[');
    out.sub_const(count, 1);

    emit_int_shift_once(quotient, BinOp::Shl, false, carry, next_carry, out);
    emit_int_shift_once(remainder, BinOp::Shl, false, carry, next_carry, out);
    out.clear(out.w(6));
    out.copy_add(dividend.base + 3, out.w(6), T0);
    out.divmod_const(out.w(6), 128, DivModResult::Quotient);
    out.move_add(out.w(6), remainder.base);
    emit_int_shift_once(dividend, BinOp::Shl, false, carry, next_carry, out);

    emit_int_unsigned_less(remainder, divisor, compare, out)?;
    out.logical_not(compare, T0);
    out.goto(compare);
    out.code.push('[');
    out.clear(compare);
    emit_int_add_sub(
        &Expr::Var(remainder),
        &Expr::Var(divisor),
        BinOp::Sub,
        remainder,
        out,
    )?;
    out.add_const(quotient.base, 1);
    out.goto(compare);
    out.code.push(']');

    out.goto(count);
    out.code.push(']');
    negate_layout_if(quotient, quotient_sign, out)?;
    out.goto(nonzero);
    out.code.push(']');

    copy_int_layout(quotient, dst, out);
    out.free_control_cells(24);
    Ok(())
}

fn copy_int_layout(src: ValueLayout, dst: ValueLayout, out: &mut BfOut) {
    if src == dst {
        return;
    }
    for offset in 0..4 {
        out.clear(dst.base + offset);
        out.copy_add(src.base + offset, dst.base + offset, T0);
    }
}

fn int_sign_bit(layout: ValueLayout, dst: usize, out: &mut BfOut) {
    out.clear(dst);
    out.copy_add(layout.base + 3, dst, T0);
    out.divmod_const(dst, 128, DivModResult::Quotient);
}

fn negate_layout_if(layout: ValueLayout, flag: usize, out: &mut BfOut) -> Result<(), String> {
    out.goto(flag);
    out.code.push('[');
    out.clear(flag);
    emit_int_add_sub(&Expr::Int(0), &Expr::Var(layout), BinOp::Sub, layout, out)?;
    out.goto(flag);
    out.code.push(']');
    Ok(())
}

fn emit_int_unsigned_less(
    left: ValueLayout,
    right: ValueLayout,
    dst: usize,
    out: &mut BfOut,
) -> Result<(), String> {
    let tmp = alloc_temp(&[dst])?;
    let undecided = out.alloc_control_cell();
    out.clear(dst);
    out.clear(undecided);
    out.add_const(undecided, 1);
    for offset in (0..4).rev() {
        out.clear(out.w(4));
        out.clear(out.w(5));
        out.copy_add(left.base + offset, out.w(4), tmp);
        out.copy_add(right.base + offset, out.w(5), tmp);
        out.less_cells(out.w(4), out.w(5), out.w(6));
        out.clear(out.w(7));
        out.copy_add(undecided, out.w(7), tmp);
        out.bool_and(out.w(7), out.w(6), tmp);
        out.move_add(out.w(7), dst);
        out.boolify(dst, tmp);

        out.clear(out.w(4));
        out.clear(out.w(5));
        out.copy_add(left.base + offset, out.w(4), tmp);
        out.copy_add(right.base + offset, out.w(5), tmp);
        out.eq(out.w(4), out.w(5));
        out.bool_and(undecided, out.w(4), tmp);
    }
    out.free_control_cells(1);
    Ok(())
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

    fn print_fixed_decimal(&mut self, value: ValueLayout) -> Result<(), String> {
        let base = self.alloc_control_cell();
        for _ in 1..21 {
            self.alloc_control_cell();
        }
        let magnitude = ValueLayout {
            base,
            value_type: ValueType::Int,
        };
        let integer = ValueLayout {
            base: base + 4,
            value_type: ValueType::Int,
        };
        let remainder = ValueLayout {
            base: base + 8,
            value_type: ValueType::Int,
        };
        let product = ValueLayout {
            base: base + 12,
            value_type: ValueType::Int,
        };
        let digit = base + 16;
        let pending_zeros = base + 17;
        let printed_fraction = base + 18;
        let nonzero = base + 19;
        let zero = base + 20;

        copy_int_layout(value, magnitude, self);
        int_sign_bit(magnitude, nonzero, self);
        self.goto(nonzero);
        self.code.push('[');
        self.clear(nonzero);
        self.put_byte_const(b'-');
        emit_int_add_sub(
            &Expr::Int(0),
            &Expr::Var(magnitude),
            BinOp::Sub,
            magnitude,
            self,
        )?;
        self.goto(nonzero);
        self.code.push(']');

        for cell in integer.cells().chain(remainder.cells()) {
            self.clear(cell);
        }
        self.copy_add(magnitude.base + 2, integer.base, T0);
        self.copy_add(magnitude.base + 3, integer.base + 1, T0);
        self.copy_add(magnitude.base, remainder.base, T0);
        self.copy_add(magnitude.base + 1, remainder.base + 1, T0);
        self.print_int_decimal(integer)?;
        self.put_byte_const(b'.');

        self.clear(pending_zeros);
        self.clear(printed_fraction);
        for _ in 0..16 {
            emit_int_mul(&Expr::Var(remainder), &Expr::Int(10), product, self)?;
            self.clear(digit);
            self.copy_add(product.base + 2, digit, T0);
            self.clear(remainder.base);
            self.clear(remainder.base + 1);
            self.copy_add(product.base, remainder.base, T0);
            self.copy_add(product.base + 1, remainder.base + 1, T0);

            self.clear(nonzero);
            self.copy_add(digit, nonzero, T0);
            self.boolify(nonzero, T0);
            self.clear(zero);
            self.add_const(zero, 1);
            self.goto(nonzero);
            self.code.push('[');
            self.clear(nonzero);
            self.clear(zero);
            self.goto(pending_zeros);
            self.code.push('[');
            self.sub_const(pending_zeros, 1);
            self.put_byte_const(b'0');
            self.goto(pending_zeros);
            self.code.push(']');
            self.add_const(digit, b'0');
            self.put(digit);
            self.clear(printed_fraction);
            self.add_const(printed_fraction, 1);
            self.goto(nonzero);
            self.code.push(']');

            self.goto(zero);
            self.code.push('[');
            self.clear(zero);
            self.add_const(pending_zeros, 1);
            self.goto(zero);
            self.code.push(']');
        }

        self.clear(zero);
        self.copy_add(printed_fraction, zero, T0);
        self.logical_not(zero, T0);
        self.goto(zero);
        self.code.push('[');
        self.clear(zero);
        self.put_byte_const(b'0');
        self.goto(zero);
        self.code.push(']');
        self.free_control_cells(21);
        Ok(())
    }

    fn print_int_decimal(&mut self, value: ValueLayout) -> Result<(), String> {
        let place_base = self.alloc_control_cell();
        for _ in 1..4 {
            self.alloc_control_cell();
        }
        let digit = self.alloc_control_cell();
        let less = self.alloc_control_cell();
        let guard = self.alloc_control_cell();
        let printed = self.alloc_control_cell();
        let flag = self.alloc_control_cell();
        let place = ValueLayout {
            base: place_base,
            value_type: ValueType::Int,
        };

        int_sign_bit(value, flag, self);
        self.goto(flag);
        self.code.push('[');
        self.clear(flag);
        self.put_byte_const(b'-');
        emit_int_add_sub(&Expr::Int(0), &Expr::Var(value), BinOp::Sub, value, self)?;
        self.goto(flag);
        self.code.push(']');

        self.clear(printed);
        for power in [
            1_000_000_000u32,
            100_000_000,
            10_000_000,
            1_000_000,
            100_000,
            10_000,
            1_000,
            100,
            10,
        ] {
            for (offset, byte) in power.to_le_bytes().into_iter().enumerate() {
                self.clear(place.base + offset);
                self.set_const(place.base + offset, byte);
            }
            self.clear(digit);
            self.clear(guard);
            self.add_const(guard, 1);
            self.goto(guard);
            self.code.push('[');
            self.clear(guard);
            emit_int_unsigned_less(value, place, less, self)?;
            self.logical_not(less, T0);
            self.goto(less);
            self.code.push('[');
            self.clear(less);
            emit_int_add_sub(
                &Expr::Var(value),
                &Expr::Var(place),
                BinOp::Sub,
                value,
                self,
            )?;
            self.add_const(digit, 1);
            self.add_const(guard, 1);
            self.goto(less);
            self.code.push(']');
            self.goto(guard);
            self.code.push(']');

            self.clear(flag);
            self.copy_add(printed, flag, T0);
            self.copy_add(digit, flag, T0);
            self.boolify(flag, T0);
            self.goto(flag);
            self.code.push('[');
            self.clear(flag);
            self.add_const(digit, b'0');
            self.put(digit);
            self.clear(printed);
            self.add_const(printed, 1);
            self.goto(flag);
            self.code.push(']');
        }

        self.add_const(value.base, b'0');
        self.put(value.base);
        self.free_control_cells(9);
        Ok(())
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
