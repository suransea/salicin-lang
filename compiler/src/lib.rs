#[cfg(not(target_pointer_width = "64"))]
compile_error!("Salicin currently supports only 64-bit compiler hosts and native targets");

pub mod alloc;
pub mod ast;
mod cleanup;
pub mod codegen;
pub mod core;
pub mod editor;
pub mod formatter;
pub mod incremental;
pub mod incremental_cache;
pub mod lexer;
pub mod lockfile;
pub mod manifest;
pub mod modules;
pub mod parser;
pub mod standard;
pub mod static_semantics;

fn format_codegen_diagnostics(diagnostics: Vec<codegen::Diagnostic>) -> Vec<String> {
    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let Some(location) = diagnostic
                .origin
                .as_ref()
                .and_then(|origin| origin.source.as_deref())
                .filter(|location| location.line > 0)
            else {
                return format!("error: {diagnostic}");
            };
            let location = match location.path.as_deref() {
                Some("<source>") | None => format!("{}:{}", location.line, location.column),
                Some(path) => format!("{path}:{}:{}", location.line, location.column),
            };
            format!("{location}: error: {diagnostic}")
        })
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
    let program = without_test_registrations(parse_and_resolve_single_source(source)?);
    codegen::compile(&program).map_err(format_codegen_diagnostics)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCompilation {
    pub ir: String,
    pub names: Vec<String>,
}

/// Compile all contextual `test("name") { ... }` registrations in one source
/// file into a single native-runner module.
pub fn compile_test_source(source: &str) -> Result<TestCompilation, Vec<String>> {
    compile_test_source_filtered(source, None)
}

/// Compile the contextual registrations whose names contain `filter`.
/// Selection is case-sensitive and retains source order.
pub fn compile_test_source_filtered(
    source: &str,
    filter: Option<&str>,
) -> Result<TestCompilation, Vec<String>> {
    let program = parse_and_resolve_single_source(source)?;
    compile_test_program(program, None, filter)
}

