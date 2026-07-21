pub mod alloc;
pub mod ast;
mod cleanup;
pub mod codegen;
pub mod core;
pub mod lexer;
pub mod lockfile;
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

/// Resolve and compile a binary from a complete package dependency graph.
pub fn compile_source_packages(packages: &[modules::SourcePackage]) -> Result<String, Vec<String>> {
    let program = modules::resolve_packages(packages)?;
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

/// Resolve and compile a library from a complete package dependency graph.
pub fn compile_library_source_packages(
    packages: &[modules::SourcePackage],
) -> Result<String, Vec<String>> {
    let program = modules::resolve_packages(packages)?;
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

/// Resolve and type-check a binary assembled from a package dependency graph.
pub fn check_source_packages(packages: &[modules::SourcePackage]) -> Result<(), Vec<String>> {
    let program = modules::resolve_packages(packages)?;
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

/// Resolve and type-check a library assembled from a package dependency graph.
pub fn check_library_source_packages(
    packages: &[modules::SourcePackage],
) -> Result<(), Vec<String>> {
    let program = modules::resolve_packages(packages)?;
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

    #[test]
    fn single_source_entry_points_validate_explicit_api_visibility_without_imports() {
        let source = "let Hidden = struct()\n\
                      pub let Record = struct(pub value: Hidden)\n";
        for errors in [
            compile_library_source(source).unwrap_err(),
            check_library_source(source).unwrap_err(),
        ] {
            assert!(errors.iter().any(|diagnostic| {
                diagnostic.contains("field `Record.value`")
                    && diagnostic.contains("private type `Hidden`")
            }));
        }
    }

    #[test]
    fn access_compile_parameters_select_shared_or_mutable_borrowing() {
        let source = "let inspect(A: access)(borrow(A) value: i32): i32 = value\n\
                      let borrow_value(A: access, 'a: region, T: type)\n\
                        (borrow(A, 'a) value: T): borrow(A, 'a) T = borrow(A, 'a) value\n\
                      let main(): i32 = {\n\
                        let mut left = 20\n\
                        let right = 22\n\
                        let mut third = 0\n\
                        let mutable = borrow_value(mut, i32)(left)\n\
                        let shared = borrow_value(T: i32)(right)\n\
                        mutable + shared + inspect(mut)(third)\n\
                      }\n";
        compile_source(source).expect("access-generic function should instantiate both modes");
    }

    #[test]
    fn alloc_accessors_use_the_access_generic_entry_points() {
        let source = "use alloc.boxed.{Box, box_as_ref}\n\
                      use alloc.vec.{Vec, vec_at}\n\
                      let main(): i32 = {\n\
                        let mut boxed = Box.new(20)\n\
                        do {\n\
                          let value = box_as_ref(A: mut, T: i32)(boxed)\n\
                          value = 21\n\
                        }\n\
                        let mut values: Vec(i32) = Vec(i32).new()\n\
                        values.push(20)\n\
                        do {\n\
                          let value = vec_at(A: mut, T: i32)(values)(0)\n\
                          value = value + 1\n\
                        }\n\
                        boxed.read() + values.read(0)\n\
                      }\n";
        compile_source(source).expect("alloc accessors should instantiate mutable access");
    }

    #[test]
    fn alloc_items_require_imports_and_may_be_renamed() {
        let errors = compile_source("let main(): i32 = Box.new(42).read()\n").unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Box` is not in the prelude")
                && diagnostic.contains("use alloc.boxed.Box")
        }));

        let source = "use alloc.boxed.Box as HeapBox\n\
                      let main(): i32 = HeapBox.new(42).read()\n";
        compile_source(source).expect("renamed alloc import should compile");
    }

    #[test]
    fn local_names_may_shadow_unimported_alloc_items() {
        let source = "let Box = struct(value: i32)\n\
                      let Vec = struct(value: i32)\n\
                      let main(): i32 = Box(value: 20).value + Vec(value: 22).value\n";
        compile_source(source).expect("alloc names should not be reserved without an import");
    }

    #[test]
    fn operator_traits_require_imports_but_operator_syntax_does_not() {
        let missing = "let Number = struct(value: i32)\n\
                       extend Number: Add(Number) {\n\
                         let Output = Number\n\
                         let add(move self)(move rhs: Number): Number = Number(self.value + rhs.value)\n\
                       }\n\
                       let main(): i32 = 0\n";
        let errors = compile_source(missing).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Add` is not in the prelude")
                && diagnostic.contains("use core.ops.Add")
        }));

        let imported = format!("use core.ops.Add\n{missing}").replace(
            "let main(): i32 = 0",
            "let main(): i32 = (Number(20) + Number(22)).value",
        );
        compile_source(&imported).expect("imported operator trait should define `+`");

        compile_source("let main(): i32 = 20 + 22\n")
            .expect("built-in operator syntax should not require importing its protocol");

        let missing_order = "let Number = struct(value: i32)\n\
                             extend Number: PartialOrd(Number) {\n\
                               let partial_cmp(borrow self)(borrow rhs: Number): core.ops.PartialOrdering =\n\
                                 core.ops.PartialOrdering.Equal\n\
                             }\n\
                             let main(): i32 = 0\n";
        let errors = compile_source(missing_order).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `PartialOrd` is not in the prelude")
                && diagnostic.contains("use core.ops.PartialOrd")
        }));

        let imported_order =
            format!("use core.ops.{{PartialOrd, PartialOrdering}}\n{missing_order}")
                .replace("core.ops.PartialOrdering", "PartialOrdering")
                .replace(
                    "let main(): i32 = 0",
                    "let main(): i32 = if Number(1) <= Number(2) { 42 } else { 0 }",
                );
        compile_source(&imported_order)
            .expect("imported PartialOrd should define ordering operators");

        let local = "let Add = struct(value: i32)\n\
                     let main(): i32 = Add(value: 42).value\n";
        compile_source(local).expect("unimported operator names should remain available to users");
    }

    #[test]
    fn generic_inherent_methods_accept_member_compile_parameters() {
        let source = "let Cell(T: type) = struct(value: T)\n\
                      extend(T: type) Cell(T) {\n\
                        let make(U: type)(move value: T)(marker: U): Cell(T) =\n\
                          Cell(T)(value)\n\
                        let view(A: access)(borrow(A) self)(): borrow(A) T =\n\
                          borrow(A) self.value\n\
                      }\n\
                      let main(): i32 = {\n\
                        let mut cell = Cell.make(i32)(bool)(20)(true)\n\
                        let before = do {\n\
                          let reference = cell.view()\n\
                          reference\n\
                        }\n\
                        do {\n\
                          let reference = cell.view(mut)()\n\
                          reference = 21\n\
                        }\n\
                        do {\n\
                          let reference: borrow(mut) i32 = cell.view()\n\
                          reference = 22\n\
                        }\n\
                        let after = do {\n\
                          let reference = cell.view()\n\
                          reference\n\
                        }\n\
                        after - before\n\
                      }\n";
        compile_source(source)
            .expect("generic inherent methods should combine outer and member parameters");
    }
}
