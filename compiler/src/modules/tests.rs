use super::*;
use crate::ast::CallArg;

fn unit(path: &str, module_path: &[&str], source: &str, is_root: bool) -> SourceUnit {
    SourceUnit {
        path: path.to_owned(),
        module_path: module_path
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect(),
        source: source.to_owned(),
        is_root,
    }
}

#[test]
fn resolves_user_closed_compile_parameter_types_across_modules() {
    let program = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            "use root.config.optimization as optimization\n\
                 let select(comptime o: optimization)(value: i32): i32 = { value }\n\
                 let main(): i32 = { 0 }\n",
            true,
        ),
        unit(
            "config.sc",
            &["config"],
            "pub let optimization = enum { size, speed }\n",
            false,
        ),
    ])
    .unwrap();
    let function = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(function) if function.name == "select" => Some(function),
            _ => None,
        })
        .expect("resolved select function");
    assert!(matches!(
        &function.compile_groups[0][0].kind,
        Sort::Named(name) if name == "config::optimization"
    ));
}

fn package(
    id: usize,
    is_primary: bool,
    dependencies: &[(&str, usize)],
    sources: Vec<SourceUnit>,
) -> SourcePackage {
    SourcePackage {
        id: PackageId(id),
        name: format!("package-{id}"),
        version: "0.0.0".to_owned(),
        identity: format!("package-{id}@0.0.0"),
        is_primary,
        dependencies: dependencies
            .iter()
            .map(|(alias, target)| ((*alias).to_owned(), PackageId(*target)))
            .collect(),
        sources,
    }
}

fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
    program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(function) if function.name == name => Some(function),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function `{name}`"))
}

fn function_tail(function: &Function) -> &Expr {
    let Some(Expr::Block(_, Some(tail))) = &function.body else {
        panic!("expected function body block with a tail value");
    };
    tail.unlocated()
}

fn match_cases(expression: &Expr) -> Vec<&Expr> {
    fn flatten<'a>(expression: &'a Expr, groups: &mut Vec<&'a [CallArg]>) -> &'a Expr {
        let expression = expression.unlocated();
        if let Expr::Call(callee, arguments) = expression {
            let root = flatten(callee, groups);
            groups.push(arguments);
            root
        } else {
            expression
        }
    }

    let mut groups = Vec::new();
    assert_eq!(
        flatten(expression, &mut groups),
        &Expr::Name("$lang$match".into())
    );
    groups
        .into_iter()
        .skip(1)
        .map(|group| {
            let [CallArg { label: None, value }] = group else {
                panic!("expected one unlabeled match case");
            };
            value
        })
        .collect()
}

#[test]
fn rejects_duplicate_stable_package_identities() {
    let mut primary = package(
        1,
        true,
        &[("dependency", 2)],
        vec![unit("primary.sc", &[], "let main(): i32 = { 0 }\n", true)],
    );
    let mut dependency = package(
        2,
        false,
        &[],
        vec![unit(
            "dependency.sc",
            &[],
            "pub let answer(): i32 = { 42 }\n",
            true,
        )],
    );
    primary.name = "same".to_owned();
    primary.version = "1.0.0".to_owned();
    primary.identity = "registry:public|same@1.0.0".to_owned();
    dependency.name = "same".to_owned();
    dependency.version = "1.0.0".to_owned();
    dependency.identity = "registry:public|same@1.0.0".to_owned();

    let diagnostics =
        resolve_packages(&[primary, dependency]).expect_err("duplicate identity must fail");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("duplicate identity `registry:public|same@1.0.0`")));
}

#[test]
fn distinct_providers_may_share_a_package_name_and_version() {
    let mut primary = package(
        1,
        true,
        &[("dependency", 2)],
        vec![unit("primary.sc", &[], "let main(): i32 = { 0 }\n", true)],
    );
    let mut dependency = package(
        2,
        false,
        &[],
        vec![unit(
            "dependency.sc",
            &[],
            "pub let answer(): i32 = { 42 }\n",
            true,
        )],
    );
    primary.name = "same".to_owned();
    primary.version = "1.0.0".to_owned();
    primary.identity = "workspace:app|same@1.0.0".to_owned();
    dependency.name = "same".to_owned();
    dependency.version = "1.0.0".to_owned();
    dependency.identity = "registry:public|same@1.0.0".to_owned();

    let program =
        resolve_packages(&[primary, dependency]).expect("distinct providers are unambiguous");
    assert_eq!(program.package_identities[&1], "workspace:app|same@1.0.0");
    assert_eq!(program.package_identities[&2], "registry:public|same@1.0.0");
}

#[test]
fn flattens_modules_and_rewrites_calls_and_dotted_types() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "let main(): geometry.point = { geometry.make() }\n",
            true,
        ),
        unit(
            "src/geometry.sc",
            &["geometry"],
            "pub(package) let point = struct { x: i32, y: i32 }\n\
                 pub(package) let make(): point = { point { x: 1, y: 2 } }\n",
            false,
        ),
    ])
    .unwrap();

    assert!(program.items.iter().any(
        |item| matches!(item, Item::Struct(definition) if definition.name == "geometry::point")
    ));
    let main = function(&program, "main");
    assert_eq!(
        main.return_type,
        Some(Type::Named("geometry::point".into(), Vec::new()))
    );
    assert!(matches!(
        function_tail(main),
        Expr::Call(callee, arguments)
            if arguments.is_empty()
                && callee.as_ref() == &Expr::Name("geometry::make".into())
    ));

    let make = function(&program, "geometry::make");
    assert_eq!(
        make.return_type,
        Some(Type::Named("geometry::point".into(), Vec::new()))
    );
    assert!(matches!(
        function_tail(make),
        Expr::StructLiteral { constructor, .. }
            if constructor.as_ref() == &Expr::Name("geometry::point".into())
    ));
}

#[test]
fn resolves_longest_declaration_prefix_and_preserves_fields() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "let main(): i32 = { data.origin.x }\n",
            true,
        ),
        unit(
            "src/data.sc",
            &["data"],
            "pub(package) let point = struct { pub(package) x: i32 }\n\
                 pub(package) let origin = point { x: 1 }\n",
            false,
        ),
    ])
    .unwrap();

    assert!(matches!(
        function_tail(function(&program, "main")),
        Expr::Member(base, field)
            if field == "x" && base.as_ref() == &Expr::Name("data::origin".into())
    ));
}

