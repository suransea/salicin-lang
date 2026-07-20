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

fn parse_and_resolve_single_source(source: &str) -> Result<ast::Program, Vec<String>> {
    let parsed = parser::parse(source).map_err(|error| vec![format!("error: {error}")])?;
    if parsed.uses.is_empty() {
        return Ok(parsed);
    }

    modules::resolve_sources(&[modules::SourceUnit {
        path: "<source>".into(),
        module_path: Vec::new(),
        source: source.into(),
        is_root: true,
    }])
    .map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| {
                diagnostic
                    .strip_prefix("<source>: ")
                    .unwrap_or(&diagnostic)
                    .to_owned()
            })
            .collect()
    })
}

/// Compile one UTF-8 Salicin source file to textual LLVM IR.
pub fn compile_source(source: &str) -> Result<String, Vec<String>> {
    let program = parse_and_resolve_single_source(source)?;
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
    let program = parse_and_resolve_single_source(source)?;
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
    let program = parse_and_resolve_single_source(source)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_source_entry_points_resolve_root_imports() {
        let source = "use root.answer as selected\n\
                      let answer(): i32 = 42\n\
                      let main(): i32 = selected()\n";
        let ir = compile_source(source).expect("single-source import should resolve");
        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("call i32 @sali.fn.616e73776572()"));

        let library = "use root.answer as selected\n\
                       let answer(): i32 = 42\n\
                       let read(): i32 = selected()\n";
        compile_library_source(library).expect("library import should resolve");
        check_library_source(library).expect("library import should type-check");
    }
}
