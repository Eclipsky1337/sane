use sane::interpreter::{
    ARRAY_GET_MARKER, ARRAY_GET_WIDE_MARKER, ARRAY_SET_MARKER, ARRAY_SET_WIDE_MARKER, Block,
    BlockKind, DISPATCH_MARKER, ExecutionStats, Op, Program, run_source,
};
use std::io::{self, Write};

#[test]
fn bf_interpreter_runs_basic_programs() {
    let mut output = Vec::new();
    run_source("+++++[>+++++++++++++<-]>.", &[][..], &mut output).unwrap();
    assert_eq!(output, b"A");
}

#[test]
fn bf_interpreter_reads_and_writes_bytes() {
    let mut output = Vec::new();
    run_source(",.,.", b"AZ".as_slice(), &mut output).unwrap();
    assert_eq!(output, b"AZ");
}

#[test]
fn bf_interpreter_flushes_after_output() {
    #[derive(Default)]
    struct FlushCounter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl Write for FlushCounter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    let mut output = FlushCounter::default();
    run_source("++..", &[][..], &mut output).unwrap();
    assert_eq!(output.bytes, [2, 2]);
    assert_eq!(output.flushes, 2);
}

#[test]
fn bf_optimizer_combines_simple_ops() {
    let program = Program::parse("+++-->>><").unwrap();
    assert_eq!(program.ops(), &[Op::Add(1), Op::Move(2)]);
}

#[test]
fn bf_optimizer_recognizes_clear_loop() {
    let program = Program::parse("+++[-].").unwrap();
    assert_eq!(program.ops(), &[Op::Add(3), Op::Clear, Op::Output]);
}

#[test]
fn bf_optimizer_recognizes_add_mul_loop() {
    let program = Program::parse("[->++>+++<<]").unwrap();
    assert_eq!(program.ops(), &[Op::AddMul(vec![(1, 2), (2, 3)])]);
}

#[test]
fn bf_optimizer_fuses_contiguous_moves_and_adds() {
    let program = Program::parse("+>++<-.").unwrap();
    assert_eq!(
        program.ops(),
        &[
            Op::Block(Box::new(Block {
                kind: BlockKind::Linear,
                edits: vec![(0, 1), (1, 2), (0, -1)],
                moves: vec![1, 0],
                advance: 0,
                min_offset: 0,
                max_offset: 1,
            })),
            Op::Output,
        ]
    );
    assert_eq!(program.operation_count(), 2);
    let mut output = Vec::new();
    let stats = program.run_with_stats(&[][..], &mut output).unwrap();
    assert_eq!(output, [0]);
    assert_eq!(stats.executed_ops, 2);
}

#[test]
fn bf_optimizer_keeps_order_and_io_boundaries() {
    let program = Program::parse("+>++<->+<.>.<.").unwrap();
    let mut output = Vec::new();
    program.run(&[][..], &mut output).unwrap();
    assert_eq!(output, [0, 3, 0]);

    let mut output = Vec::new();
    run_source("->++<-.>.", &[][..], &mut output).unwrap();
    assert_eq!(output, [254, 2]);

    let program = Program::parse("++[>+<-.]").unwrap();
    assert!(matches!(program.ops()[1], Op::Loop(_)));
    let mut output = Vec::new();
    program.run(&[][..], &mut output).unwrap();
    assert_eq!(output, [1, 0]);
}

#[test]
fn bf_optimizer_checks_intermediate_pointer_bounds() {
    let error = run_source("<+>+", &[][..], Vec::new()).unwrap_err();
    assert!(error.contains("before cell 0"));

    let source = format!(">+{}+<+", ">".repeat(29_999));
    let error = run_source(&source, &[][..], Vec::new()).unwrap_err();
    assert!(error.contains("past cell 29999"));
}

#[test]
fn bf_optimizer_does_not_cross_dispatch_markers() {
    let source = format!("+>+{DISPATCH_MARKER}<+>");
    let program = Program::parse(&source).unwrap();
    assert!(matches!(program.ops()[1], Op::Dispatch));
    assert_eq!(
        program
            .run_with_stats(&[][..], Vec::new())
            .unwrap()
            .dispatches,
        1
    );
}

fn byte_array_fixture(marker: &str, template: &str, index: u8, transfer: u8) -> String {
    let mut source = ">".repeat(24);
    source.push('>');
    source.push_str(&"+".repeat(usize::from(index)));
    source.push('>');
    source.push_str(&"+".repeat(usize::from(index)));
    source.push('>');
    source.push_str(&"+".repeat(usize::from(transfer)));
    source.push_str(">+");
    source.push_str(&">".repeat(usize::from(index)));
    source.push_str("+++");
    source.push_str(&"<".repeat(usize::from(index) + 4));
    source.push_str(marker);
    source.push_str(template);
    source.push_str(&".>".repeat(usize::from(index) + 6));
    source
}