#[test]
fn local_parameters_blocks_closures_and_match_bindings_shadow_modules() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "use core.option\n\
                 let keep(math: i32): i32 = {\n\
                   let local = { (math: i32) -> math }\n\
                   option.some(math) match { option.some(math) => local(math), _ => math }\n\
                 }\n",
            true,
        ),
        unit(
            "src/math.sc",
            &["math"],
            "pub(package) let value = 42\n",
            false,
        ),
    ])
    .unwrap();

    let Some(Expr::Block(statements, Some(tail))) = &function(&program, "keep").body else {
        panic!("expected block body");
    };
    let Stmt::Let(local) = &statements[0] else {
        panic!("expected local closure");
    };
    let Expr::Closure(_, closure_body) = &local.value else {
        panic!("expected closure");
    };
    assert!(matches!(
        closure_body.as_ref(),
        Expr::Block(_, Some(value)) if value.unlocated() == &Expr::Name("math".into())
    ));
    let cases = match_cases(tail);
    let Expr::PatternClosure { body, .. } = cases[0] else {
        panic!("expected pattern closure");
    };
    assert!(matches!(
        body.as_ref(),
        Expr::Call(_, arguments)
            if arguments[0].value == Expr::Name("math".into())
    ));
}

#[test]
fn reports_private_sibling_access_but_allows_descendants() {
    let error = resolve_sources(&[
        unit("src/main.sc", &[], "let main(): i32 = { b.read() }\n", true),
        unit("src/a.sc", &["a"], "let secret(): i32 = { 1 }\n", false),
        unit(
            "src/a/child.sc",
            &["a", "child"],
            "pub(package) let read(): i32 = { secret() }\n",
            false,
        ),
        unit(
            "src/b.sc",
            &["b"],
            "pub(package) let read(): i32 = { a.secret() }\n",
            false,
        ),
    ])
    .unwrap_err();

    assert_eq!(error.len(), 1, "{error:?}");
    assert!(error[0].contains("private to module `a`"), "{error:?}");
}

#[test]
fn preserves_self_and_associated_types_inside_traits_and_extensions() {
    let program = resolve_sources(&[
        unit("src/main.sc", &[], "let main(): i32 = { 0 }\n", true),
        unit(
            "src/api.sc",
            &["api"],
            "pub(package) let self = struct { value: i32 }\n\
                 pub(package) let output = struct { value: i32 }\n\
                 pub(package) let a = struct { value: i32 }\n\
                 pub(package) let convert = trait {\n\
                   let output: type\n\
                   let a: type\n\
                   let b: type\n\
                   let convert(self: borrow(self))(value: self): output\n\
                 }\n\
                 pub(package) let number = struct { value: i32 }\n\
                 extend(number, convert) {\n\
                   let output = i32\n\
                   let a = self\n\
                   let b = a\n\
                   let convert(self: borrow(self))(value: self): output = { value.value }\n}\n",
            false,
        ),
    ])
    .unwrap();

    let trait_definition = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Trait(definition) if definition.name == "api::convert" => Some(definition),
            _ => None,
        })
        .expect("missing resolved trait");
    let trait_method = trait_definition
        .members
        .iter()
        .find_map(|member| match member {
            TraitMember::Function(function) => Some(function),
            TraitMember::AssociatedType { .. } => None,
        })
        .expect("missing trait method");
    assert_eq!(
        trait_method.groups[0][0].ty,
        Type::Borrow {
            mutable: false,
            access: None,
            region: None,
            pointee: Box::new(Type::Named("self".into(), Vec::new())),
        }
    );
    assert_eq!(
        trait_method.groups[1][0].ty,
        Type::Named("self".into(), Vec::new())
    );
    assert_eq!(
        trait_method.return_type,
        Some(Type::Named("output".into(), Vec::new()))
    );

    let extension = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Extend(extension) => Some(extension),
            _ => None,
        })
        .expect("missing resolved extension");
    let implementation = extension
        .members
        .iter()
        .find_map(|member| match member {
            ExtendMember::Function(function) => Some(function),
            ExtendMember::Const(_) => None,
        })
        .expect("missing implementation method");
    assert_eq!(
        implementation.groups[0][0].ty,
        Type::Borrow {
            mutable: false,
            access: None,
            region: None,
            pointee: Box::new(Type::Named("self".into(), Vec::new())),
        }
    );
    assert_eq!(
        implementation.groups[1][0].ty,
        Type::Named("self".into(), Vec::new())
    );
    assert_eq!(
        implementation.return_type,
        Some(Type::Named("output".into(), Vec::new()))
    );
    let associated_values = extension
        .members
        .iter()
        .filter_map(|member| match member {
            ExtendMember::Const(binding) => Some((binding.name.as_str(), binding.value.clone())),
            ExtendMember::Function(_) => None,
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(associated_values["a"], Expr::Name("self".into()));
    assert_eq!(associated_values["b"], Expr::Name("a".into()));
}

#[test]
fn preserves_generic_extend_parameters_while_qualifying_the_target() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "let main(): i32 = { api.cell.new(42).take() }\n",
            true,
        ),
        unit(
            "src/api.sc",
            &["api"],
            "pub(package) let cell(comptime t: type) = struct { value: t }\n\
                 extend(cell(t)) {\n\
                   let new(move value: t): cell(t) = { cell { value: value } }\n\
                   let take(move self)(): t = { self.value }\n\
                 }\n",
            false,
        ),
    ])
    .unwrap();

    let extension = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Extend(extension) if !extension.compile_groups.is_empty() => Some(extension),
            _ => None,
        })
        .expect("missing generic extension");
    assert_eq!(extension.compile_groups[0][0].name, "t");
    assert_eq!(
        extension.target,
        Type::Named(
            "api::cell".into(),
            vec![Type::Named("t".into(), Vec::new())]
        )
    );
    let ExtendMember::Function(new) = &extension.members[0] else {
        panic!("missing associated constructor");
    };
    assert_eq!(
        new.return_type,
        Some(Type::Named(
            "api::cell".into(),
            vec![Type::Named("t".into(), Vec::new())]
        ))
    );
}

