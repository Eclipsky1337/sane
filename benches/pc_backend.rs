use sane::interpreter::{ExecutionStats, Program as BfProgram};
use sane::{bf, ir, lexer, parser, sema};

const TOY_AES_SOURCE: &str = include_str!("../examples/toy_aes_round.sn");
const COLLATZ_SOURCE: &str = include_str!("../examples/Collatz.sn");

const TOY_AES_OUTPUT: &[u8] = b"toy-aes:ED958E73EFAD3E04336822CC237B065A ok\n";
const COLLATZ_OUTPUT: &[u8] = b"input a number: step 0: 7\n\
step 1: 22\n\
step 2: 11\n\
step 3: 34\n\
step 4: 17\n\
step 5: 52\n\
step 6: 26\n\
step 7: 13\n\
step 8: 40\n\
step 9: 20\n\
step 10: 10\n\
step 11: 5\n\
step 12: 16\n\
step 13: 8\n\
step 14: 4\n\
step 15: 2\n\
step 16: 1\n";

#[derive(Debug)]
struct Measurement {
    name: &'static str,
    ir: ir::IrStats,
    dispatch_groups: usize,
    bf_instructions: usize,
    parsed_ops: usize,
    dispatches: u64,
    executed_ops: u64,
}

fn main() {
    let measurements = [
        measure(
            "toy_aes_round",
            TOY_AES_SOURCE,
            b"ABCDEFGHIJKLMNOP",
            TOY_AES_OUTPUT,
        ),
        measure("Collatz", COLLATZ_SOURCE, b"7\n", COLLATZ_OUTPUT),
    ];

    println!(
        "| Program | Blocks | Dispatch groups | Reachable | IR ops | Empty | Trivial jumps | BF instructions | Parsed ops | Dispatches | Executed ops |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for measurement in measurements {
        println!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            measurement.name,
            measurement.ir.blocks,
            measurement.dispatch_groups,
            measurement.ir.reachable_blocks,
            measurement.ir.ops,
            measurement.ir.empty_blocks,
            measurement.ir.trivial_jumps,
            measurement.bf_instructions,
            measurement.parsed_ops,
            measurement.dispatches,
            measurement.executed_ops,
        );
    }
}

fn measure(name: &'static str, source: &str, input: &[u8], expected_output: &[u8]) -> Measurement {
    let resolved = resolve(source);
    let lowered = ir::lower(&resolved);
    let generated = bf::compile_pc(&resolved).unwrap();
    let parsed = BfProgram::parse(&generated).unwrap();
    let mut output = Vec::new();
    let ExecutionStats { executed_ops, .. } = parsed
        .run_with_stats(input, &mut output)
        .unwrap_or_else(|error| panic!("failed to run {name}: {error}"));
    assert_eq!(output, expected_output, "unexpected output from {name}");

    let profiled = bf::compile_pc_with_profiling(&resolved).unwrap();
    let profiled = BfProgram::parse(&profiled).unwrap();
    let mut profiled_output = Vec::new();
    let ExecutionStats { dispatches, .. } = profiled
        .run_with_stats(input, &mut profiled_output)
        .unwrap_or_else(|error| panic!("failed to profile {name}: {error}"));
    assert_eq!(
        profiled_output, expected_output,
        "profiling changed output from {name}"
    );

    Measurement {
        name,
        dispatch_groups: lowered.superblocks().len(),
        ir: lowered.stats(),
        bf_instructions: generated.bytes().filter(|byte| is_bf(*byte)).count(),
        parsed_ops: parsed.operation_count(),
        dispatches,
        executed_ops,
    }
}

fn resolve(source: &str) -> sema::ResolvedProgram {
    let tokens = lexer::lex(source).unwrap();
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program().unwrap();
    sema::resolve(&program).unwrap()
}

fn is_bf(byte: u8) -> bool {
    matches!(byte, b'>' | b'<' | b'+' | b'-' | b'.' | b',' | b'[' | b']')
}
