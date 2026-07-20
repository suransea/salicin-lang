pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod manifest;
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

/// Compile one UTF-8 Salicin library source file to textual LLVM IR without a
/// platform `main` wrapper.
pub fn compile_library_source(source: &str) -> Result<String, Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::compile_library(&program).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("error: {diagnostic}"))
            .collect()
    })
}

/// Parse and type-check one Salicin library source file without requiring a
/// `main` entry point.
pub fn check_library_source(source: &str) -> Result<(), Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::check_library(&program).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("error: {diagnostic}"))
            .collect()
    })
}