#[test]
fn bf_interpreter_recognizes_byte_array_templates_without_changing_results() {
    let templates = [
        (
            ARRAY_GET_MARKER,
            ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<",
            BlockKind::ArrayGet,
        ),
        (
            ARRAY_SET_MARKER,
            ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<",
            BlockKind::ArraySet,
        ),
    ];
    for (marker, template, expected_kind) in templates {
        for index in [0, 1, 2, 7, 31, 127, 200, 255] {
            let transfer = if marker == ARRAY_GET_MARKER { 0 } else { 7 };
            let source = byte_array_fixture(marker, template, index, transfer);
            let raw_source = source.replace(marker, "");
            let mut optimized_output = Vec::new();
            let mut raw_output = Vec::new();
            let optimized = Program::parse(&source).unwrap();
            assert!(
                optimized
                    .ops()
                    .iter()
                    .any(|op| { matches!(op, Op::Block(block) if block.kind == expected_kind) })
            );
            optimized.run(&[][..], &mut optimized_output).unwrap();
            Program::parse(&raw_source)
                .unwrap()
                .run(&[][..], &mut raw_output)
                .unwrap();
            assert_eq!(optimized_output, raw_output, "{marker} index {index}");
            assert_eq!(
                source
                    .chars()
                    .filter(|ch| matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']'))
                    .collect::<String>(),
                raw_source
            );
        }
    }
}

