pub mod ast;
pub mod bf;
pub mod debug;
pub mod diagnostic;
pub mod interpreter;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod sema;

pub fn compile_source(src: &str) -> Result<String, String> {
    compile_source_with_path(src, "<source>")
}

pub fn compile_source_with_path(src: &str, path: &str) -> Result<String, String> {
    compile_source_inner(src).map_err(|err| err.render(path, src))
}

pub fn compile_source_inner(src: &str) -> Result<String, diagnostic::Diagnostic> {
    let tokens = lexer::lex(src)?;
    let mut parser = parser::Parser::new(tokens);
    let program = parser.parse_program()?;
    let program = sema::resolve(&program)?;
    Ok(bf::compile(&program)?)
}