#[test]
fn reinfers_cross_module_extend_pattern_sorts_after_resolution() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "extend(api.handle(a)(t)) {}\nlet main(): i32 = { 0 }\n",
            true,
        ),
        unit(
            "src/api.sc",
            &["api"],
            "pub let mode = sort(1) { shared unique }\n\
                 pub let handle(comptime a: mode)(comptime t: type) = struct {}\n",
            false,
        ),
    ])
    .unwrap();

    let extension = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Extend(extension) => Some(extension),
            _ => None,
        })
        .expect("missing resolved extension");
    assert_eq!(
        extension.compile_groups[0]
            .iter()
            .map(|parameter| (parameter.name.as_str(), parameter.kind.clone()))
            .collect::<Vec<_>>(),
        vec![("a", Sort::Named("api::mode".into())), ("t", Sort::Type),]
    );
}

#[test]
fn leaves_unknown_names_for_semantic_analysis() {
    let program = resolve_sources(&[unit(
        "src/main.sc",
        &[],
        "let main(): i32 = { missing.value() }\n",
        true,
    )])
    .unwrap();

    assert!(matches!(
        function_tail(function(&program, "main")),
        Expr::Call(callee, _)
            if matches!(callee.as_ref(), Expr::Member(base, field)
                if base.as_ref() == &Expr::Name("missing".into()) && field == "value")
    ));
}

#[test]
fn rejects_duplicate_modules_declarations_and_module_name_conflicts() {
    let duplicate_module = resolve_sources(&[
        unit("root.sc", &[], "let main() = {}\n", true),
        unit("one.sc", &["net"], "let one = 1\n", false),
        unit("two.sc", &["net"], "let two = 2\n", false),
    ])
    .unwrap_err();
    assert!(duplicate_module
        .iter()
        .any(|diagnostic| diagnostic.contains("duplicate module `net`")));

    let duplicate_declaration =
        resolve_sources(&[unit("root.sc", &[], "let value = 1\nlet value = 2\n", true)])
            .unwrap_err();
    assert!(duplicate_declaration
        .iter()
        .any(|diagnostic| diagnostic.contains("duplicate declaration `value`")));

    let conflict = resolve_sources(&[
        unit("root.sc", &[], "let net = 1\n", true),
        unit("net.sc", &["net"], "let value = 2\n", false),
    ])
    .unwrap_err();
    assert!(conflict
        .iter()
        .any(|diagnostic| diagnostic.contains("conflicts with child module")));
}

#[test]
fn requires_exactly_one_root_source() {
    let no_root = resolve_sources(&[unit("a.sc", &["a"], "let value = 1\n", false)]).unwrap_err();
    assert!(no_root[0].contains("exactly one root source"));

    let two_roots = resolve_sources(&[
        unit("a.sc", &[], "let a = 1\n", true),
        unit("b.sc", &["b"], "let b = 2\n", true),
    ])
    .unwrap_err();
    assert!(two_roots
        .iter()
        .any(|diagnostic| diagnostic.contains("exactly one root source")));
}

#[test]
fn rejects_module_segments_that_are_unspellable_or_canonicalize_ambiguously() {
    for segment in ["", "_", "let", "Upper", "has-dash", "a.b", "a::b"] {
        let error = resolve_sources(&[
            unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
            unit("bad.sc", &[segment], "let value = 1\n", false),
        ])
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|diagnostic| diagnostic.contains("module path segment")),
            "segment `{segment}` was not rejected: {error:?}"
        );
    }
}

#[test]
fn resolves_forward_aliases_module_aliases_and_reexport_chains() {
    let program = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            "let selected = root.facade.answer\nlet main(): i32 = { selected() }\n",
            true,
        ),
        unit(
            "facade.sc",
            &["facade"],
            "pub let answer = root.implementation.answer\n",
            false,
        ),
        unit(
            "implementation.sc",
            &["implementation"],
            "pub let answer(): i32 = { 42 }\n",
            false,
        ),
    ])
    .unwrap();

    assert!(program.uses.is_empty());
    assert!(matches!(
        function_tail(function(&program, "main")),
        Expr::Call(callee, _)
            if callee.as_ref() == &Expr::Name("implementation::answer".into())
    ));
}

#[test]
fn rejects_bare_modules_before_they_can_fall_back_to_prelude_names() {
    let errors = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            r#"use root.fake as option
let add = root.fake
let never = root.fake

let number = struct { value: i32 }
extend(number, add(number)) {
  let output = i32
  let add(self)(rhs: number): i32 = { self.value + rhs.value }
}

let stop(): never = { loop {} }
let main(): i32 = { option {} }
"#,
            true,
        ),
        unit("fake.sc", &["fake"], "let marker = 0\n", false),
    ])
    .unwrap_err();

    for expected in [
        "module `option` cannot be used as a value or callable",
        "module `add` cannot be used as a type",
        "module `never` cannot be used as a type",
    ] {
        assert!(
            errors
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "missing `{expected}` in {errors:?}"
        );
    }
}

#[test]
fn resolves_root_self_super_and_anchor_only_module_aliases() {
    let program = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            "let root_value(): i32 = { 10 }\nlet main(): i32 = { nested.deep.answer() }\n",
            true,
        ),
        unit(
            "nested.sc",
            &["nested"],
            "let parent_value(): i32 = { 20 }\n",
            false,
        ),
        unit(
            "nested/deep.sc",
            &["nested", "deep"],
            "let pkg = root\n\
                 let local = self.local_value\n\
                 let parent = super.parent_value\n\
                 let local_value(): i32 = { 12 }\n\
                 pub(package) let answer(): i32 = { pkg.root_value() + parent() + local() }\n",
            false,
        ),
    ])
    .unwrap();

    let names = expression_names(function(&program, "nested::deep::answer").body.as_ref());
    assert!(names.contains("root_value"));
    assert!(names.contains("nested::parent_value"));
    assert!(names.contains("nested::deep::local_value"));
}

#[test]
fn rejects_import_cycles_privacy_and_visibility_escalation() {
    let cycle = resolve_sources(&[unit(
        "root.sc",
        &[],
        "use root.second as first\nuse root.first as second\nlet main(): i32 = { 0 }\n",
        true,
    )])
    .unwrap_err();
    assert!(cycle.iter().any(|diagnostic| {
        diagnostic.contains("cyclic import aliases")
            && diagnostic.contains("first")
            && diagnostic.contains("second")
    }));

    let private = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            "use root.sibling.secret\nlet main(): i32 = { secret() }\n",
            true,
        ),
        unit(
            "sibling.sc",
            &["sibling"],
            "let secret(): i32 = { 1 }\n",
            false,
        ),
    ])
    .unwrap_err();
    assert!(private.iter().any(|diagnostic| {
        diagnostic.contains("private") && diagnostic.contains("sibling.secret")
    }));

    let promotion = resolve_sources(&[
        unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
        unit(
            "facade.sc",
            &["facade"],
            "pub(package) let internal(): i32 = { 1 }\npub use self.internal as exposed\n",
            false,
        ),
    ])
    .unwrap_err();
    assert!(promotion.iter().any(|diagnostic| {
        diagnostic.contains("pub use")
            && diagnostic.contains("pub(package)")
            && diagnostic.contains("facade.internal")
    }));
}