#[test]
fn bf_interpreter_falls_back_if_array_preconditions_or_template_differ() {
    let template = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<";
    let source = byte_array_fixture(ARRAY_GET_MARKER, template, 0, 5);
    let optimized = Program::parse(&source).unwrap();
    let mut actual = Vec::new();
    let stats = optimized.run_with_stats(&[][..], &mut actual).unwrap();
    let mut expected = Vec::new();
    run_source(
        &source.replace(ARRAY_GET_MARKER, ""),
        &[][..],
        &mut expected,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert!(stats.executed_ops > optimized.operation_count() as u64);

    let altered = format!("{ARRAY_GET_MARKER}+. ");
    assert_eq!(
        Program::parse(&altered).unwrap().ops(),
        &[Op::Add(1), Op::Output]
    );
}

fn wide_array_fixture(
    marker: &str,
    template: &str,
    lane: usize,
    index: u8,
    second_index: u8,
    transfer: u8,
    space: u8,
) -> String {
    let mut source = ">".repeat(24 + lane);
    source.push_str(&"+".repeat(usize::from(space)));
    source.push_str(">>>>");
    source.push_str(&"+".repeat(usize::from(index)));
    source.push_str(">>>>");
    source.push_str(&"+".repeat(usize::from(second_index)));
    source.push_str(">>>>");
    source.push_str(&"+".repeat(usize::from(transfer)));
    source.push_str(&">>>>".repeat(usize::from(index) + 1));
    source.push_str("+++++++");
    source.push_str(&"<<<<".repeat(usize::from(index) + 4));
    source.push_str(marker);
    source.push_str(template);
    source.push_str(".>>>>.>>>>.>>>>.");
    source.push_str(&">>>>".repeat(usize::from(index) + 1));
    source.push_str(".>.>.>.>. ");
    source
}

#[test]
fn bf_interpreter_recognizes_stride_four_array_templates_on_every_lane() {
    let get_template = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<";
    let set_template =
        ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<";
    for (marker, template, expected_kind) in [
        (ARRAY_GET_WIDE_MARKER, get_template, BlockKind::ArrayGetWide),
        (ARRAY_SET_WIDE_MARKER, set_template, BlockKind::ArraySetWide),
    ] {
        let strided = template.replace('>', ">>>>").replace('<', "<<<<");
        for lane in 0..4 {
            for index in [0, 1, 2, 7, 127, 255] {
                let transfer = if marker == ARRAY_GET_WIDE_MARKER {
                    0
                } else {
                    11
                };
                let source = wide_array_fixture(marker, &strided, lane, index, index, transfer, 0);
                let raw_source = source.replace(marker, "");
                let program = Program::parse(&source).unwrap();
                assert!(
                    program.ops().iter().any(|op| {
                        matches!(op, Op::Block(block) if block.kind == expected_kind)
                    })
                );
                let mut actual = Vec::new();
                let mut expected = Vec::new();
                program.run(&[][..], &mut actual).unwrap();
                run_source(&raw_source, &[][..], &mut expected).unwrap();
                assert_eq!(actual, expected, "{marker} lane {lane} index {index}");
            }
        }
    }
}

#[test]
fn bf_interpreter_falls_back_for_stride_four_metadata_and_template_mismatch() {
    let template = ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<"
        .replace('>', ">>>>")
        .replace('<', "<<<<");
    for (index, second_index, transfer, space) in [(0, 0, 5, 0), (1, 1, 0, 1), (0, 1, 0, 0)] {
        let source = wide_array_fixture(
            ARRAY_GET_WIDE_MARKER,
            &template,
            2,
            index,
            second_index,
            transfer,
            space,
        );
        let program = Program::parse(&source).unwrap();
        let mut actual = Vec::new();
        let stats = program.run_with_stats(&[][..], &mut actual).unwrap();
        let mut expected = Vec::new();
        run_source(
            &source.replace(ARRAY_GET_WIDE_MARKER, ""),
            &[][..],
            &mut expected,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert!(stats.executed_ops > program.operation_count() as u64);
    }

    let set_template =
        ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<"
            .replace('>', ">>>>")
            .replace('<', "<<<<");
    let source = wide_array_fixture(ARRAY_SET_WIDE_MARKER, &set_template, 3, 0, 1, 9, 0);
    let program = Program::parse(&source).unwrap();
    let mut actual = Vec::new();
    let stats = program.run_with_stats(&[][..], &mut actual).unwrap();
    let mut expected = Vec::new();
    run_source(
        &source.replace(ARRAY_SET_WIDE_MARKER, ""),
        &[][..],
        &mut expected,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert!(stats.executed_ops > program.operation_count() as u64);

    let altered = format!("{ARRAY_GET_WIDE_MARKER}+. ");
    assert_eq!(
        Program::parse(&altered).unwrap().ops(),
        &[Op::Add(1), Op::Output]
    );
}

#[test]
fn bf_interpreter_falls_back_for_stride_four_near_tape_end() {
    for (marker, template) in [
        (
            ARRAY_GET_WIDE_MARKER,
            ">[>>>[-<<<<+>>>>]<<[->+<]<[->+<]>-]>>>[-<+<<+>>>]<<<[->>>+<<<]>[[-<+>]>[-<+>]<<<<[->>>>+<<<<]>>-]<<",
        ),
        (
            ARRAY_SET_WIDE_MARKER,
            ">[>>>[-<<<<+>>>>]<[->+<]<[->+<]<[->+<]>-]>>>[-]<[->+<]<[[-<+>]<<<[->>>>+<<<<]>>-]<<",
        ),
    ] {
        let strided = template.replace('>', ">>>>").replace('<', "<<<<");
        let source = format!("{}{marker}{strided}.", ">".repeat(29_980));
        let raw_source = source.replace(marker, "");
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        let program = Program::parse(&source).unwrap();
        let result = program.run_with_stats(&[][..], &mut actual);
        let raw_result = run_source(&raw_source, &[][..], &mut expected);
        assert_eq!(
            result.as_ref().map(|_| ()),
            raw_result.as_ref().map(|_| ()),
            "{marker}"
        );
        assert_eq!(actual, expected, "{marker}");
        if let Ok(stats) = result {
            assert!(stats.executed_ops > program.operation_count() as u64);
        }
    }
}

#[test]
fn compiled_wide_arrays_match_unmarked_bf_for_int_and_fixed() {
    for (value_type, value, expected) in [
        ("int", "66051", b"66051".as_slice()),
        ("fixed", "1.25", b"1.25".as_slice()),
    ] {
        let source = format!(
            "let values: {value_type}[4];\
             let index: byte = 0;\
             while index < 4 {{\
                 values[index] = {value};\
                 index += 1;\
             }}\
             index = 2;\
             print values[index];"
        );
        let bf = sane::compile_source(&source).unwrap();
        assert_eq!(bf.matches(ARRAY_GET_WIDE_MARKER).count(), 4);
        assert_eq!(bf.matches(ARRAY_SET_WIDE_MARKER).count(), 4);
        let raw_bf = bf
            .replace(ARRAY_GET_WIDE_MARKER, "")
            .replace(ARRAY_SET_WIDE_MARKER, "");
        let mut actual = Vec::new();
        let mut unmarked = Vec::new();
        run_source(&bf, &[][..], &mut actual).unwrap();
        run_source(&raw_bf, &[][..], &mut unmarked).unwrap();
        assert_eq!(actual, expected, "{value_type}");
        assert_eq!(actual, unmarked, "{value_type}");
        assert_eq!(
            bf.chars()
                .filter(|ch| matches!(ch, '>' | '<' | '+' | '-' | '.' | ',' | '[' | ']'))
                .collect::<String>(),
            raw_bf
        );
    }
}

#[test]
fn bf_interpreter_reports_unmatched_brackets() {
    assert!(Program::parse("[").is_err());
    assert!(Program::parse("]").is_err());
}

#[test]
fn bf_interpreter_reports_deterministic_execution_stats() {
    let source = format!("+[{DISPATCH_MARKER}-]");
    let program = Program::parse(&source).unwrap();
    let stats = program.run_with_stats(&[][..], Vec::new()).unwrap();

    assert_eq!(program.operation_count(), 4);
    assert_eq!(
        stats,
        ExecutionStats {
            executed_ops: 4,
            dispatches: 1,
        }
    );
}