/// Resolve and compile a Salicin binary from multiple source units to textual
/// LLVM IR.
pub fn compile_source_units(units: &[modules::SourceUnit]) -> Result<String, Vec<String>> {
    let program = without_test_registrations(modules::resolve_sources(units)?);
    codegen::compile(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and compile a binary from a complete package dependency graph.
pub fn compile_source_packages(packages: &[modules::SourcePackage]) -> Result<String, Vec<String>> {
    let program = without_test_registrations(modules::resolve_packages(packages)?);
    codegen::compile(&program).map_err(format_codegen_diagnostics)
}

/// Resolve a package graph and compile the selected package's test
/// declarations into one native-runner module.
pub fn compile_test_source_packages(
    packages: &[modules::SourcePackage],
) -> Result<TestCompilation, Vec<String>> {
    compile_test_source_packages_filtered(packages, None)
}

/// Resolve a package graph and compile only primary-package registrations
/// whose names contain `filter`, preserving primary source order.
pub fn compile_test_source_packages_filtered(
    packages: &[modules::SourcePackage],
    filter: Option<&str>,
) -> Result<TestCompilation, Vec<String>> {
    let primary_package = packages
        .iter()
        .find(|package| package.is_primary)
        .map(|package| package.id.0);
    let program = modules::resolve_packages(packages)?;
    compile_test_program(program, primary_package, filter)
}

fn compile_test_program(
    program: ast::Program,
    primary_package: Option<usize>,
    filter: Option<&str>,
) -> Result<TestCompilation, Vec<String>> {
    let (program, names) = test_runner_program(program, primary_package, filter)?;
    let ir = codegen::compile(&program).map_err(format_codegen_diagnostics)?;
    Ok(TestCompilation { ir, names })
}

fn test_runner_program(
    mut program: ast::Program,
    primary_package: Option<usize>,
    filter: Option<&str>,
) -> Result<(ast::Program, Vec<String>), Vec<String>> {
    let all_tests = program
        .items
        .iter()
        .zip(&program.item_origins)
        .filter_map(|(item, origin)| {
            if primary_package.is_some_and(|package| origin.package != package) {
                return None;
            }
            let ast::Item::Function(function) = item else {
                return None;
            };
            let encoded = function.name.rsplit("::").next()?.strip_prefix("$test$")?;
            let name = decode_test_name(encoded)?;
            Some((function.name.clone(), name))
        })
        .collect::<Vec<_>>();
    if all_tests.is_empty() {
        return Err(vec![
            "error: test target contains no test declarations".to_owned()
        ]);
    }
    let mut seen = std::collections::HashSet::new();
    let mut reported = std::collections::HashSet::new();
    let duplicate_diagnostics = all_tests
        .iter()
        .filter_map(|(_, name)| {
            if seen.insert(name.clone()) || !reported.insert(name.clone()) {
                None
            } else {
                Some(format!("error: duplicate test registration name {name:?}"))
            }
        })
        .collect::<Vec<_>>();
    if !duplicate_diagnostics.is_empty() {
        return Err(duplicate_diagnostics);
    }
    let tests = all_tests
        .into_iter()
        .filter(|(_, name)| filter.is_none_or(|filter| name.contains(filter)))
        .collect::<Vec<_>>();
    let names = tests
        .iter()
        .map(|(_, name)| name.clone())
        .collect::<Vec<_>>();
    let selected_functions = tests
        .iter()
        .map(|(function, _)| function.clone())
        .collect::<std::collections::HashSet<_>>();

    let mut retained_items = Vec::with_capacity(program.items.len() + 1);
    let mut retained_visibilities = Vec::with_capacity(program.items.len() + 1);
    let mut retained_origins = Vec::with_capacity(program.items.len() + 1);
    for ((item, visibility), origin) in program
        .items
        .into_iter()
        .zip(program.item_visibilities)
        .zip(program.item_origins)
    {
        let is_unselected_test = is_test_registration(&item)
            && matches!(&item, ast::Item::Function(function)
                if !selected_functions.contains(&function.name));
        if matches!(&item, ast::Item::Function(function) if function.name == "main")
            || is_unselected_test
        {
            continue;
        }
        retained_items.push(item);
        retained_visibilities.push(visibility);
        retained_origins.push(origin);
    }

    let test_count = tests.len();
    let mut statements = vec![ast::Stmt::Let(ast::Binding {
        mutable: true,
        name: "$test_failures".to_owned(),
        annotation: Some(ast::Type::U64),
        value: ast::Expr::Integer(0),
        value_source: None,
    })];
    for (index, (test, _)) in tests.into_iter().enumerate() {
        let outcome = ast::Expr::Call(
            Box::new(ast::Expr::Name("core::testing::run".to_owned())),
            vec![ast::CallArg {
                label: None,
                value: ast::Expr::Name(test),
            }],
        );
        let reported = ast::Expr::Call(
            Box::new(ast::Expr::Name("std::test::report".to_owned())),
            vec![
                ast::CallArg {
                    label: None,
                    value: ast::Expr::Integer(
                        u128::try_from(index).expect("test index fits in u128"),
                    ),
                },
                ast::CallArg {
                    label: None,
                    value: outcome,
                },
            ],
        );
        statements.push(ast::Stmt::Expr(ast::Expr::If {
            condition: Box::new(reported),
            then_branch: Box::new(ast::Expr::Unit),
            else_branch: Some(Box::new(ast::Expr::Assign(
                Box::new(ast::Expr::Name("$test_failures".to_owned())),
                Box::new(ast::Expr::Binary(
                    Box::new(ast::Expr::Name("$test_failures".to_owned())),
                    ast::BinaryOp::Add,
                    Box::new(ast::Expr::Integer(1)),
                )),
            ))),
        }));
    }
    let body = ast::Expr::Block(
        statements,
        Some(Box::new(ast::Expr::Call(
            Box::new(ast::Expr::Name("std::test::finish".to_owned())),
            vec![
                ast::CallArg {
                    label: None,
                    value: ast::Expr::Integer(
                        u128::try_from(test_count).expect("test count fits in u128"),
                    ),
                },
                ast::CallArg {
                    label: None,
                    value: ast::Expr::Name("$test_failures".to_owned()),
                },
            ],
        ))),
    );
    retained_items.push(ast::Item::Function(ast::Function {
        name: "main".to_owned(),
        foreign: None,
        builtin: false,
        compile_groups: Vec::new(),
        groups: vec![Vec::new()],
        return_type: Some(ast::Type::I32),
        effects: ast::FunctionEffects::default(),
        where_predicates: Vec::new(),
        body: Some(body),
    }));
    retained_visibilities.push(ast::Visibility::Private);
    retained_origins.push(ast::ItemOrigin::default());

    program.items = retained_items;
    program.item_visibilities = retained_visibilities;
    program.item_origins = retained_origins;
    Ok((program, names))
}

fn decode_test_name(encoded: &str) -> Option<String> {
    if !encoded.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..encoded.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn is_test_registration(item: &ast::Item) -> bool {
    matches!(
        item,
        ast::Item::Function(function)
            if function
                .name
                .rsplit("::")
                .next()
                .is_some_and(|name| name.starts_with("$test$"))
    )
}

fn without_test_registrations(mut program: ast::Program) -> ast::Program {
    let mut items = Vec::with_capacity(program.items.len());
    let mut visibilities = Vec::with_capacity(program.item_visibilities.len());
    let mut origins = Vec::with_capacity(program.item_origins.len());
    for ((item, visibility), origin) in program
        .items
        .into_iter()
        .zip(program.item_visibilities)
        .zip(program.item_origins)
    {
        if !is_test_registration(&item) {
            items.push(item);
            visibilities.push(visibility);
            origins.push(origin);
        }
    }
    program.items = items;
    program.item_visibilities = visibilities;
    program.item_origins = origins;
    program
}

/// Compile one UTF-8 Salicin library source file to textual LLVM IR without a
/// platform `main` wrapper.
pub fn compile_library_source(source: &str) -> Result<String, Vec<String>> {
    let program = without_test_registrations(parse_and_resolve_single_source(source)?);
    codegen::compile_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and compile a Salicin library from multiple source units to textual
/// LLVM IR without a platform `main` wrapper.
pub fn compile_library_source_units(units: &[modules::SourceUnit]) -> Result<String, Vec<String>> {
    let program = without_test_registrations(modules::resolve_sources(units)?);
    codegen::compile_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and compile a library from a complete package dependency graph.
pub fn compile_library_source_packages(
    packages: &[modules::SourcePackage],
) -> Result<String, Vec<String>> {
    let program = without_test_registrations(modules::resolve_packages(packages)?);
    codegen::compile_library(&program).map_err(format_codegen_diagnostics)
}

/// Parse and type-check one Salicin binary source file, including its required
/// `main` entry point, without generating LLVM IR.
pub fn check_source(source: &str) -> Result<(), Vec<String>> {
    let program = without_test_registrations(parse_and_resolve_single_source(source)?);
    codegen::check(&program).map_err(format_codegen_diagnostics)
}

/// Parse and type-check one Salicin library source file without requiring a
/// `main` entry point.
pub fn check_library_source(source: &str) -> Result<(), Vec<String>> {
    let program = without_test_registrations(parse_and_resolve_single_source(source)?);
    codegen::check_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a Salicin binary assembled from multiple source
/// units. A valid `main` entry point is required.
pub fn check_source_units(units: &[modules::SourceUnit]) -> Result<(), Vec<String>> {
    let program = without_test_registrations(modules::resolve_sources(units)?);
    codegen::check(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a binary assembled from a package dependency graph.
pub fn check_source_packages(packages: &[modules::SourcePackage]) -> Result<(), Vec<String>> {
    let program = without_test_registrations(modules::resolve_packages(packages)?);
    codegen::check(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a Salicin library assembled from multiple source
/// units without requiring a `main` entry point.
pub fn check_library_source_units(units: &[modules::SourceUnit]) -> Result<(), Vec<String>> {
    let program = without_test_registrations(modules::resolve_sources(units)?);
    codegen::check_library(&program).map_err(format_codegen_diagnostics)
}

/// Resolve and type-check a library assembled from a package dependency graph.
pub fn check_library_source_packages(
    packages: &[modules::SourcePackage],
) -> Result<(), Vec<String>> {
    let program = without_test_registrations(modules::resolve_packages(packages)?);
    codegen::check_library(&program).map_err(format_codegen_diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_definition_markers_are_private_to_core() {
        for (source, expected) in [
            (
                "let fake(): i32 = builtin()\nlet main(): i32 = { 0 }\n",
                "cannot define `fake`",
            ),
            (
                "let fake: type = builtin()\nlet main(): i32 = { 0 }\n",
                "cannot define type `fake`",
            ),
            (
                "extend(i32) { let fake(self)(): i32 = builtin() }\nlet main(): i32 = { 0 }\n",
                "cannot define extension methods",
            ),
        ] {
            let diagnostics = check_source(source).expect_err("user builtin marker must fail");
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.contains("private to the core package")
                        && diagnostic.contains(expected)
                }),
                "{diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_compilation_preserves_registration_names_and_rejects_invalid_targets() {
        let compilation = compile_test_source(
            "test(\"arithmetic\") { std.test.assert(20 + 22 == 42) }\n\
             test(\"utf-8: 盐\") { () }\n",
        )
        .expect("test registrations should compile into one runner");
        assert_eq!(compilation.names, ["arithmetic", "utf-8: 盐"]);
        assert!(compilation
            .ir
            .contains("define i32 @main(i32 %argc, ptr %argv)"));

        let filtered = compile_test_source_filtered(
            "test(\"alpha\") { () }\n\
             test(\"beta\") { std.test.fail(\"not selected\") }\n\
             test(\"alphabet\") { () }\n",
            Some("alpha"),
        )
        .expect("filtered registrations should compile into one runner");
        assert_eq!(filtered.names, ["alpha", "alphabet"]);
        assert!(!filtered.ir.contains("62657461"));

        let empty_selection =
            compile_test_source_filtered("test(\"available\") { () }\n", Some("missing"))
                .expect("a filter with no matches should compile an empty runner");
        assert!(empty_selection.names.is_empty());
        assert!(empty_selection
            .ir
            .contains("define i32 @main(i32 %argc, ptr %argv)"));

        let no_tests =
            compile_test_source("let helper(): bool = { true }\n").expect_err("tests are required");
        assert!(no_tests
            .iter()
            .any(|diagnostic| diagnostic.contains("contains no test declarations")));

        let wrong_result =
            compile_test_source("test(\"wrong result\") { true }\n").expect_err("unit is required");
        assert!(
            wrong_result
                .iter()
                .any(|diagnostic| diagnostic.contains("expected `()`, found `bool`")),
            "{wrong_result:?}"
        );

        let ordinary = compile_source(
            "test(\"not part of a build\") { missing_test_only_name }\n\
             let main(): i32 = { 42 }\n",
        )
        .expect("ordinary builds should discard test registrations before checking");
        assert!(!ordinary.contains("not part of a build"));
    }

    #[test]
    fn single_source_entry_points_resolve_root_imports() {
        let source = "use root.answer as selected\n\
                      let answer(): i32 = { 42 }\n\
                      let main(): i32 = { selected() }\n";
        let ir = compile_source(source).expect("single-source import should resolve");
        assert!(ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
        assert!(ir.contains("call i32 @sali.fn.616e73776572()"));

        let library = "use root.answer as selected\n\
                       let answer(): i32 = { 42 }\n\
                       let read(): i32 = { selected() }\n";
        compile_library_source(library).expect("library import should resolve");
        check_library_source(library).expect("library import should type-check");
    }

    #[test]
    fn single_source_entry_points_validate_explicit_api_visibility_without_imports() {
        let source = "let hidden = struct {}\n\
                      pub let record = struct { pub value: hidden }\n";
        for errors in [
            compile_library_source(source).unwrap_err(),
            check_library_source(source).unwrap_err(),
        ] {
            assert!(errors.iter().any(|diagnostic| {
                diagnostic.contains("field `record.value`")
                    && diagnostic.contains("private type `hidden`")
            }));
        }
    }

    #[test]
    fn access_compile_parameters_select_shared_or_mutable_borrowing() {
        let source = "let inspect(comptime a: access)(value: borrow(a)(i32)): i32 = { value }\n\
                      let borrow_value(comptime a: access, comptime r: region, comptime t: type)\n\
                        (value: borrow(a)(r)(t)): borrow(a)(r)(t) = { borrow(a)(value) }\n\
                      let main(): i32 = {\n\
                        let mut left = 20\n\
                        let right = 22\n\
                        let mut third = 0\n\
                        let mutable = borrow_value(mut, i32)(left)\n\
                        let shared = borrow_value(t: i32)(right)\n\
                        mutable + shared + inspect(mut)(third)\n\
                      }\n";
        compile_source(source).expect("access-generic function should instantiate both modes");
    }

    #[test]
    fn closed_types_can_parameterize_compile_time_functions() {
        let source = "let optimization = enum { size, speed }\n\
                      let select_bool(b: bool)(value: i32): i32 = { value }\n\
                      let select_optimization(comptime o: optimization)(value: i32): i32 = { value }\n\
                      let main(): i32 = {\n\
                        select_bool(true)(20) +\n\
                          select_bool(false)(1) +\n\
                          select_optimization(size)(1)\n\
                      }\n";
        compile_source(source)
            .expect("bool and user-declared closed types should support compile-time values");
    }

    #[test]
    fn closed_compile_time_parameters_use_declared_defaults() {
        let source = "let select(comptime b: bool = false)(value: i32): i32 = { value }\n\
                      let main(): i32 = { select(42) }\n";
        compile_source(source).expect("closed compile-time defaults should be normalized by type");
    }

    #[test]
    fn closed_compile_time_defaults_are_checked_against_their_type() {
        let errors = compile_source(
            "let optimization = enum { size, speed }\n\
             let select(comptime o: optimization = true)(value: i32): i32 = { value }\n\
             let main(): i32 = { select(42) }\n",
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error.contains("default `true`") && error.contains("is not a member of `optimization`")
        }));
    }

    #[test]
    fn parameter_modifiers_are_type_checked_after_instantiation() {
        let source = "let decorate(comptime b: bool)(b value: i32): i32 = { value }\n\
                      let main(): i32 = { decorate(true)(42) }\n";
        let errors = compile_source(source).unwrap_err();
        assert!(errors.iter().any(|error| {
            error.contains("parameter modifier `b`")
                && error.contains("does not normalize to a `parameters` schema")
        }));
    }

    #[test]
    fn parameter_modifier_functions_can_be_forwarded_generically() {
        let source = "let modifier_identity(comptime m: (comptime p: parameters): parameters) = m\n\
             let apply(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }\n\
             let forward(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = {\n\
               apply(modifier_identity(m), t)(value)\n\
             }\n\
             let main(): i32 = {\n\
               forward(copy, i32)(20) + forward(move, i32)(22)\n\
             }\n";
        compile_source(source)
            .expect("copy and move parameter modifier functions should instantiate generically");
    }

    #[test]
    fn priority_semantic_diagnostics_include_source_statement_locations() {
        let cases = [
            (
                "ownership",
                "let resource = struct { value: i32 }\n\
                 let consume(move value: resource): () = { () }\n\
                 let main(): i32 = {\n\
                   let value = resource { value: 42 }\n\
                   consume(value)\n\
                   value.value\n\
                }\n",
                6,
                1,
                "moved",
            ),
            (
                "borrow",
                "let main(): i32 = {\n\
                   let mut value = 42\n\
                   let alias = borrow(value)\n\
                   value = 0\n\
                   alias\n\
                }\n",
                4,
                1,
                "borrow",
            ),
            (
                "trait",
                "let missing = struct { value: i32 }\n\
                let main(): i32 = { missing { value: 42 } + missing { value: 0 } }\n",
                2,
                21,
                "no matching `add` implementation",
            ),
            (
                "generic",
                "let identity(comptime t: type)(value: t): t = { value }\n\
                let main(): i32 = { identity() }\n",
                2,
                21,
                "argument",
            ),
            (
                "handler",
                "let ask = effect { let value(): i32 }\n\
                let main(): i32 = { ask.value() }\n",
                2,
                21,
                "requires custom effect",
            ),
            (
                "local initializer",
                "let main(): i32 = {\n\
                  let value: i32 = true\n\
                  value\n\
                }\n",
                2,
                18,
                "expected `i32`, found `bool`",
            ),
            (
                "trailing closure call",
                "let choose()(move action: (): bool): bool = { action() }\n\
                 let main(): i32 = { choose() { true } }\n",
                2,
                21,
                "expected `i32`, found `bool`",
            ),
        ];

        for (category, source, line, column, expected) in cases {
            let diagnostics = match check_source(source) {
                Ok(()) => panic!("{category} source unexpectedly passed"),
                Err(diagnostics) => diagnostics,
            };
            assert!(
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.starts_with(&format!("{line}:{column}: error:"))
                        && diagnostic.contains(expected)
                }),
                "{category}: {diagnostics:?}"
            );
            assert!(
                diagnostics
                    .iter()
                    .all(|diagnostic| !diagnostic.contains('$')),
                "{category} leaked an internal name: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn source_unit_semantic_diagnostics_include_the_defining_path() {
        let diagnostics = check_source_units(&[modules::SourceUnit {
            path: "src/main.sc".into(),
            module_path: Vec::new(),
            source: "let consume(move value: i32): () = { () }\n\
                     let main(): i32 = {\n\
                       let value = 42\n\
                       consume(value)\n\
                       value\n\
                     }\n"
            .into(),
            is_root: true,
        }])
        .unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.starts_with("src/main.sc:5:1: error:")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn alloc_accessors_use_the_access_generic_entry_points() {
        let source = "let box = alloc.boxed.box\n\
                      let vec = alloc.vec.vec
                      let main(): i32 = {\n\
                        let mut boxed = box.new(20)\n\
                        do {\n\
                          let value = boxed.as_ref(mut)()\n\
                          value = 21\n\
                        }\n\
                        let mut values: vec(i32) = vec(i32).new()\n\
                        values.push(20)\n\
                        do {\n\
                          let value = values.at(mut)(0)\n\
                          value = value + 1\n\
                        }\n\
                        boxed.read() + values.read(0)\n\
                      }\n";
        compile_source(source).expect("alloc accessors should instantiate mutable access");
    }

    #[test]
    fn alloc_helper_functions_are_not_public_api() {
        for (path, name) in [
            ("alloc.boxed.box_new", "box_new"),
            ("alloc.vec.vec_push", "vec_push"),
        ] {
            let source = format!("let helper = {path}\nlet main(): i32 = {{ 0 }}\n");
            let errors = compile_source(&source).unwrap_err();
            assert!(errors.iter().any(|diagnostic| {
                diagnostic.contains(name)
                    && (diagnostic.contains("unknown") || diagnostic.contains("private"))
            }));
        }
    }

    #[test]
    fn alloc_items_require_imports_and_may_be_renamed() {
        let errors = compile_source("let main(): i32 = { box.new(42).read() }\n").unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `box` is not in the prelude")
                && diagnostic.contains("let box = alloc.boxed.box")
        }));

        let source = "use alloc.boxed.box as heap_box\n\
                      let main(): i32 = { heap_box.new(42).read() }\n";
        compile_source(source).expect("renamed alloc import should compile");
    }

    #[test]
    fn local_names_may_shadow_unimported_alloc_items() {
        let source = "let box = struct { value: i32 }\n\
                      let vec = struct { value: i32 }\n\
                      let main(): i32 = { box { value: 20 }.value + vec { value: 22 }.value }\n";
        compile_source(source).expect("alloc names should not be reserved without an import");
    }

    #[test]
    fn operator_traits_require_imports_but_operator_syntax_does_not() {
        let missing = "let number = struct { value: i32 }\n\
                       extend(number, add(number)) {\n\
                         let output = number\n\
                         let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }\n\
                       }\n\
                       let main(): i32 = { 0 }\n";
        let errors = compile_source(missing).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `add` is not in the prelude")
                && diagnostic.contains("let add = core.ops.add")
        }));

        let imported = format!("use core.ops.add\n{missing}").replace(
            "let main(): i32 = { 0 }",
            "let main(): i32 = { (number { value: 20 } + number { value: 22 }).value }",
        );
        compile_source(&imported).expect("imported operator trait should define `+`");

        compile_source("let main(): i32 = { 20 + 22 }\n")
            .expect("built-in operator syntax should not require importing its protocol");

        let missing_order = "let number = struct { value: i32 }\n\
                             extend(number, partial_ord(number)) {\n\
                               let partial_cmp(self: borrow(self))(rhs: borrow(number)): core.ops.partial_ordering = {\n\
                                 core.ops.partial_ordering.equal\n\
                               }\n\
                             }\n\
                             let main(): i32 = { 0 }\n";
        let errors = compile_source(missing_order).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `partial_ord` is not in the prelude")
                && diagnostic.contains("let partial_ord = core.ops.partial_ord")
        }));

        let imported_order = format!(
            "use core.ops.{{partial_ord, partial_ordering}}\n{missing_order}"
        )
        .replace("core.ops.partial_ordering", "partial_ordering")
        .replace(
            "let main(): i32 = { 0 }",
            "let main(): i32 = { if number { value: 1 } <= number { value: 2 } { 42 } else { 0 } }",
        );
        compile_source(&imported_order)
            .expect("imported partial_ord should define ordering operators");

        let missing_unary = "let number = struct { value: i32 }\n\
                             extend(number, neg) {\n\
                               let output = number\n\
                               let neg(self)(): number = { self }\n}\n\
                             let main(): i32 = { 0 }\n";
        let errors = compile_source(missing_unary).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `neg` is not in the prelude")
                && diagnostic.contains("let neg = core.ops.neg")
        }));

        let imported_unary = format!("use core.ops.neg\n{missing_unary}").replace(
            "let main(): i32 = { 0 }",
            "let main(): i32 = { (-number { value: 42 }).value }",
        );
        compile_source(&imported_unary).expect("imported neg should define unary `-`");

        let missing_bitwise = "let bits = struct { value: i32 }\n\
                               extend(bits, bit_and(bits)) {\n\
                                 let output = bits\n\
                                 let bit_and(self)(rhs: bits): bits = { bits { value: self.value & rhs.value } }\n\
                               }\n\
                               let main(): i32 = { 0 }\n";
        let errors = compile_source(missing_bitwise).unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `bit_and` is not in the prelude")
                && diagnostic.contains("let bit_and = core.ops.bit_and")
        }));

        let imported_bitwise = format!("use core.ops.bit_and\n{missing_bitwise}").replace(
            "let main(): i32 = { 0 }",
            "let main(): i32 = { (bits { value: 6 } & bits { value: 3 }).value }",
        );
        compile_source(&imported_bitwise).expect("imported bit_and should define binary `&`");
        compile_source("let main(): i32 = { 6 & 3 }\n")
            .expect("built-in bitwise syntax should not require importing its protocol");

        let local = "let add = struct { value: i32 }\n\
                     let main(): i32 = { add { value: 42 }.value }\n";
        compile_source(local).expect("unimported operator names should remain available to users");
    }

    #[test]
    fn generic_inherent_methods_accept_member_compile_parameters() {
        let source = "let cell(comptime t: type) = struct { value: t }\n\
                      extend(cell(t)) {\n\
                        let make(comptime u: type)(move value: t)(marker: u): cell(t) = {\n\
                          cell(t) { value: value }\n\
                        }\n\
                        let view(comptime a: access)(self: borrow(a)(self))(): borrow(a)(t) = {\n\
                          borrow(a)(self.value)\n\
                        }\n\
                      }\n\
                      let main(): i32 = {\n\
                        let mut cell = cell.make(t: i32)(u: bool)(20)(true)\n\
                        let before = do {\n\
                          let reference = cell.view()\n\
                          reference\n\
                        }\n\
                        do {\n\
                          let reference = cell.view(a: mut)()\n\
                          reference = 21\n\
                        }\n\
                        do {\n\
                          let reference: borrow(mut)(i32) = cell.view()\n\
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

    #[test]
    fn slice_is_a_non_prelude_unsized_core_type() {
        check_library_source(
            "let slice = core.memory.slice\n\
             let inspect(comptime r: region)(values: borrow(r)(slice(i32))): u64 = { 0 }\n",
        )
        .expect("borrowed slice types should be accepted");

        let diagnostics = check_library_source(
            "let inspect(comptime r: region)(values: borrow(r)(slice(i32))): u64 = { 0 }\n",
        )
        .expect_err("slice must require an ordinary standard-library alias");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("let slice = core.memory.slice")));
    }

    #[test]
    fn arrays_unsize_to_region_bound_slice_borrows() {
        let ir = compile_source(
            "let slice = core.memory.slice\n\
             let view(comptime r: region)\n\
               (values: borrow(r)(array(i32)(3))): borrow(r)(slice(i32)) = {\n\
               borrow(values)\n\
             }\n\
             let main(): i32 = {\n\
               let values = [40, 1, 1]\n\
               let slice = view(borrow(values))\n\
               42\n\
             }\n",
        )
        .expect("array borrow should unsize to a slice borrow");
        assert!(ir.contains("{ ptr, i64 }"), "{ir}");
        assert!(ir.contains("insertvalue { ptr, i64 }"), "{ir}");
    }

    #[test]
    fn slice_methods_preserve_length_and_element_borrow_access() {
        let ir = compile_source(
            "let slice = core.memory.slice\n\
             let inspect(comptime r: region)\n\
               (values: borrow(r)(slice(i32))): i32 = {\n\
               let item = values.at(1)\n\
               if values.len() == 3 { item } else { 0 }\n\
             }\n\
             let main(): i32 = {\n\
               let values = [1, 42, 3]\n\
               let slice: borrow(slice(i32)) = borrow(values)\n\
               inspect(slice)\n\
             }\n",
        )
        .expect("slice len and shared element access should compile");
        assert!(ir.contains("extractvalue { ptr, i64 }"), "{ir}");
        assert!(ir.contains("slice.index.trap"), "{ir}");
    }

    #[test]
    fn vec_slice_borrows_preserve_mutable_element_access() {
        let ir = compile_source(
            "let vec = alloc.vec.vec\n\
             let main(): i32 = {\n\
               let mut values = vec.new(t: i32)()\n\
               values.push(1)\n\
               values.push(2)\n\
               do {\n\
                 let slice = values.as_slice(a: mut)()\n\
                 let item = slice.at(a: mut)(1)\n\
                 item = 42\n\
               }\n\
               values.read(1)\n\
             }\n",
        )
        .expect("vec mutable slice access should compile");
        assert!(ir.contains("insertvalue { ptr, i64 }"), "{ir}");
        assert!(ir.contains("slice.index.trap"), "{ir}");
    }

    #[test]
    fn user_index_protocol_dispatches_bracket_reads() {
        let ir = compile_source(
            "let index = core.ops.index\n\
             let bag = struct { value: i32 }\n\
             extend(bag, index(i32)) {\n\
               let output = i32\n\
               let index(comptime a: access)\n\
                 (self: borrow(a)(self))\n\
                 (key: i32): borrow(a)(i32) = {\n\
                 borrow(a)(self.value)\n\
               }\n\
             }\n\
             let main(): i32 = {\n\
               let bag = bag { value: 42 }\n\
               bag[0]\n\
             }\n",
        )
        .expect("user index implementation should drive bracket reads");
        assert!(ir.contains("call ptr @"), "{ir}");
        assert!(ir.contains("load i32, ptr"), "{ir}");
    }

    #[test]
    fn user_index_protocol_dispatches_bracket_assignment() {
        let ir = compile_source(
            "let index = core.ops.index\n\
             let bag = struct { value: i32 }\n\
             extend(bag, index(i32)) {\n\
               let output = i32\n\
               let index(comptime a: access)\n\
                 (self: borrow(a)(self))\n\
                 (key: i32): borrow(a)(i32) = {\n\
                 borrow(a)(self.value)\n\
               }\n\
             }\n\
             let main(): i32 = {\n\
               let mut bag = bag { value: 1 }\n\
               bag[0] = 42\n\
               bag[0]\n\
             }\n",
        )
        .expect("user index implementation should drive bracket assignment");
        assert!(ir.contains("store i32 42, ptr"), "{ir}");
    }

    #[test]
    fn user_index_protocol_preserves_explicit_borrows() {
        compile_source(
            "let index = core.ops.index\n\
             let bag = struct { value: i32 }\n\
             extend(bag, index(i32)) {\n\
               let output = i32\n\
               let index(comptime a: access)\n\
                 (self: borrow(a)(self))\n\
                 (key: i32): borrow(a)(i32) = {\n\
                 borrow(a)(self.value)\n\
               }\n\
             }\n\
             let read(value: borrow(i32)): i32 = { value }\n\
             let main(): i32 = {\n\
               let mut bag = bag { value: 42 }\n\
               let shared = borrow(bag[0])\n\
               read(shared)\n\
             }\n",
        )
        .expect("user index implementation should preserve explicit bracket borrows");
    }

    #[test]
    fn vec_index_protocol_supports_read_borrow_and_assignment() {
        compile_source(
            "let vec = alloc.vec.vec\n\
             let read(value: borrow(i32)): i32 = { value }\n\
             let main(): i32 = {\n\
               let mut values = vec.new(t: i32)()\n\
               values.push(1)\n\
               values[0] = 42\n\
               let value = borrow(values[0])\n\
               read(value)\n\
             }\n",
        )
        .expect("vec brackets should use its source index implementation");
    }

    #[test]
    fn slice_index_protocol_supports_read_borrow_and_assignment() {
        compile_source(
            "let slice = core.memory.slice\n\
             let inspect(values: borrow(mut)(slice(i32))): i32 = {\n\
               values[1] = 42\n\
               let value = borrow(values[1])\n\
               value\n\
             }\n\
             let main(): i32 = {\n\
               let mut values: array(i32)(2) = [1, 2]\n\
               let slice: borrow(mut)(slice(i32)) = borrow(mut)(values)\n\
               inspect(slice)\n\
             }\n",
        )
        .expect("slice brackets should use its source index implementation");
    }
}