#[test]
fn preserves_private_module_alias_boundaries_and_skips_relative_self_aliases() {
    let program = resolve_sources(&[
        unit(
            "root.sc",
            &[],
            "pub(package) let answer(): i32 = { 42 }\nlet main(): i32 = { child.read() }\n",
            true,
        ),
        unit(
            "child.sc",
            &["child"],
            "use answer\npub(package) let read(): i32 = { answer() }\n",
            false,
        ),
    ])
    .unwrap();
    assert!(matches!(
        function_tail(function(&program, "child::read")),
        Expr::Call(callee, _)
            if callee.as_ref() == &Expr::Name("answer".into())
    ));

    let bypass = resolve_sources(&[
        unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
        unit(
            "net.sc",
            &["net"],
            "pub(package) let value(): i32 = { 1 }\n",
            false,
        ),
        unit(
            "owner.sc",
            &["owner"],
            "use root.net as hidden\nlet local(): i32 = { hidden.value() }\n",
            false,
        ),
        unit(
            "sibling.sc",
            &["sibling"],
            "use root.owner.hidden.value as stolen\nlet read(): i32 = { stolen() }\n",
            false,
        ),
    ])
    .unwrap_err();
    assert!(bypass.iter().any(|diagnostic| {
        diagnostic.contains("private") && diagnostic.contains("owner.hidden")
    }));

    let unknown = resolve_sources(&[unit(
        "root.sc",
        &[],
        "use missing as value\nlet main(): i32 = { 0 }\n",
        true,
    )])
    .unwrap_err();
    assert_eq!(
        unknown
            .iter()
            .filter(|diagnostic| diagnostic.contains("unknown import"))
            .count(),
        1,
        "{unknown:?}"
    );
}

