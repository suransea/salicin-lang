pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;

/// Compile one UTF-8 Salicin source file to textual LLVM IR.
pub fn compile_source(source: &str) -> Result<String, Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::compile(&program).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("error: {diagnostic}"))
            .collect()
    })
}
