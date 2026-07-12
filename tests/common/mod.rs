pub fn run_bf(code: &str, input: &[u8]) -> Vec<u8> {
    let mut tape = vec![0u8; 30_000];
    let mut ptr = 0usize;
    let mut pc = 0usize;
    let bytes = code.as_bytes();
    let mut input_pos = 0usize;
    let mut output = Vec::new();

    while pc < bytes.len() {
        match bytes[pc] {
            b'>' => ptr += 1,
            b'<' => ptr -= 1,
            b'+' => tape[ptr] = tape[ptr].wrapping_add(1),
            b'-' => tape[ptr] = tape[ptr].wrapping_sub(1),
            b'.' => output.push(tape[ptr]),
            b',' => {
                tape[ptr] = input.get(input_pos).copied().unwrap_or(0);
                input_pos += 1;
            }
            b'[' if tape[ptr] == 0 => {
                let mut depth = 1usize;
                while depth != 0 {
                    pc += 1;
                    match bytes[pc] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        _ => {}
                    }
                }
            }
            b']' if tape[ptr] != 0 => {
                let mut depth = 1usize;
                while depth != 0 {
                    pc -= 1;
                    match bytes[pc] {
                        b']' => depth += 1,
                        b'[' => depth -= 1,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        pc += 1;
    }

    output
}