#[test]
fn resolves_dependency_packages_with_independent_roots() {
    let program = resolve_packages(&[
        package(
            0,
            true,
            &[("math", 1)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "use math.answer as selected\nlet main(): i32 = { selected() }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[],
            vec![
                unit("math/src/lib.sc", &[], "pub use root.inner.answer\n", true),
                unit(
                    "math/src/inner.sc",
                    &["inner"],
                    "pub let answer(): i32 = { 42 }\n",
                    false,
                ),
            ],
        ),
    ])
    .unwrap();

    let answer = format!("{}::inner::answer", stable_package_root("package-1@0.0.0"));
    assert!(function(&program, &answer).body.is_some());
    assert!(matches!(
        function_tail(function(&program, "main")),
        Expr::Call(callee, _)
            if callee.as_ref() == &Expr::Name(answer)
    ));
}

#[test]
fn enforces_package_visibility_across_dependency_boundaries() {
    for (visibility, expected) in [("pub(package)", "pub(package)"), ("", "private")] {
        let error = resolve_packages(&[
            package(
                0,
                true,
                &[("dep", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "let main(): i32 = { dep.hidden() }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[],
                vec![unit(
                    "dep/src/lib.sc",
                    &[],
                    &format!("{visibility} let hidden(): i32 = {{ 1 }}\n"),
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(
            error.iter().any(|diagnostic| diagnostic.contains(expected)),
            "{error:?}"
        );
    }
}

#[test]
fn prevents_dependency_anchors_from_escaping_their_package() {
    let root_lookup = resolve_packages(&[
        package(
            0,
            true,
            &[("dep", 1)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "pub let root_value(): i32 = { 1 }\nlet main(): i32 = { 0 }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[],
            vec![unit(
                "dep/src/lib.sc",
                &[],
                "use root.root_value\npub let answer(): i32 = { root_value() }\n",
                true,
            )],
        ),
    ])
    .unwrap_err();
    assert!(
        root_lookup.iter().any(|diagnostic| {
            diagnostic.contains("unknown import") && diagnostic.contains("root.root_value")
        }),
        "{root_lookup:?}"
    );

    let super_escape = resolve_packages(&[
        package(
            0,
            true,
            &[("dep", 1)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "let main(): i32 = { 0 }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[],
            vec![unit(
                "dep/src/lib.sc",
                &[],
                "use super.outside as value\npub let answer(): i32 = { 0 }\n",
                true,
            )],
        ),
    ])
    .unwrap_err();
    assert!(super_escape
        .iter()
        .any(|diagnostic| diagnostic.contains("escapes above the package root")));
}

#[test]
fn rejects_dependency_aliases_that_conflict_with_file_modules() {
    let error = resolve_packages(&[
        package(
            0,
            true,
            &[("dep", 1)],
            vec![
                unit("app/src/main.sc", &[], "let main(): i32 = { 0 }\n", true),
                unit(
                    "app/src/dep/internal.sc",
                    &["dep", "internal"],
                    "pub(package) let local = 1\n",
                    false,
                ),
            ],
        ),
        package(
            1,
            false,
            &[],
            vec![unit(
                "dep/src/lib.sc",
                &[],
                "pub let answer(): i32 = { 1 }\n",
                true,
            )],
        ),
    ])
    .unwrap_err();
    assert!(error
        .iter()
        .any(|diagnostic| diagnostic.contains("module `dep`")
            && diagnostic.contains("dependency alias")));
}

#[test]
fn hides_transitive_dependencies_until_the_owner_reexports_them() {
    let hidden = resolve_packages(&[
        package(
            0,
            true,
            &[("middle", 1)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "use middle.leaf.answer\nlet main(): i32 = { answer() }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[("leaf", 2)],
            vec![unit(
                "middle/src/lib.sc",
                &[],
                "pub let middle_value(): i32 = { leaf.answer() }\n",
                true,
            )],
        ),
        package(
            2,
            false,
            &[],
            vec![unit(
                "leaf/src/lib.sc",
                &[],
                "pub let answer(): i32 = { 42 }\n",
                true,
            )],
        ),
    ])
    .unwrap_err();
    assert!(hidden.iter().any(|diagnostic| {
        diagnostic.contains("unknown import") && diagnostic.contains("middle.leaf.answer")
    }));

    let exposed = resolve_packages(&[
        package(
            0,
            true,
            &[("middle", 1)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "use middle.answer as selected\nlet main(): i32 = { selected() }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[("leaf", 2)],
            vec![unit(
                "middle/src/lib.sc",
                &[],
                "pub use leaf.answer\n",
                true,
            )],
        ),
        package(
            2,
            false,
            &[],
            vec![unit(
                "leaf/src/lib.sc",
                &[],
                "pub let answer(): i32 = { 42 }\n",
                true,
            )],
        ),
    ])
    .unwrap();
    let answer = format!("{}::answer", stable_package_root("package-2@0.0.0"));
    assert!(matches!(
        function_tail(function(&exposed, "main")),
        Expr::Call(callee, _)
            if callee.as_ref() == &Expr::Name(answer)
    ));
}

#[test]
fn shared_dependencies_keep_one_definition_identity() {
    let program = resolve_packages(&[
        package(
            0,
            true,
            &[("left", 1), ("right", 2)],
            vec![unit(
                "app/src/main.sc",
                &[],
                "let same(value: left.token): right.token = { value }\nlet main(): i32 = { 0 }\n",
                true,
            )],
        ),
        package(
            1,
            false,
            &[("shared", 3)],
            vec![unit("left/src/lib.sc", &[], "pub use shared.token\n", true)],
        ),
        package(
            2,
            false,
            &[("shared", 3)],
            vec![unit(
                "right/src/lib.sc",
                &[],
                "pub use shared.token\n",
                true,
            )],
        ),
        package(
            3,
            false,
            &[],
            vec![unit(
                "shared/src/lib.sc",
                &[],
                "pub let token = struct { value: i32 }\n",
                true,
            )],
        ),
    ])
    .unwrap();

    let same = function(&program, "same");
    let token = format!("{}::token", stable_package_root("package-3@0.0.0"));
    assert_eq!(same.groups[0][0].ty, Type::Named(token.clone(), vec![]));
    assert_eq!(same.return_type, Some(Type::Named(token.clone(), vec![])));
    assert_eq!(
        program
            .items
            .iter()
            .filter(|item| matches!(item, Item::Struct(definition) if definition.name == token))
            .count(),
        1
    );
}

#[test]
fn dependency_aliases_conflict_with_root_declarations_and_imports() {
    for source in [
        "let dep = 1\nlet main(): i32 = { 0 }\n",
        "use root.answer as dep\nlet answer(): i32 = { 1 }\nlet main(): i32 = { 0 }\n",
    ] {
        let error = resolve_packages(&[
            package(
                0,
                true,
                &[("dep", 1)],
                vec![unit("app/src/main.sc", &[], source, true)],
            ),
            package(
                1,
                false,
                &[],
                vec![unit(
                    "dep/src/lib.sc",
                    &[],
                    "pub let answer(): i32 = { 1 }\n",
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(error
            .iter()
            .any(|diagnostic| diagnostic.contains("dependency alias `dep`")));
    }
}

#[test]
fn validates_public_package_graph_inputs() {
    let cycle = resolve_packages(&[
        package(
            0,
            true,
            &[("next", 1)],
            vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
        ),
        package(
            1,
            false,
            &[("back", 0)],
            vec![unit("next.sc", &[], "pub let value = 1\n", true)],
        ),
    ])
    .unwrap_err();
    assert!(cycle
        .iter()
        .any(|diagnostic| diagnostic.contains("cyclic package dependencies")));

    let invalid = resolve_packages(&[
        package(
            0,
            true,
            &[("missing", 99)],
            vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
        ),
        package(
            1,
            false,
            &[],
            vec![unit("orphan.sc", &[], "pub let value = 1\n", true)],
        ),
    ])
    .unwrap_err();
    assert!(invalid
        .iter()
        .any(|diagnostic| diagnostic.contains("unknown package ID 99")));
    assert!(invalid
        .iter()
        .any(|diagnostic| diagnostic.contains("not reachable")));

    let reserved = resolve_packages(&[package(
        PackageId::CORE.0,
        true,
        &[],
        vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
    )])
    .unwrap_err();
    assert!(reserved
        .iter()
        .any(|diagnostic| diagnostic.contains(&format!(
            "package ID {} is reserved for compiler core",
            PackageId::CORE.0
        ))));

    let reserved_alloc = resolve_packages(&[package(
        PackageId::ALLOC.0,
        true,
        &[],
        vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
    )])
    .unwrap_err();
    assert!(reserved_alloc
        .iter()
        .any(|diagnostic| diagnostic.contains(&format!(
            "package ID {} is reserved for compiler alloc",
            PackageId::ALLOC.0
        ))));
}

#[test]
fn rejects_nominal_types_that_are_narrower_than_function_and_global_apis() {
    let errors = resolve_sources(&[unit(
        "src/lib.sc",
        &[],
        "let hidden = struct {}\n\
             pub let wrapper(comptime t: type) = struct {}\n\
             pub let expose(value: wrapper(hidden)): hidden = { value }\n\
             pub let shared: hidden = hidden {}\n",
        true,
    )])
    .unwrap_err();

    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `expose` parameter `value`")
            && diagnostic.contains("exposes private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `expose` return type")
            && diagnostic.contains("exposes private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("global `shared` type")
            && diagnostic.contains("exposes private type `hidden`")
    }));
}

#[test]
fn canonicalizes_qualified_custom_effects_across_modules() {
    let program = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "pub let screen(): i32 with(ui.ui) = { 0 }\n",
            true,
        ),
        unit("src/ui.sc", &["ui"], "pub let ui = effect\n", false),
    ])
    .unwrap();

    assert_eq!(
        function(&program, "screen").effects.custom,
        [Type::Named("ui::ui".into(), Vec::new())]
    );
}

#[test]
fn rejects_private_effects_exposed_by_public_callable_apis() {
    let errors = resolve_sources(&[unit(
        "src/lib.sc",
        &[],
        "let ui = effect\n\
             pub let expose(action: (): i32 with(ui)): i32 with(ui) = { 0 }\n",
        true,
    )])
    .unwrap_err();

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `expose` parameter `action`")
            && diagnostic.contains("exposes private effect `ui`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `expose` with public visibility")
            && diagnostic.contains("exposes private effect `ui`")
    }));
}

#[test]
fn rejects_traits_that_are_narrower_than_public_where_predicates() {
    let errors = resolve_sources(&[unit(
        "src/lib.sc",
        &[],
        "let hidden = trait {}\n\
             pub let expose(comptime t: type)(value: t): t where t: hidden = { value }\n",
        true,
    )])
    .unwrap_err();

    assert!(
        errors.iter().any(|diagnostic| {
            diagnostic.contains("where predicate 1 trait")
                && diagnostic.contains("exposes private type `hidden`")
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_traits_that_are_narrower_than_constrained_extension_members() {
    let errors = resolve_sources(&[unit(
        "src/lib.sc",
        &[],
        "let hidden = trait {}\n\
             pub let cell(comptime t: type) = struct { pub value: t }\n\
             extend(cell(t)) where t: hidden {\n\
               let take(move self)(): t = { self.value }\n\
             }\n",
        true,
    )])
    .unwrap_err();

    assert!(
        errors.iter().any(|diagnostic| {
            diagnostic.contains("extension where predicate 1 trait")
                && diagnostic.contains("exposes private type `hidden`")
        }),
        "{errors:?}"
    );
}

#[test]
fn compares_package_and_private_api_audiences_by_module_ancestry() {
    let errors = resolve_sources(&[
        unit(
            "src/main.sc",
            &[],
            "let root_secret = struct {}\n\
                 pub(package) let package_secret = struct {}\n\
                 let main(): i32 = { 0 }\n",
            true,
        ),
        unit(
            "src/child.sc",
            &["child"],
            "let child_secret = struct {}\n\
                 pub(package) let package_ok(value: root_secret): root_secret = { value }\n\
                 let private_ok(value: root_secret): root_secret = { value }\n\
                 pub(package) let package_bad(value: child_secret): i32 = { 0 }\n\
                 pub let public_bad(value: package_secret): i32 = { 0 }\n",
            false,
        ),
    ])
    .unwrap_err();

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `child::package_bad` parameter `value`")
            && diagnostic.contains("private type `child::child_secret`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("function `child::public_bad` parameter `value`")
            && diagnostic.contains("pub(package) type `package_secret`")
    }));
}

#[test]
fn validates_effective_struct_and_enum_field_audiences() {
    let errors = resolve_sources(&[unit(
        "src/lib.sc",
        &[],
        "let hidden = struct {}\n\
             pub let record = struct { pub visible: hidden, private: hidden }\n\
             pub let choice = enum {\n\
               positional(hidden),\n\
               named(pub visible: hidden, private: hidden),\n\
             }\n",
        true,
    )])
    .unwrap_err();

    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("field `record.visible`")
            && diagnostic.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("enum variant `choice.positional` payload 0")
            && diagnostic.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("enum variant field `choice.named.visible`")
            && diagnostic.contains("private type `hidden`")
    }));
    assert!(errors
        .iter()
        .all(|diagnostic| !diagnostic.contains("record.private")
            && !diagnostic.contains("choice.named.private")));
}

#[test]
fn validates_trait_signatures_without_treating_bound_types_as_nominals() {
    let valid = resolve_sources(&[unit(
        "src/valid.sc",
        &[],
        "pub let convert(comptime t: type) = trait {\n\
               let output: type = t\n\
               let convert(comptime u: type)(self: borrow(self))(value: t): output\n\
             }\n",
        true,
    )]);
    assert!(valid.is_ok(), "{valid:?}");

    let errors = resolve_sources(&[unit(
        "src/invalid.sc",
        &[],
        "let hidden = struct {}\n\
             pub let expose = trait {\n\
               let output: type = hidden\n\
               let convert(self: borrow(self))(value: hidden): hidden\n\
             }\n",
        true,
    )])
    .unwrap_err();

    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("associated type `expose.output` default")
            && diagnostic.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("trait method `expose.convert` parameter `value`")
            && diagnostic.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.contains("trait method `expose.convert` return type")
            && diagnostic.contains("private type `hidden`")
    }));
}

#[test]
fn standard_library_modules_are_explicit_reserved_namespaces() {
    let program = resolve_sources(&[unit(
        "main.sc",
        &[],
        "use alloc.boxed.box as heap_box\nuse alloc.vec.vec\n\
             let keep(move boxed: heap_box(i32)): heap_box(i32) = { boxed }\n\
             let empty(): vec(i32) = { vec(i32).new() }\n",
        true,
    )])
    .unwrap();
    assert_eq!(
        function(&program, "keep").groups[0][0].ty,
        Type::Named("alloc::boxed::box".into(), vec![Type::I32])
    );
    assert_eq!(
        function(&program, "empty").return_type,
        Some(Type::Named("alloc::vec::vec".into(), vec![Type::I32]))
    );

    let operator = resolve_sources(&[unit(
        "operator.sc",
        &[],
        "use core.ops.add as plus\n\
             let number = struct { value: i32 }\n\
             extend(number, plus(number)) {\n\
               let output = number\n\
               let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }\n\
             }\n",
        true,
    )])
    .unwrap();
    assert!(operator.items.iter().any(|item| {
        matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::ops::arith::add"))
    }));

    let flow = resolve_sources(&[unit(
        "flow.sc",
        &[],
        "let chain = core.flow.chain\n\
             let ops_coalesce = core.ops.coalesce\n\
             let legacy_coalesce = core.ops.coalesce\n\
             let maybe(comptime t: type) = enum { some(t), none }\n\
             let legacy_maybe(comptime t: type) = enum { some(t), none }\n\
             extend(maybe(t), chain) {}\n\
             extend(maybe(t), ops_coalesce) {}\n\
             extend(legacy_maybe(t), legacy_coalesce) {}\n",
        true,
    )])
    .unwrap();
    assert!(flow.items.iter().any(|item| {
        matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::flow::chain"))
    }));
    assert!(flow.items.iter().any(|item| {
        matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::flow::coalesce"))
    }));

    let standard_modules = resolve_sources(&[unit(
            "standard.sc",
            &[],
            "use core.async.async\n\
             let semigroup = std.algebra.semigroup
             let monoid = std.algebra.monoid
             let number = struct { value: i32 }\n\
             let suspended(): i32 with(async) = { 0 }\n\
             let invoke(move action: (): i32 with(async)): i32 with(async) = { action() }\n\
             extend(number, semigroup) {\n\
               let combine(move left: number, move right: number): number = { number { value: left.value + right.value } }\n}\n\
             extend(number, monoid) {\n\
               let empty(): number = { number { value: 0 } }\n}\n",
            true,
        )])
        .unwrap();
    assert_eq!(
        function(&standard_modules, "suspended").effects.custom,
        vec![Type::Named("core::async::async".into(), Vec::new())]
    );
    let invoke = function(&standard_modules, "invoke");
    let Type::Function { effects, .. } = &invoke.groups[0][0].ty else {
        panic!("expected function-typed action parameter");
    };
    assert_eq!(
        effects.custom,
        vec![Type::Named("core::async::async".into(), Vec::new())]
    );
    assert!(standard_modules.items.iter().any(|item| {
        matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "std::algebra::semigroup"))
    }));
    assert!(standard_modules.items.iter().any(|item| {
        matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "std::algebra::monoid"))
    }));

    let bare = resolve_sources(&[unit(
        "main.sc",
        &[],
        "let make(): box(i32) = { box.new(1) }\n",
        true,
    )])
    .unwrap_err();
    assert!(bare.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `box` is not in the prelude")
            && diagnostic.contains("let box = alloc.boxed.box")
    }));

    let bare_option = resolve_sources(&[unit(
        "option.sc",
        &[],
        "let maybe(): option(i32) = { option.none }\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_option.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `option` is not in the prelude")
            && diagnostic.contains("let option = core.option")
    }));

    let bare_result = resolve_sources(&[unit(
        "result.sc",
        &[],
        "let outcome(): result(bool)(i32) = { result.ok(1) }\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_result.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `result` is not in the prelude")
            && diagnostic.contains("let result = core.result")
    }));

    let bare_operator = resolve_sources(&[unit(
        "operator.sc",
        &[],
        "let number = struct { value: i32 }\n\
             extend(number, add(number)) {\n\
               let output = number\n\
               let add(self)(rhs: number): number = { self }\n\
             }\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_operator.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `add` is not in the prelude")
            && diagnostic.contains("let add = core.ops.add")
    }));

    let bare_flow = resolve_sources(&[unit(
        "flow.sc",
        &[],
        "let maybe(comptime t: type) = enum { some(t), none }\n\
             extend(maybe(t), chain) {}\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_flow.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `chain` is not in the prelude")
            && diagnostic.contains("let chain = core.flow.chain")
    }));

    let bare_effect = resolve_sources(&[unit(
        "effect.sc",
        &[],
        "let suspended(): i32 with(async) = { 0 }\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_effect.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `async` is not in the prelude")
            && diagnostic.contains("let async = core.async.async")
    }));

    let bare_algebra = resolve_sources(&[unit(
        "algebra.sc",
        &[],
        "let number = struct { value: i32 }\n\
             extend(number, semigroup) {\n\
               let combine(move left: number, move right: number): number = { left }\n}\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_algebra.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `semigroup` is not in the prelude")
            && diagnostic.contains("let semigroup = std.algebra.semigroup")
    }));

    let bare_functional = resolve_sources(&[unit(
        "functional.sc",
        &[],
        "let number = struct { value: i32 }\n\
             extend(number, functor) {}\n",
        true,
    )])
    .unwrap_err();
    assert!(bare_functional.iter().any(|diagnostic| {
        diagnostic.contains("standard-library item `functor` is not in the prelude")
            && diagnostic.contains("let functor = std.functional.functor")
    }));

    for namespace in ["core", "alloc", "std"] {
        let module = resolve_sources(&[
            unit("main.sc", &[], "let main(): i32 = { 0 }\n", true),
            unit(
                &format!("{namespace}.sc"),
                &[namespace],
                "let value = 1\n",
                false,
            ),
        ])
        .unwrap_err();
        assert!(module
            .iter()
            .any(|diagnostic| diagnostic.contains("standard-library namespace")));
    }

    for namespace in ["core", "alloc", "std"] {
        let dependency = resolve_packages(&[
            package(
                0,
                true,
                &[(namespace, 1)],
                vec![unit("main.sc", &[], "let main(): i32 = { 0 }\n", true)],
            ),
            package(
                1,
                false,
                &[],
                vec![unit("dep.sc", &[], "pub let value = 1\n", true)],
            ),
        ])
        .unwrap_err();
        assert!(dependency
            .iter()
            .any(|diagnostic| { diagnostic.contains(&format!("dependency alias `{namespace}`")) }));
    }
}

