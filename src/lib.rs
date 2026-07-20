pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod manifest;
pub mod modules;
pub mod parser;

fn format_codegen_diagnostics(diagnostics: Vec<codegen::Diagnostic>) -> Vec<String> {
    diagnostics
        .into_iter()
        .map(|diagnostic| format!("error: {diagnostic}"))
        .collect()
}

/// Compile one UTF-8 Salicin source file to textual LLVM IR.
pub fn compile_source(source: &str) -> Result<String, Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::compile(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and compile a Salicin binary from multiple source units to textual
/// LLVM IR.
pub fn compile_source_units(units: &[modules::SourceUnit]) -> Result<String, Vec<String>> {
    let program = modules::resolve_sources(units)?;
    codegen::compile(&program).map_err(format_codegen_diagnostics)
}

/// Compile one UTF-8 Salicin library source file to textual LLVM IR without a
/// platform `main` wrapper.
pub fn compile_library_source(source: &str) -> Result<String, Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::compile_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and compile a Salicin library from multiple source units to textual
/// LLVM IR without a platform `main` wrapper.
pub fn compile_library_source_units(units: &[modules::SourceUnit]) -> Result<String, Vec<String>> {
    let program = modules::resolve_sources(units)?;
    codegen::compile_library(&program).map_err(format_codegen_diagnostics)
}

/// Parse and type-check one Salicin library source file without requiring a
/// `main` entry point.
pub fn check_library_source(source: &str) -> Result<(), Vec<String>> {
    let program = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    codegen::check_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a Salicin binary assembled from multiple source
/// units. A valid `main` entry point is required.
pub fn check_source_units(units: &[modules::SourceUnit]) -> Result<(), Vec<String>> {
    let program = modules::resolve_sources(units)?;
    codegen::compile(&program)
        .map(|_| ())
        .map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a Salicin library assembled from multiple source
/// units without requiring a `main` entry point.
pub fn check_library_source_units(units: &[modules::SourceUnit]) -> Result<(), Vec<String>> {
    let program = modules::resolve_sources(units)?;
    codegen::check_library(&program).map_err(format_codegen_diagnostics)
}