#[test]
fn mounted_standard_exports_match_the_validated_bundles() {
    let core = crate::core::CoreBundle::for_edition(crate::manifest::Edition::Edition2026).unwrap();
    let core_exports = core
        .program()
        .items
        .iter()
        .zip(&core.program().item_visibilities)
        .zip(&core.program().item_origins)
        .filter(|((_, visibility), _)| **visibility == Visibility::Public)
        .filter_map(|((item, _), origin)| {
            Some((
                origin.module_path.last()?.as_str(),
                declaration_name(item)?.rsplit("::").next()?,
            ))
        })
        .collect::<BTreeSet<_>>();
    let expected_core = CORE_NEVER_EXPORTS
        .iter()
        .map(|name| ("never", *name))
        .chain([("@core", "test")])
        .chain(CORE_MARKER_EXPORTS.iter().map(|name| ("marker", *name)))
        .chain(
            CORE_PRIMITIVE_EXPORTS
                .iter()
                .map(|name| ("primitives", *name)),
        )
        .chain([("option", "option")])
        .chain(CORE_RESULT_EXPORTS.iter().map(|name| ("result", *name)))
        .chain(CORE_ERROR_EXPORTS.iter().map(|name| ("error", *name)))
        .chain(CORE_CMP_EXPORTS.iter().map(|name| ("cmp", *name)))
        .chain(CORE_FLOW_EXPORTS.iter().map(|name| ("flow", *name)))
        .chain(CORE_ARITH_EXPORTS.iter().map(|name| ("arith", *name)))
        .chain(CORE_BIT_EXPORTS.iter().map(|name| ("bit", *name)))
        .chain(CORE_ASSIGN_EXPORTS.iter().map(|name| ("assign", *name)))
        .chain(CORE_INDEX_EXPORTS.iter().map(|name| ("index", *name)))
        .chain(CORE_EFFECT_EXPORTS.iter().map(|name| ("effect", *name)))
        .chain(CORE_UNSAFE_EXPORTS.iter().map(|name| ("unsafe", *name)))
        .chain(CORE_ASYNC_EXPORTS.iter().map(|name| ("async", *name)))
        .chain(CORE_SORT_EXPORTS.iter().map(|name| ("sorts", *name)))
        .chain(CORE_STRING_EXPORTS.iter().map(|name| ("string", *name)))
        .chain(CORE_LITERAL_EXPORTS.iter().map(|name| ("literal", *name)))
        .chain(CORE_FOREIGN_EXPORTS.iter().map(|name| ("foreign", *name)))
        .chain(CORE_PASSING_EXPORTS.iter().map(|name| ("passing", *name)))
        .chain(
            CORE_BORROW_EXPORTS
                .iter()
                .filter(|name| !matches!(**name, "mut" | "shared"))
                .map(|name| ("borrow", *name)),
        )
        .chain(CORE_MEMORY_EXPORTS.iter().map(|name| ("memory", *name)))
        .chain(CORE_CONTROL_EXPORTS.iter().map(|name| ("control", *name)))
        .chain(CORE_ITER_EXPORTS.iter().map(|name| ("iter", *name)))
        .collect::<BTreeSet<_>>();
    assert_eq!(core_exports, expected_core);

    let alloc =
        crate::alloc::AllocBundle::for_edition(crate::manifest::Edition::Edition2026).unwrap();
    let alloc_exports = alloc
        .program()
        .items
        .iter()
        .zip(&alloc.program().item_visibilities)
        .zip(&alloc.program().item_origins)
        .filter(|((_, visibility), _)| **visibility == Visibility::Public)
        .map(|((item, _), origin)| {
            (
                origin
                    .module_path
                    .last()
                    .expect("alloc declaration has module provenance")
                    .as_str(),
                declaration_name(item)
                    .expect("public alloc item is named")
                    .rsplit("::")
                    .next()
                    .expect("declaration name is nonempty"),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected_alloc = ALLOC_EXPORTS.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(alloc_exports, expected_alloc);

    let mut symbols = SymbolTable::new();
    let mut module_paths = HashSet::new();
    let mut diagnostics = Vec::new();
    install_core_namespace(&mut symbols, &mut module_paths, &mut diagnostics, &[]);
    install_alloc_namespace(&mut symbols, &mut module_paths, &mut diagnostics, &[]);
    let std =
        crate::standard::StdBundle::for_edition(crate::manifest::Edition::Edition2026).unwrap();
    install_std_namespace(
        &mut symbols,
        &mut module_paths,
        &mut diagnostics,
        &[],
        std.exports(),
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(module_paths.contains(&vec!["std".to_owned()]));
    assert!(module_paths.contains(&vec!["std".to_owned(), "functional".to_owned()]));
    assert!(module_paths.contains(&vec!["std".to_owned(), "async".to_owned()]));
    assert!(!module_paths.contains(&vec!["std".to_owned(), "vec".to_owned()]));
    for export in std.exports() {
        let mut logical_path = vec!["std".to_owned()];
        logical_path.extend(
            export
                .module
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_owned),
        );
        logical_path.push(export.name.clone());
        let target_path = export.target.clone();
        assert_eq!(
            symbols[&logical_path].canonical,
            symbols[&target_path].canonical
        );
    }
}

fn expression_names(expression: Option<&Expr>) -> HashSet<String> {
    fn visit(expression: &Expr, names: &mut HashSet<String>) {
        match expression {
            Expr::Located { value, .. } => visit(value, names),
            Expr::Name(name) => {
                names.insert(name.clone());
            }
            Expr::Unary(_, value)
            | Expr::Try(value)
            | Expr::Throw(value)
            | Expr::Unsafe(value)
            | Expr::Borrow { value, .. }
            | Expr::ChainMember(value, _)
            | Expr::Loop { body: value } => visit(value, names),
            Expr::DoBlock { body } | Expr::Async { body } | Expr::Await(body) => visit(body, names),
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right)
            | Expr::CompoundAssign(left, _, right) => {
                visit(left, names);
                visit(right, names);
            }
            Expr::HandlerCoalesce {
                scrutinee,
                success,
                fallback,
                ..
            } => {
                visit(scrutinee, names);
                visit(success, names);
                visit(fallback, names);
            }
            Expr::HandlerChainCall(chain) => {
                visit(&chain.scrutinee, names);
                for argument in chain.groups.iter().flatten() {
                    visit(&argument.value, names);
                }
                visit(&chain.success, names);
                visit(&chain.residual, names);
            }
            Expr::Call(callee, arguments) => {
                visit(callee, names);
                for argument in arguments {
                    visit(&argument.value, names);
                }
            }
            Expr::StructLiteral {
                constructor,
                fields,
            } => {
                visit(constructor, names);
                for field in fields {
                    visit(&field.value, names);
                }
            }
            Expr::Member(base, _) => visit(base, names),
            Expr::Array(elements) | Expr::Tuple(elements) => {
                for element in elements {
                    visit(element, names);
                }
            }
            Expr::Index { base, index } => {
                visit(base, names);
                visit(index, names);
            }
            Expr::Block(statements, tail) => {
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => visit(&binding.value, names),
                        Stmt::Expr(expression) => visit(expression, names),
                    }
                }
                if let Some(tail) = tail {
                    visit(tail, names);
                }
            }
            Expr::Closure(_, body) => visit(body, names),
            Expr::PatternClosure { guard, body, .. } => {
                if let Some(guard) = guard {
                    visit(guard, names);
                }
                visit(body, names);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                visit(condition, names);
                visit(then_branch, names);
                if let Some(else_branch) = else_branch {
                    visit(else_branch, names);
                }
            }
            Expr::Return(value) | Expr::Break(value) => {
                if let Some(value) = value {
                    visit(value, names);
                }
            }
            Expr::While {
                condition, body, ..
            } => {
                visit(condition, names);
                visit(body, names);
            }
            Expr::Match { scrutinee, arms } => {
                visit(scrutinee, names);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        visit(guard, names);
                    }
                    visit(&arm.body, names);
                }
            }
            Expr::Type(_)
            | Expr::Unit
            | Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Continue => {}
        }
    }

    let mut names = HashSet::new();
    if let Some(expression) = expression {
        visit(expression, &mut names);
    }
    names
}
