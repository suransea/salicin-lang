use super::ctfe_value::CtfeValueKind;
use super::*;
use crate::ast::Param;
use crate::cleanup::{
    CleanupEdge, CleanupOp, CleanupPlan, LocalKind as CleanupLocalKind,
    LocalOwnership as CleanupLocalOwnership, MovePathId as CleanupMovePathId,
    Projection as CleanupProjection, ScopeKind as CleanupScopeKind,
    Terminator as CleanupTerminator, TransferKind,
};

fn resolve_text(source: &str) -> Program {
    crate::modules::resolve_sources(&[source_unit("<test>", &[], source, true)])
        .unwrap_or_else(|diagnostics| panic!("test source must resolve: {diagnostics:?}"))
}

fn source_unit(
    path: &str,
    module_path: &[&str],
    source: &str,
    is_root: bool,
) -> crate::modules::SourceUnit {
    crate::modules::SourceUnit {
        path: path.to_owned(),
        module_path: module_path
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect(),
        source: source.to_owned(),
        is_root,
    }
}

fn compile_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile(&program)
}

fn compile_unresolved_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = crate::parser::parse(source).expect("test source must parse");
    compile(&program)
}

fn compile_resolved_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile(&program)
}

fn compile_library_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile_library(&program)
}

fn compile_resolved_library_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile_library(&program)
}

fn cleanup_plan_text(source: &str, function_name: &str) -> CleanupPlan {
    let source = source.to_owned();
    let function_name = function_name.to_owned();
    std::thread::Builder::new()
        .name("cleanup-plan-test".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let program = resolve_text(&source);
            let mut analyzer = Analyzer::new(&program);
            let hir = match analyzer.analyze_target(false) {
                Some(hir) => hir,
                None => panic!("cleanup-plan source must lower: {:?}", analyzer.diagnostics),
            };
            assert!(
                analyzer.diagnostics.is_empty(),
                "unexpected diagnostics: {:?}",
                analyzer.diagnostics
            );
            let function = hir
                .functions
                .iter()
                .find(|function| function.name == function_name)
                .expect("requested hir function");
            HirCleanupPlanner::build(&hir, function).expect("cleanup plan must build and verify")
        })
        .expect("spawn cleanup plan test")
        .join()
        .expect("cleanup plan test thread")
}

fn compile_with_origins(source: &str, origins: Vec<ItemOrigin>) -> Result<String, Vec<Diagnostic>> {
    let mut program = crate::parser::parse(source).expect("test source must parse");
    assert_eq!(program.items.len(), origins.len());
    program.item_origins = origins;
    compile(&program)
}

fn compile_resolved_with_origins(
    source: &str,
    origins: Vec<ItemOrigin>,
) -> Result<String, Vec<Diagnostic>> {
    let mut program = resolve_text(source);
    assert_eq!(program.items.len(), origins.len());
    program.item_origins = origins;
    compile(&program)
}

fn origin(package: usize, module_path: &[&str]) -> ItemOrigin {
    ItemOrigin {
        package,
        module_path: module_path
            .iter()
            .map(|segment| (*segment).to_owned())
            .collect(),
        ..ItemOrigin::default()
    }
}

fn function(name: &str, groups: Vec<Vec<Param>>, result: Type, body: Expr) -> Item {
    Item::Function(Function {
        name: name.to_owned(),
        foreign: None,
        builtin: false,
        compile_groups: Vec::new(),
        groups,
        return_type: Some(result),
        effects: crate::ast::FunctionEffects::default(),
        where_predicates: Vec::new(),
        body: Some(body),
    })
}

fn param(name: &str, ty: Type) -> Param {
    Param {
        mode: PassMode::Inferred,
        access: None,
        modifiers: Vec::new(),
        region: None,
        name: name.to_owned(),
        ty,
    }
}

fn arg(value: Expr) -> CallArg {
    CallArg { label: None, value }
}

#[test]
fn monomorphizes_and_deduplicates_explicit_generic_function_calls() {
    let program = crate::parser::parse(
        "let identity(comptime t: type)(move value: t): t = { value }\n\
         let main(): i32 = { identity(i32)(40) + identity(i32)(2) }\n",
    )
    .expect("generic source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze();
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(hir.is_some());
    assert_eq!(
        analyzer
            .function_instances
            .values()
            .filter(|instance| instance.key.template == "identity")
            .count(),
        1
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == "identity")
        .expect("identity instance");
    assert_eq!(instance.key.arguments, vec![Ty::I32]);
    assert!(instance.canonical.starts_with("$mono$fn$"));
}

#[test]
fn inferred_and_explicit_type_arguments_share_instance_cache_keys() {
    let program = crate::parser::parse(
        "let identity(comptime t: type)(move value: t): t = { value }\n\
         let cell(comptime t: type) = struct { value: t }\n\
         let main(): i32 = {\n\
           let explicit = cell(i32) { value: identity(i32)(20) }\n\
           let inferred_value = identity(22)\n\
           let inferred = cell { value: inferred_value }\n\
           explicit.value + inferred.value\n\
         }\n",
    )
    .expect("mixed explicit and inferred source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze();
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(hir.is_some(), "mixed generic program hir");

    let function_instances: Vec<_> = analyzer
        .function_instances
        .values()
        .filter(|instance| instance.key.template == "identity")
        .collect();
    assert_eq!(function_instances.len(), 1);
    assert_eq!(function_instances[0].key.arguments, vec![Ty::I32]);

    let nominal_instances: Vec<_> = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "cell")
        .collect();
    assert_eq!(nominal_instances.len(), 1);
    assert_eq!(nominal_instances[0].key.arguments, vec![Ty::I32]);
}

#[test]
fn generic_inherent_extensions_materialize_members_per_nominal_instance() {
    let program = crate::parser::parse(
        "let cell(comptime t: type) = struct { value: t }\n\
         extend(cell(t)) {\n\
           let new(move value: t): cell(t) = { cell { value: value } }\n\
           let take(move self)(): t = { self.value }\n\
         }\n\
         let main(): i32 = { let cell = cell.new(42); cell.take() }\n",
    )
    .expect("generic inherent extension source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer
        .analyze()
        .expect("generic inherent extension must lower");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );

    let key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::I32],
    };
    let canonical = &analyzer.nominal_instance_names[&key];
    let members = &analyzer.inherent_members[canonical];
    assert!(members.functions.contains_key("new"));
    assert!(members.methods.contains_key("take"));
    assert!(hir
        .functions
        .iter()
        .any(|function| function.name == members.methods["take"]));
    assert!(analyzer
        .generic_inherent_functions
        .contains_key(&("cell".to_owned(), "new".to_owned())));
}

#[test]
fn generic_nominals_accept_compile_time_usize_arguments() {
    let source = "\
let buffer(comptime t: type)(comptime l: usize) = struct {
  values: array(t)(l),
}
extend(buffer(t)(l)) {
  let second(self: borrow(self))(): t = { self.values[1] }
}
let main(): i32 = {
  let buffer = buffer(i32)(2) { values: [40, 2] }
  buffer.second()
}
";
    compile_text(source).expect("generic nominal should accept a compile-time usize argument");
}

#[test]
fn nominal_fields_can_store_borrows_without_losing_the_loan() {
    let source = "\
let holder(comptime t: type) = struct { value: borrow(t) }
let hold(comptime t: type)(value: borrow(t)): holder(t) = {
  holder(t) { value: value }
}
let read(value: borrow(i32)): i32 = { value }
let main(): i32 = {
  let value = 42
  let holder = hold(i32)(value)
  read(holder.value)
}
";
    compile_text(source).expect("a nominal borrow field should preserve its source loan");
}

#[test]
fn nominal_borrow_fields_cannot_escape_a_local_source() {
    let errors = compile_text(
        "\
let holder(comptime t: type) = struct { value: borrow(t) }
let escape(): holder(i32) = {
  let value = 42
  holder(i32) { value: borrow(value) }
}
let main(): i32 = { 42 }
",
    )
    .expect_err("a stored borrow of a local must not escape");
    assert!(errors
        .iter()
        .any(|error| error.message.contains("cannot return")));
}

#[test]
fn nominal_borrow_fields_keep_the_source_borrowed() {
    let errors = compile_text(
        "\
let holder(comptime t: type) = struct { value: borrow(t) }
let hold(comptime t: type)(value: borrow(t)): holder(t) = {
  holder(t) { value: value }
}
let read(value: borrow(i32)): i32 = { value }
let main(): i32 = {
  let mut value = 41
  let holder = hold(i32)(value)
  value = 42
  read(holder.value)
}
",
    )
    .expect_err("a stored shared borrow must keep the source borrowed");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("already borrowed")),
        "{errors:?}"
    );
}

#[test]
fn inference_reifies_and_decomposes_generic_nominal_types() {
    let program = crate::parser::parse(
        "let cell(comptime t: type) = struct { value: t }\n\
         let unwrap(comptime t: type)(move value: cell(t)): t = { value.value }\n\
         let main(): i32 = {\n\
           let inner = cell(i32) { value: 42 }\n\
           let outer = cell { value: inner }\n\
           let nested = unwrap(outer.value)\n\
           let direct = unwrap(cell(i32) { value: 0 })\n\
           nested + direct\n\
         }\n",
    )
    .expect("nested inferred nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("nested inferred nominal hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );

    let inner_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::I32],
    };
    let inner = analyzer.nominal_instance_names[&inner_key].clone();
    let outer_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::Struct(inner)],
    };
    assert!(analyzer.nominal_instance_names.contains_key(&outer_key));

    let unwrap_instances: Vec<_> = analyzer
        .function_instances
        .values()
        .filter(|instance| instance.key.template == "unwrap")
        .collect();
    assert_eq!(unwrap_instances.len(), 1);
    assert_eq!(unwrap_instances[0].key.arguments, vec![Ty::I32]);
}

#[test]
fn integer_constraints_precede_defaulting_independent_of_source_order() {
    let program = crate::parser::parse(
        "let same(comptime t: type)(left: t, right: t): i32 = { 14 }\n\
         let accept(comptime t: type)(value: t): i32 = { 14 }\n\
         let main(): i32 = {\n\
           let wide: i64 = 7\n\
           same(0, wide) + same(wide, 0) + accept(0 + wide)\n\
         }\n",
    )
    .expect("ordered inference source must parse");
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("ordered inference hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    for template in ["same", "accept"] {
        let instances: Vec<_> = analyzer
            .function_instances
            .values()
            .filter(|instance| instance.key.template == template)
            .collect();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].key.arguments, vec![Ty::I64]);
    }
}

#[test]
fn explicit_generic_enum_values_are_available_to_outer_inference() {
    let source = "let maybe(comptime t: type) = enum { some(t), none }\n\
                  let identity(comptime t: type)(move value: t): t = { value }\n\
                  let main(): i32 = {\n\
                    let some = identity(maybe(i32).some(42))\n\
                    let none: maybe(i32) = identity(maybe(i32).none)\n\
                    some match { some(value) => value, none => 0 }\n\
                  }\n";
    compile_text(source).expect("outer inference over enum constructors must compile");
}

#[test]
fn inference_conflicts_do_not_materialize_instances() {
    let program = crate::parser::parse(
        "let identity(comptime t: type)(move value: t): t = { value }\n\
         let cell(comptime t: type) = struct { value: t }\n\
         let main(): bool = { identity(cell(i32) { value: 42 }) }\n",
    )
    .expect("conflicting inference source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(analyzer.analyze().is_none());
    assert!(analyzer.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting inference for type parameter `t`")
    }));
    assert!(analyzer
        .function_instances
        .values()
        .all(|instance| instance.key.template.contains("::")));
    assert!(analyzer
        .function_instance_names
        .keys()
        .all(|key| key.template.contains("::")));
    let baseline_nominals = [
        analyzer.lang_item_name(LangItemKind::Never),
        analyzer.lang_item_name(LangItemKind::PartialOrdering),
    ];
    assert!(analyzer
        .nominal_instances
        .values()
        .all(
            |instance| baseline_nominals.contains(&instance.key.template.as_str())
                || instance.key.template.contains("::")
        ));
    assert!(analyzer
        .nominal_instance_names
        .keys()
        .all(
            |key| baseline_nominals.contains(&key.template.as_str()) || key.template.contains("::")
        ));
}

#[test]
fn template_validation_rolls_back_temporary_instances_and_emits_closed_ir() {
    let program = crate::parser::parse(
        "let identity(comptime t: type)(move value: t): t = { value }\n\
         let wrap(comptime t: type)(move value: t): t = { identity(t)(value) }\n\
         let main(): i32 = { wrap(i32)(42) }\n",
    )
    .expect("generic composition must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(analyzer
        .function_instances
        .values()
        .all(|instance| instance.key.template.contains("::")));
    assert!(analyzer
        .function_instance_names
        .keys()
        .all(|key| key.template.contains("::")));
    assert!(analyzer
        .function_type_substitutions
        .keys()
        .all(|name| name.contains("::")));
    assert!(analyzer
        .function_order
        .iter()
        .all(|name| !name.contains("$generic$")));

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    let hir = analyzer.analyze().expect("closed program hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(
        analyzer
            .function_instances
            .values()
            .filter(|instance| matches!(instance.key.template.as_str(), "identity" | "wrap"))
            .count(),
        2
    );
    assert!(hir
        .functions
        .iter()
        .all(|function| !function.name.contains("$generic$")));
    assert!(analyzer.function_instances.values().all(|instance| {
        instance
            .key
            .arguments
            .iter()
            .all(|argument| match argument {
                Ty::Struct(name) | Ty::Enum(name) => !markers.contains(name),
                _ => true,
            })
    }));

    let ir = compile(&program).expect("closed generic composition must compile");
    for marker in markers {
        assert!(!ir.contains(&marker));
        assert!(!ir.contains(&hex_name(&marker)));
    }
}

#[test]
fn inferred_template_calls_roll_back_abstract_instances() {
    let program = crate::parser::parse(
        "let identity(comptime u: type)(move value: u): u = { value }\n\
         let wrap(comptime t: type)(move value: t): t = { identity(value) }\n\
         let main(): i32 = { wrap(i32)(42) }\n",
    )
    .expect("inferred generic composition must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(analyzer
        .function_instances
        .values()
        .all(|instance| instance.key.template.contains("::")));
    assert!(analyzer
        .function_instance_names
        .keys()
        .all(|key| key.template.contains("::")));
    assert!(analyzer
        .function_type_substitutions
        .keys()
        .all(|name| name.contains("::")));

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    let hir = analyzer.analyze().expect("closed inferred generic hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(
        analyzer
            .function_instances
            .values()
            .filter(|instance| matches!(instance.key.template.as_str(), "identity" | "wrap"))
            .count(),
        2
    );
    assert!(hir
        .functions
        .iter()
        .all(|function| !function.name.contains("$generic$")));
    assert!(analyzer.function_instances.values().all(|instance| {
        instance
            .key
            .arguments
            .iter()
            .all(|argument| match argument {
                Ty::Struct(name) | Ty::Enum(name) => !markers.contains(name),
                _ => true,
            })
    }));

    let ir = compile(&program).expect("closed inferred composition must compile");
    for marker in markers {
        assert!(!ir.contains(&marker));
        assert!(!ir.contains(&hex_name(&marker)));
    }
}

#[test]
fn registers_plain_nominals_and_deduplicates_generic_nominal_instances() {
    let program = crate::parser::parse(
        "let plain = struct { value: i32 }\n\
         let cell(comptime t: type) = struct { value: t }\n\
         let main(): i32 = { cell(i32) { value: plain { value: 40 }.value }.value + cell(i32) { value: 2 }.value }\n",
    )
    .expect("generic nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );

    let plain = analyzer
        .nominal_instances
        .get("plain")
        .expect("plain nominal metadata");
    assert_eq!(plain.canonical, "plain");
    assert_eq!(plain.key.kind, NominalKind::Struct);
    assert_eq!(plain.key.template, "plain");
    assert!(plain.key.arguments.is_empty());
    assert!(analyzer
        .nominal_instances
        .values()
        .all(|instance| instance.key.arguments.is_empty()
            || instance.key.template.contains("::")
            || instance.key.template.starts_with("core::")));

    let hir = analyzer.analyze().expect("generic nominal hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::I32],
    };
    let canonical = analyzer
        .nominal_instance_names
        .get(&key)
        .expect("cell(i32) canonical name");
    let instances: Vec<_> = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "cell")
        .collect();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].key, key);
    assert_eq!(instances[0].canonical, *canonical);
    assert!(canonical.starts_with("$mono$type$"));
    assert!(hir.structs.iter().any(|layout| layout.name == *canonical));
}

#[test]
fn allows_same_named_struct_and_constructor_function() {
    let ir = compile_unresolved_text(
        r#"
let pair = struct { left: i32, right: i32 }
let pair(left: i32, right: i32): pair = { pair { left: left, right: right } }
let main(): i32 = {
  let pair = pair(40, 2)
  pair.left + pair.right
}
"#,
    )
    .expect("same-named constructor function should compile");

    let constructor = function_symbol("pair");
    assert!(ir.contains(&format!(
        "define internal %{} @{constructor}(i32 %arg.0, i32 %arg.1)",
        type_symbol("pair")
    )));
    assert!(ir.contains(&format!("call %{} @{constructor}", type_symbol("pair"))));
}

#[test]
fn materializes_nested_generic_struct_layouts_in_dependency_order() {
    let program = crate::parser::parse(
        "let cell(comptime t: type) = struct { value: t }\n\
         let main(): i32 = { cell(cell(i32)) { value: cell(i32) { value: 42 } }.value.value }\n",
    )
    .expect("nested generic nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze();
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    let hir = hir.expect("nested generic nominal hir");

    let inner_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::I32],
    };
    let inner = analyzer.nominal_instance_names[&inner_key].clone();
    let outer_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::Struct(inner.clone())],
    };
    let outer = analyzer.nominal_instance_names[&outer_key].clone();
    assert_ne!(inner, outer);
    assert_eq!(
        analyzer.struct_layouts[&outer].fields[0].ty,
        Ty::Struct(inner.clone())
    );
    let inner_index = hir
        .structs
        .iter()
        .position(|layout| layout.name == inner)
        .expect("inner layout order");
    let outer_index = hir
        .structs
        .iter()
        .position(|layout| layout.name == outer)
        .expect("outer layout order");
    assert!(inner_index < outer_index);
}

#[test]
fn lowers_generic_enum_type_heads_unit_variants_and_short_patterns() {
    let ir = compile_unresolved_text(
        "let maybe(comptime t: type) = enum {\n\
           some(t),\n\
           none,\n\
         }\n\
         let choose(flag: bool): maybe(i32) = { if flag {\n\
           maybe(i32).some(42)\n\
         } else {\n\
           maybe(i32).none\n\
         } }\n\
         let unwrap(move value: maybe(i32)): i32 = { value match {\n\
           some(item) => item,\n\
           none => 0,\n\
         } }\n\
         let main(): i32 = { unwrap(choose(false)) }\n",
    )
    .expect("generic enum program must compile");
    let key = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "maybe".into(),
        arguments: vec![Ty::I32],
    };
    let canonical = nominal_instance_name(&key);
    assert!(ir.contains(&hex_name(&canonical)));
}

#[test]
fn registers_source_backed_core_lang_items() {
    let analyzer = Analyzer::new(&Program::new(Vec::new()));
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected prelude diagnostics: {:?}",
        analyzer.diagnostics
    );
    for kind in [
        LangItemKind::Builtin,
        LangItemKind::Foreign,
        LangItemKind::Test,
        LangItemKind::CopyParameters,
        LangItemKind::MoveParameters,
        LangItemKind::BreakEffect,
        LangItemKind::ContinueEffect,
        LangItemKind::ReturnEffect,
        LangItemKind::Break,
        LangItemKind::BreakUnit,
        LangItemKind::Continue,
        LangItemKind::Return,
        LangItemKind::ReturnUnit,
        LangItemKind::Defer,
    ] {
        assert!(!analyzer.lang_item_name(kind).is_empty());
    }
    for kind in [
        LangItemKind::Builtin,
        LangItemKind::Foreign,
        LangItemKind::Test,
        LangItemKind::CopyParameters,
        LangItemKind::MoveParameters,
    ] {
        assert!(!analyzer
            .function_templates
            .contains_key(analyzer.lang_item_name(kind)));
    }
    let async_function =
        &analyzer.function_templates[analyzer.lang_item_name(LangItemKind::AsyncFunction)];
    let await_function =
        &analyzer.function_templates[analyzer.lang_item_name(LangItemKind::AwaitFunction)];
    assert!(async_function.builtin && async_function.body.is_none());
    assert!(!await_function.builtin && await_function.body.is_some());
    assert_eq!(
        &analyzer.enum_template_order[..2],
        &[
            "core::option::option".to_owned(),
            "core::result::result".to_owned()
        ]
    );

    let option = &analyzer.enum_templates["core::option::option"];
    assert_eq!(option.compile_groups.len(), 1);
    assert_eq!(option.compile_groups[0].len(), 1);
    assert_eq!(option.compile_groups[0][0].name, "t");
    assert_eq!(option.variants.len(), 2);
    assert_eq!(option.variants[0].name, "some");
    assert_eq!(
        option.variants[0].fields,
        VariantFields::Positional(vec![Type::Named("t".into(), Vec::new())])
    );
    assert_eq!(option.variants[1].name, "none");
    assert_eq!(option.variants[1].fields, VariantFields::Unit);

    let result = &analyzer.enum_templates["core::result::result"];
    assert_eq!(result.compile_groups.len(), 2);
    assert_eq!(result.compile_groups[0].len(), 1);
    assert_eq!(result.compile_groups[0][0].name, "e");
    assert_eq!(result.compile_groups[1].len(), 1);
    assert_eq!(result.compile_groups[1][0].name, "t");
    assert_eq!(
        result
            .compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        vec!["e", "t"]
    );
    assert_eq!(result.variants.len(), 2);
    assert_eq!(result.variants[0].name, "ok");
    assert_eq!(result.variants[1].name, "err");

    let never_name = analyzer.lang_item_name(LangItemKind::Never);
    let never = &analyzer.enum_defs[never_name];
    assert!(never.compile_groups.is_empty());
    assert!(never.variants.is_empty());
    assert!(analyzer.enum_layouts[never_name].variants.is_empty());
    for operator_trait in BINARY_OPERATOR_TRAITS {
        let name = analyzer.lang_item_name(operator_trait.lang_item).to_owned();
        assert!(analyzer.traits[&name].valid);
        assert_eq!(
            analyzer.lang_item_name(operator_trait.lang_item),
            analyzer
                .lang_items
                .get(operator_trait.lang_item)
                .canonical_name()
        );
    }
    for operator_trait in UNARY_OPERATOR_TRAITS {
        let name = analyzer.lang_item_name(operator_trait.lang_item).to_owned();
        assert!(analyzer.traits[&name].valid);
        assert_eq!(
            analyzer.lang_item_name(operator_trait.lang_item),
            analyzer
                .lang_items
                .get(operator_trait.lang_item)
                .canonical_name()
        );
    }
    let throw_name = analyzer.lang_item_name(LangItemKind::Throw);
    let throw = &analyzer.function_templates[throw_name];
    assert_eq!(
        throw.compile_groups,
        vec![vec![CompileParam {
            name: "error".to_owned(),
            kind: Sort::Type,
            default: None,
        }]]
    );
    assert_eq!(
        throw.return_type,
        Some(Type::Named(never_name.to_owned(), Vec::new()))
    );
    assert_eq!(
        throw.effects.custom,
        vec![Type::Named(
            analyzer
                .lang_item_name(LangItemKind::ThrowsEffect)
                .to_owned(),
            vec![Type::Named("error".to_owned(), Vec::new())],
        )]
    );
    let unsafe_name = analyzer.lang_item_name(LangItemKind::Unsafe);
    assert!(
        analyzer.function_templates[unsafe_name].body.is_some(),
        "core.control.unsafe must remain source-backed"
    );
    assert!(analyzer.nominal_instances.len() >= 2);
    assert!(analyzer.nominal_instance_names.len() >= 2);
    let partial_ordering = analyzer.lang_item_name(LangItemKind::PartialOrdering);
    assert!(analyzer
        .nominal_instances
        .values()
        .any(|instance| instance.key.kind == NominalKind::Enum
            && instance.key.template == partial_ordering
            && instance.key.arguments.is_empty()));
    assert!(analyzer.functions.keys().all(|name| name.contains("::")));
    let boxed = |name: &str| format!("alloc::boxed::{name}");
    let vec = |name: &str| format!("alloc::vec::{name}");
    assert!(analyzer.function_templates.contains_key(&boxed("box_new")));
    assert!(analyzer
        .function_templates
        .contains_key(&boxed("box_into_raw")));
    assert!(analyzer.function_templates.contains_key(&boxed("box_read")));
    assert!(analyzer
        .function_templates
        .contains_key(&boxed("box_write")));
    assert!(analyzer
        .function_templates
        .contains_key(&boxed("box_into_inner")));
    assert!(analyzer
        .function_templates
        .contains_key(&boxed("box_replace")));
    assert!(analyzer
        .function_templates
        .contains_key(&boxed("box_as_ref")));
    assert!(!analyzer
        .function_templates
        .contains_key(&boxed("box_as_mut")));
    assert!(matches!(
        analyzer.function_templates[&boxed("box_as_ref")]
            .compile_groups
            .as_slice(),
        [group] if matches!(group.as_slice(), [access, ty]
            if access.name == "a" && access.kind.is_access()
                && ty.name == "t" && ty.kind == Sort::Type)
    ));
    assert!(analyzer.function_templates.contains_key(&vec("vec_new")));
    assert!(analyzer
        .function_templates
        .contains_key(&vec("vec_with_capacity")));
    assert!(analyzer.function_templates.contains_key(&vec("vec_at")));
    assert!(!analyzer.function_templates.contains_key(&vec("vec_at_mut")));
    assert!(matches!(
        analyzer.function_templates[&vec("vec_at")]
            .compile_groups
            .as_slice(),
        [group] if matches!(group.as_slice(), [access, ty]
            if access.name == "a" && access.kind.is_access()
                && ty.name == "t" && ty.kind == Sort::Type)
    ));
    for name in [
        "vec_reserve",
        "vec_push",
        "vec_replace",
        "vec_pop",
        "vec_truncate",
        "vec_clear",
        "vec_is_empty",
        "vec_swap_remove",
        "vec_swap",
        "vec_reverse",
        "vec_insert",
        "vec_remove",
        "vec_append",
    ] {
        assert!(analyzer.function_templates.contains_key(&vec(name)));
    }
    assert!(analyzer
        .function_templates
        .contains_key(&vec("vec_shrink_to_fit")));
    assert!(analyzer.function_templates.contains_key(&vec("vec_read")));
    assert!(analyzer.function_templates.contains_key(&vec("vec_write")));
    assert!(analyzer
        .generic_inherent_functions
        .contains_key(&("alloc::boxed::box".to_owned(), "new".to_owned())));
    assert!(analyzer
        .generic_inherent_functions
        .contains_key(&("alloc::vec::vec".to_owned(), "new".to_owned())));
    assert!(analyzer.generic_inherent_extensions["alloc::vec::vec"]
        .iter()
        .flat_map(|extension| &extension.members)
        .any(
            |member| matches!(member, ExtendMember::Function(function) if function.name == "push")
        ));
    assert!(analyzer
        .function_order
        .iter()
        .all(|name| name.contains("::")));
}

#[test]
fn constructs_infers_and_matches_core_option_and_result() {
    let ir = compile_resolved_text(
        r#"
let option = core.option
let result = core.result

let unwrap_option(move value: option(i32)): i32 = { value match {
  some(item) => item,
  none => 0,
} }
let unwrap_result(move value: result(bool)(i32)): i32 = { value match {
  ok(item) => item,
  err(_) => 0,
} }
let main(): i32 = {
  let some = option.some(19)
  let none: option(i32) = option.none
  let ok: result(bool)(i32) = result.ok(23)
  let err: result(bool)(i32) = result.err(false)
  unwrap_option(some) + unwrap_option(none) + unwrap_result(ok) + unwrap_result(err)
}
"#,
    )
    .expect("core option and result program must compile");
    let option = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::option::option".into(),
        arguments: vec![Ty::I32],
    };
    let result = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::result::result".into(),
        arguments: vec![Ty::Bool, Ty::I32],
    };
    assert!(ir.contains(&hex_name(&nominal_instance_name(&option))));
    assert!(ir.contains(&hex_name(&nominal_instance_name(&result))));
}

#[test]
fn empty_enums_are_uninhabited_and_never_supports_empty_match() {
    let ir = compile_library_text(
        r#"
let empty = enum {}
let from_never(move value: never): i32 = { value match {} }
let from_empty(move value: empty): bool = { value match {} }
let stop(): never = { loop {} }
let choose(flag: bool): i32 = { if flag { 42 } else { stop() } }
"#,
    )
    .expect("empty enums and empty matches must compile");

    assert!(ir.contains(&type_symbol("core::never::never")));
    assert!(ir.contains(&type_symbol("empty")));
    assert!(ir.contains("unreachable"));
    assert!(!ir.contains("define i32 @main()"));
}

#[test]
fn coalesce_lowers_option_and_result_through_lazy_match_control_flow() {
    let ir = compile_resolved_text(
        r#"
let option = core.option
let result = core.result

let make(count: borrow(mut)(i32)): option(i32) = {
  count = count + 1
  option(i32).some(20)
}
let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 10
  22
}
let main(): i32 = {
  let mut count = 0
  let option = make(count) ?? fallback(count)
  let result = result(bool)(i32).err(false) ?? option
  if count == 11 { result } else { 0 }
}
"#,
    )
    .expect("option and result coalescing must compile");

    let calls_to_make = ir
        .lines()
        .filter(|line| line.contains("call ") && line.contains(&function_symbol("make")))
        .count();
    assert_eq!(calls_to_make, 1, "left operand must be evaluated once");
    assert!(ir.matches("switch i32").count() >= 2);
    assert!(
        ir.contains(" phi "),
        "coalesce result must join through a phi"
    );
}

#[test]
fn throw_returns_the_enclosing_failure_error_variant() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let answer(fail: bool): i32 with(throwing(bool)) = {
  if fail { throw(true) }
  42
}
let main(): i32 = {
  let result: result(bool)(i32) = try { answer(true) }
  result ?? 42
}
"#,
    )
    .expect("throw(in) a failure function must compile");
    assert!(ir.contains("switch i32"));
    assert!(ir.contains("ret %sali.type."));
}

#[test]
fn failure_calls_propagate_automatically_and_try_handles_them() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let read(fail: bool): i32 with(throwing(bool)) = {
  if fail { throw(true) }
  40
}
let forward(fail: bool): i32 with(throwing(bool)) = { read(fail) + 2 }
let invoke(action: (bool): i32 with(throwing(bool)))(fail: bool): i32 with(throwing(bool)) = {
  action(fail) }
let main(): i32 = {
  let result: result(bool)(i32) = try { invoke(forward)(false) }
  result match {
ok(value) => value,
err(_) => 0
  }
}
"#,
    )
    .expect("failure calls should branch automatically and try should produce result");
    assert!(ir.contains("switch"));

    let unhandled = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let read(): i32 with(throwing(bool)) = { throw(true) }
let main(): i32 = { read() }
"#,
    )
    .expect_err("a pure caller must handle a failure effect");
    assert!(unhandled
        .iter()
        .any(|error| error.message.contains("handle it with `try { ... }`")));
}

#[test]
fn try_infers_a_unique_escaping_failure_source_without_context() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let fail(flag: bool): i32 with(throwing(bool)) = { if flag { throw(true) } else { 41 } }
let main(): i32 = {
  let action = fail
  let direct = try { fail(false) }
  let indirect = try { action(false) }
  (direct ?? 0) + (indirect ?? 0) - 40
}
"#,
    )
    .expect("try should infer result from a unique direct or indirect failure source");
    assert!(ir.contains("switch"));

    compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let failure = struct { code: i32 }
extend(failure, copyable) {}
extend(failure) {
  let raise(self: borrow(self))(): i32 with(throwing(bool)) = { throw(true) }
}
let main(): i32 = {
  let failure = failure { code: 1 }
  let result: result(bool)(i32) = try { failure.raise() }
  result ?? 42
}
"#,
    )
    .expect("contextual try should handle a complete throwing method call");

    let ambiguous = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let left(): i32 with(throwing(bool)) = { throw(true) }
let right(): i32 with(throwing(i64)) = { throw(1) }
let main(): i32 = {
  let result = try { if true { left() } else { right() } }
  result ?? 0
}
"#,
    )
    .unwrap_err();
    assert!(
        ambiguous.iter().any(|error| {
            error
                .message
                .contains("multiple escaping error types: `bool`, `i64`")
        }),
        "{ambiguous:?}"
    );

    let handled = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let fail(): i32 with(throwing(bool)) = { throw(true) }
let main(): i32 = {
  let inner = try { fail() }
  let outer = try { inner }
  outer match { ok(_) => 0, err(_) => 1 }
}
"#,
    )
    .unwrap_err();
    assert!(handled.iter().any(|error| {
        error
            .message
            .contains("body has no escaping failure source")
    }));
}

#[test]
fn named_arguments_select_top_level_function_overloads() {
    let ir = compile_text(
        r#"
let choose(left: i32): i32 = { left + 1 }
let choose(right: i32): i32 = { right + 2 }
let main(): i32 = { choose(right: 40) }
"#,
    )
    .expect("named parameters should select one overload");
    assert!(ir.contains(&format!(
        "call i32 @{}(i32 40)",
        function_symbol("choose$overload$1$right")
    )));

    let positional = compile_unresolved_text(
        r#"
let choose(left: i32): i32 = { left }
let choose(right: i32): i32 = { right }
let main(): i32 = { choose(42) }
"#,
    )
    .unwrap_err();
    assert!(positional
        .iter()
        .any(|error| error.message.contains("requires named arguments")));

    let duplicate = compile_unresolved_text(
        r#"
let choose(value: i32): i32 = { value }
let choose(value: bool): i32 = { 0 }
let main(): i32 = { choose(value: 42) }
"#,
    )
    .unwrap_err();
    assert!(duplicate
        .iter()
        .any(|error| error.message.contains("duplicate overload")));

    let partial = compile_unresolved_text(
        r#"
let choose(value: i32)(left: i32): i32 = { value + left }
let choose(value: i32)(right: i32): i32 = { value + right }
let main(): i32 = { choose(value: 40)(right: 2) }
"#,
    )
    .expect("later named parameter groups should disambiguate curried overloads");
    assert!(partial.contains(&function_symbol("choose$overload$1$value$$1$right")));

    compile_unresolved_text(
        r#"
let choose(left: i32): i32 = { left + 1 }
let choose(right: i32): i32 = { right + 2 }
let main(): i32 = {
  let deferred = { choose(left: 41) }
  deferred()
}
"#,
    )
    .expect("a local closure should retain named overload selection");

    compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let choose(fail: bool): i32 with(throwing(bool)) = { if fail { throw(true) } else { 42 } }
let choose(value: i32): i32 = { value }
let main(): i32 = {
  let result = try { choose(fail: false) }
  result ?? 0
}
"#,
    )
    .expect("a selected overload should preserve failure inference and lowering");

    compile_unresolved_text(
        r#"
let choose(comptime t: type)(left: t): t = { left }
let choose(comptime t: type)(right: t): t = { right }
let main(): i32 = { choose(left: 20) + choose(i32)(right: 22) }
"#,
    )
    .expect("generic overload selection should precede inferred or explicit type arguments");

    compile_unresolved_text(
        r#"
let choose(left: i32): i32 = { left }
let choose(comptime t: type)(right: t): t = { right }
let main(): i32 = { choose(left: 20) + choose(right: 22) }
"#,
    )
    .expect("concrete and generic functions may share one label-directed overload set");
}

#[test]
fn named_arguments_select_inherent_member_overloads() {
    let ir = compile_text(
        r#"
let counter = struct { value: i32 }
extend(counter, copyable) {}
extend(counter) {
  let add(self: borrow(self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(self))(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): counter = { counter { value: left } }
  let make(right: i32): counter = { counter { value: right + 1 } }
}
let main(): i32 = {
  let counter = counter.make(right: 19)
  counter.add(right: 21)
}
"#,
    )
    .expect("named parameters should select method and associated-function overloads");
    assert!(ir.contains(&function_symbol("counter::function::make$overload$1$right")));
    assert!(ir.contains(&function_symbol(
        "counter::method::add$overload$1$self$$1$right"
    )));

    let positional = compile_text(
        r#"
let counter = struct { value: i32 }
extend(counter) {
  let add(self: borrow(self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(self))(right: i32): i32 = { self.value + right }
}
let main(): i32 = { counter { value: 40 }.add(2) }
"#,
    )
    .unwrap_err();
    assert!(positional
        .iter()
        .any(|error| error.message.contains("requires named arguments")));

    compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let counter = struct { value: i32 }
extend(counter, copyable) {}
extend(counter) {
  let read(self: borrow(self))(fail: bool): i32 with(throwing(bool)) = {
if fail { throw(true) } else { self.value }
  }
  let read(self: borrow(self))(fallback: i32): i32 = { fallback }
}
let main(): i32 = {
  let counter = counter { value: 42 }
  let result: result(bool)(i32) = try { counter.read(fail: false) }
  result ?? 0
}
"#,
    )
    .expect("a selected method overload should preserve failure inference");

    compile_text(
        r#"
let counter = struct { value: i32 }
extend(counter) {
  let choose(comptime t: type)(left: t): t = { left }
  let choose(comptime t: type)(right: t): t = { right }
  let add(comptime t: type)(self: borrow(self))(left: t): t = { left }
  let add(comptime t: type)(self: borrow(self))(right: t): t = { right }
}
let main(): i32 = {
  counter.choose(left: 20) + counter { value: 0 }.add(i32)(right: 22)
}
"#,
    )
    .expect("generic inherent overloads should infer or explicitly consume compile arguments");

    compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(t)) {
  let choose(left: t): t = { left }
  let choose(right: t): t = { right }
  let add(self: borrow(self))(left: t): t = { left }
  let add(self: borrow(self))(right: t): t = { right }
}
let main(): i32 = {
  cell.choose(left: 20) + cell(i32) { value: 0 }.add(right: 22)
}
"#,
    )
    .expect("blanket generic inherent extensions should preserve overload sets per instance");
}

#[test]
fn named_arguments_select_trait_member_overloads() {
    compile_text(
        r#"
let select = trait {
  let pick(self: borrow(self))(left: i32): i32
  let pick(self: borrow(self))(right: i32): i32
  let make(left: i32): i32
  let make(right: i32): i32
}
let counter = struct { value: i32 }
extend(counter, select) {
  let pick(self: borrow(self))(left: i32): i32 = { self.value + left }
  let pick(self: borrow(self))(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): i32 = { left }
  let make(right: i32): i32 = { right + 1 }
}
let main(): i32 = { counter { value: 0 }.pick(right: 20) + counter.make(right: 20) }
"#,
    )
    .expect("named parameters should select trait method and associated-function overloads");

    let positional = compile_text(
        r#"
let select = trait {
  let pick(self: borrow(self))(left: i32): i32
  let pick(self: borrow(self))(right: i32): i32
}
let counter = struct { value: i32 }
extend(counter, select) {
  let pick(self: borrow(self))(left: i32): i32 = { self.value + left }
  let pick(self: borrow(self))(right: i32): i32 = { self.value + right }
}
let main(): i32 = { counter { value: 40 }.pick(2) }
"#,
    )
    .unwrap_err();
    assert!(
        positional
            .iter()
            .any(|error| error.message.contains("requires named arguments")),
        "{positional:?}"
    );

    compile_text(
        r#"
let select = trait {
  let pick(self: borrow(self))(left: i32): i32 = { left }
  let pick(self: borrow(self))(right: i32): i32 = { right + 1 }
}
let counter = struct { value: i32 }
extend(counter, select) {}
let select(comptime t: type)(value: borrow(t)): i32 where t: select = {
  value.pick(right: 41)
}
let main(): i32 = { select(counter { value: 0 }) }
"#,
    )
    .expect("default overloads should dispatch through an assumed where-bound implementation");

    compile_text(
        r#"
let select = trait {
  let pick(self: borrow(self))(left: i32): i32
  let pick(self: borrow(self))(right: i32): i32
}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), select) {
  let pick(self: borrow(self))(left: i32): i32 = { left }
  let pick(self: borrow(self))(right: i32): i32 = { right + 1 }
}
let main(): i32 = { cell(i32) { value: 0 }.pick(right: 41) }
"#,
    )
    .expect("blanket trait implementations should preserve overload identities");

    compile_text(
        r#"
let left = trait { let pick(self: borrow(self))(left: i32): i32 }
let right = trait { let pick(self: borrow(self))(right: i32): i32 }
let counter = struct { value: i32 }
extend(counter, left) {
  let pick(self: borrow(self))(left: i32): i32 = { self.value + left }
}
extend(counter, right) {
  let pick(self: borrow(self))(right: i32): i32 = { self.value + right + 1 }
}
let main(): i32 = { counter { value: 20 }.pick(right: 21) }
"#,
    )
    .expect("named arguments should disambiguate same-named methods from distinct traits");

    let duplicate = compile_text(
        r#"
let select = trait {
  let pick(self: borrow(self))(value: i32): i32
  let pick(self: borrow(self))(value: bool): i32
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(duplicate
        .iter()
        .any(|error| error.message.contains("duplicate trait method overload")));
}

#[test]
fn effect_parameters_infer_forward_and_explicitly_select_failure_rows() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let invoke(comptime e: effects)(action: (): i32 with(e))(): i32 with(e) = { action() }
let fail(): i32 with(throwing(bool)) = { throw(true) }
let forward(): i32 with(throwing(bool)) = { invoke(fail)() }
let explicit(): i32 with(throwing(bool)) = { invoke(throwing(bool))(fail)() }
let main(): i32 = {
  let inferred: result(bool)(i32) = try { forward() }
  let selected: result(bool)(i32) = try { explicit() }
  (inferred ?? 21) + (selected ?? 21)
}
"#,
    )
    .expect("effect parameters should carry failure rows through inference and forwarding");
    assert!(ir.contains("switch"));
}

#[test]
fn throw_requires_an_exact_active_failure_boundary() {
    for (source, expected) in [
        (
            "let fail(): i32 with(throwing(bool)) = { throw(0) }\nlet main(): i32 = { 0 }\n",
            "requires `throwing(i32)`",
        ),
        (
            "let fail(): option(i32) = { throw(false) }\nlet main(): i32 = { 0 }\n",
            "handle it with `try { ... }`",
        ),
        (
            "let fail(): i32 = { throw(false) }\nlet main(): i32 = { 0 }\n",
            "handle it with `try { ... }`",
        ),
        (
            "let fail() = { throw(false) }\nlet main(): i32 = { 0 }\n",
            "handle it with `try { ... }`",
        ),
    ] {
        let source = format!("use core.option\nuse core.result\nuse core.error.throwing\n{source}");
        let errors = compile_resolved_text(&source).expect_err("invalid throw(must) be rejected");
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "missing `{expected}` diagnostic in {errors:?}"
        );
    }
}

#[test]
fn coalesce_hir_keeps_the_fallback_call_in_the_residual_arm() {
    let program = resolve_text(
        r#"
let option = core.option

let fallback(): i32 = { 42 }
let main(): i32 = { option(i32).some(20) ?? fallback() }
"#,
    );
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("coalesce hir must lower");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main hir");
    let HirExprKind::Block(_, Some(tail)) = &main.body.kind else {
        panic!("function body must remain a block");
    };
    let HirExprKind::Match { arms, .. } = &tail.kind else {
        panic!("coalesce must lower to match hir");
    };
    assert_eq!(main.body.ty, Ty::I32);
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[0].body.kind, HirExprKind::Read { .. }));
    assert!(matches!(
        &arms[1].body.kind,
        HirExprKind::Call { function, .. } if function == "fallback"
    ));
}

#[test]
fn concrete_trait_implementation_methods_can_be_compile_time_generic() {
    let program = crate::parser::parse(
        r#"
let apply = trait {
  let apply(comptime t: type)(move self)(move value: t): t
}
let boxed = struct { value: i32 }
extend(boxed, apply) {
  let apply(comptime t: type)(move self)(move value: t): t = {
value
  }
}
let main(): i32 = { boxed { value: 40 }.apply(i32)(2) }
"#,
    )
    .expect("generic concrete trait method source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected generic concrete trait method diagnostics: {:?}",
        analyzer.diagnostics
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("boxed".into()),
        trait_ref: TraitRefKey {
            name: "apply".into(),
            arguments: Vec::new(),
        },
    };
    let canonical = trait_method_name(&key, "apply");
    assert!(analyzer.function_templates.contains_key(&canonical));
    let mut analyzed = Analyzer::new(&program);
    let lowered = analyzed.analyze();
    assert!(
        lowered.is_some() && analyzed.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzed.diagnostics
    );
    let instance = analyzed
        .function_instances
        .values()
        .find(|instance| instance.key.template == canonical)
        .expect("generic trait method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("generic concrete trait method must compile");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("call i32 @{symbol}(")));
}

#[test]
fn generic_trait_method_binders_are_alpha_equivalent() {
    compile_text(
        r#"
let choose = trait {
  let choose(comptime value: type)(self: borrow(self))(move value: value): value
}
let boxed(comptime t: type) = struct { value: t }
extend(boxed(i32), choose) {
  let choose(comptime result: type)(self: borrow(self))(move value: result): result = {
    value
  }
}
let main(): i32 = { boxed(i32) { value: 0 }.choose(i32)(42) }
"#,
    )
    .expect("method type binders should compare by position and sort");

    compile_text(
        r#"
let choose = trait {
  let choose(comptime value: type)(self: borrow(self))(move value: value): value
}
let boxed(comptime t: type) = struct { value: t }
extend(boxed(item), choose) {
  let choose(comptime result: type)(self: borrow(self))(move value: result): result = {
    value
  }
}
let main(): i32 = { boxed(i32) { value: 0 }.choose(i32)(42) }
"#,
    )
    .expect("blanket implementation method binders should be alpha-equivalent");
}

#[test]
fn generic_trait_method_effect_usize_and_where_binders_are_alpha_equivalent() {
    compile_text(
        r#"
let run = trait {
  let run(comptime e: effects)(self: borrow(self))(action: (): i32 with(e)): i32 with(e)
}
let runner = struct {}
extend(runner, run) {
  let run(comptime f: effects)(self: borrow(self))(action: (): i32 with(f)): i32 with(f) = {
    action()
  }
}
let main(): i32 = { runner {}.run(pure)({ 42 }) }
"#,
    )
    .expect("method effect binders should be alpha-equivalent");

    compile_text(
        r#"
let first = trait {
  let first(comptime n: usize)(self: borrow(self))(values: array(i32)(n)): i32
}
let picker = struct {}
extend(picker, first) {
  let first(comptime l: usize)(self: borrow(self))(values: array(i32)(l)): i32 = {
    42
  }
}
let main(): i32 = { picker {}.first(1)([42]) }
"#,
    )
    .expect("method usize binders should be alpha-equivalent");

    compile_resolved_text(
        r#"
let copy = core.copyable
let select = trait {
  let select(comptime value: type)(self: borrow(self))(move value: value): value
    where value: copyable
}
let selector = struct {}
extend(selector, select) {
  let select(comptime result: type)(self: borrow(self))(move value: result): result
    where result: copyable = { value }
}
let main(): i32 = { selector {}.select(i32)(42) }
"#,
    )
    .expect("method where predicates should alpha-normalize their binders");

    compile_text(
        r#"
let view = trait {
  let view(comptime a: access)(comptime r: region)
    (self: borrow(a)(r)(self))(): borrow(a)(r)(i32)
}
let cell = struct { value: i32 }
extend(cell, view) {
  let view(comptime b: access)(comptime s: region)
    (self: borrow(b)(s)(self))(): borrow(b)(s)(i32) = {
    borrow(b)(self.value)
  }
}
let read(value: borrow(i32)): i32 = { value }
let main(): i32 = {
  let cell = cell { value: 42 }
  let value = cell.view(shared)()
  read(value)
}
"#,
    )
    .expect("method access and region binders should be alpha-equivalent");

    compile_text(
        r#"
let identity = trait {
  let identity(comptime value: type)(self: borrow(self))(move value: value): value = {
    value
  }
}
let unit = struct {}
extend(unit, identity) {}
let main(): i32 = { unit {}.identity(i32)(42) }
"#,
    )
    .expect("default generic trait methods should instantiate through the selected implementation");
}

#[test]
fn trait_implementation_cannot_strengthen_generic_method_predicates() {
    let errors = compile_resolved_text(
        r#"
let copy = core.copyable
let select = trait {
  let select(comptime value: type)(self: borrow(self))(move value: value): value
}
let selector = struct {}
extend(selector, select) {
  let select(comptime result: type)(self: borrow(self))(move value: result): result
    where result: copyable = { value }
}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("an implementation must not strengthen a method contract");
    assert!(errors.iter().any(|error| {
        error.message.contains("select.select") && error.message.contains("signature mismatch")
    }));
}

#[test]
fn constructor_trait_method_binders_are_alpha_equivalent() {
    compile_text(
        r#"
let choose = trait(comptime self: (comptime item: type): type) {
  let choose(comptime value: type)
    (move self: self(value))
    (move value: value): self(value)
}
let boxed(comptime t: type) = struct { value: t }
extend(boxed, choose) {
  let choose(comptime result: type)
    (move self: boxed(result))
    (move value: result): boxed(result) = {
    boxed(result) { value: value }
  }
}
let main(): i32 = {
  boxed(i32) { value: 0 }.choose(i32)(42).value
}
"#,
    )
    .expect("constructor trait method binders should be alpha-equivalent");
}

#[test]
fn generic_trait_method_binders_cannot_capture_implementation_binders() {
    let errors = compile_text(
        r#"
let choose = trait {
  let choose(comptime value: type)(self: borrow(self))(move value: value): value
}
let boxed(comptime t: type) = struct { value: t }
extend(boxed(item), choose) {
  let choose(comptime item: type)(self: borrow(self))(move value: item): item = {
    value
  }
}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("a method binder must not capture an implementation binder");
    assert!(errors.iter().any(|error| {
        error.message.contains("choose.choose")
            && error.message.contains("redeclares")
            && error.message.contains("item")
    }));
}

#[test]
fn generic_trait_method_associated_equalities_follow_alpha_renaming() {
    compile_text(
        r#"
let has_item = trait {
  let item: type
}
let select = trait {
  let select(comptime value: type)(self: borrow(self))(move value: value): value
    where value: has_item(item = i32)
}
let wrapped = struct { value: i32 }
extend(wrapped, has_item) {
  let item = i32
}
let selector = struct {}
extend(selector, select) {
  let select(comptime result: type)(self: borrow(self))(move value: result): result
    where result: has_item(item = i32) = {
    value
  }
}
let main(): i32 = {
  selector {}.select(wrapped)(wrapped { value: 42 }).value
}
"#,
    )
    .expect("associated equalities should survive method binder alpha-normalization");
}

#[test]
fn generic_trait_method_binder_kinds_must_match() {
    let errors = compile_text(
        r#"
let choose = trait {
  let choose(comptime value: type)(self: borrow(self))(): i32
}
let selector = struct {}
extend(selector, choose) {
  let choose(comptime length: usize)(self: borrow(self))(): i32 = { 42 }
}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("method binder sorts are part of the trait contract");
    assert!(errors.iter().any(|error| {
        error.message.contains("choose.choose")
            && error.message.contains("compile-time parameter groups")
    }));
}

#[test]
fn noncapturing_closure_arguments_can_fill_function_parameters() {
    compile_text(
        r#"
let invoke(move action: (): i32): i32 = { action() }
let main(): i32 = { invoke({ 42 }) }
"#,
    )
    .expect("noncapturing closure argument must compile");
}

#[test]
fn capturing_closure_arguments_specialize_static_callees() {
    compile_text(
        r#"
let invoke(move action: (): i32): i32 = { action() }
let main(): i32 = {
  let captured = 42
  invoke({ captured })
}
"#,
    )
    .expect("capturing closure argument must specialize a static callee");
}

#[test]
fn capturing_callable_bridge_transfers_mutable_and_move_captures() {
    compile_text(
        r#"
let invoke(move action: (): i32): i32 = { action() }
let main(): i32 = {
  let mut value = 40
  invoke({ () ->
    value = value + 2;
    value
  })
  value
}
"#,
    )
    .expect("mutable captures must be borrowed for the specialized call");

    compile_text(
        r#"
let resource = struct { value: i32 }
let consume(move value: resource): i32 = { value.value }
let invoke(move action: (): i32): i32 = { action() }
let main(): i32 = {
  let resource = resource { value: 42 }
  invoke({ consume(resource) })
}
"#,
    )
    .expect("resource captures must move into the specialized call");
}

#[test]
fn capturing_callable_bridge_reuses_equivalent_specializations() {
    std::thread::Builder::new()
        .name("capturing-callable-bridge-test".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let program = resolve_text(
                r#"
let invoke(move action: (): i32): i32 = { action() }
let main(): i32 = {
  let first = 20
  let left = invoke({ first })
  let second = 22
  left + invoke({ second })
}
"#,
            );
            let mut analyzer = Analyzer::new(&program);
            assert!(analyzer.analyze().is_some());
            assert!(
                analyzer.diagnostics.is_empty(),
                "unexpected bridge diagnostics: {:?}",
                analyzer.diagnostics
            );
            assert_eq!(
                analyzer
                    .functions
                    .keys()
                    .filter(|name| name.contains("$callable$bridge$"))
                    .count(),
                1,
                "equivalent closure code and capture types must share one specialization"
            );
        })
        .expect("spawn capturing callable bridge test")
        .join()
        .expect("capturing callable bridge test thread");
}

#[test]
fn coalesce_operator_dispatches_through_core_trait_for_user_types() {
    let program = resolve_text(
        r#"
let coalesce = core.flow.coalesce

let choice = enum { present(i32), missing }

extend(choice, coalesce) {
  let item = i32
  let coalesce(comptime e: effects)
    (self)
    (fallback: (): i32 with(e)): i32 with(e) = {
self match {
  present(value) => value,
  missing => fallback(),
}
  }}

let main(): i32 = {
  let present = choice.present(10) ?? 1
  let fallback = 2
  let missing = choice.missing ?? fallback
  present + missing
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum("choice".into()),
        trait_ref: TraitRefKey {
            name: "core::flow::coalesce".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "coalesce");
    let mut analyzer = Analyzer::new(&program);
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected custom coalesce diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("custom coalesce method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("custom coalesce implementation must drive `??`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("call i32 @{symbol}(")));
}

#[test]
fn generic_associated_type_constructor_rebinds_chain_result() {
    let program = resolve_text(
        r#"
let chain = core.flow.chain

let boxed = struct { value: i32 }
let maybe(comptime t: type) = enum { some(t), none }

extend(maybe(boxed), chain) {
  let item = boxed
  let rebind = maybe
  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (boxed): u with(e)): maybe(u) with(e) = {
self match {
  some(value) => maybe(u).some(transform(value)),
  none => maybe(u).none,
}
  }}

let main(): i32 = {
  let value = maybe(boxed).some(boxed { value: 40 }).chain(pure, i32)({ (item: boxed) -> item.value + 2 })
  value match { some(answer) => answer, none => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "maybe".into(),
            arguments: vec![Ty::Struct("boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::flow::chain".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "chain");
    let mut analyzer = Analyzer::new(&program);
    let implementation = analyzer
        .trait_impls
        .get(&key)
        .expect("maybe(boxed) must implement chain");
    assert_eq!(
        implementation.associated_type_sources["rebind"],
        Type::Named("maybe".into(), Vec::new())
    );
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected custom chain diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("custom chain method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("direct custom chain call must compile");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn generic_associated_type_constructor_preserves_compile_parameter_sorts() {
    let program = resolve_text(
        r#"
let lend = trait {
  let item(comptime a: access)(comptime r: region): type
  let view(comptime a: access, comptime r: region)(self: borrow(a)(r)(self))(): item(a)(r)
}
let main(): i32 = { 0 }
"#,
    );
    let analyzer = Analyzer::new(&program);
    let parameters = &analyzer.traits["lend"].associated_type_parameters["item"];
    assert_eq!(parameters.len(), 2);
    assert_eq!(parameters[0].kind, Sort::Named("access".into()));
    assert_eq!(parameters[1].kind, Sort::Region);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected gat sort diagnostics: {:?}",
        analyzer.diagnostics
    );
}

#[test]
fn chain_operator_dispatches_through_core_trait_for_user_types() {
    let program = resolve_text(
        r#"
let chain = core.flow.chain

let boxed = struct { value: i32 }
let maybe(comptime t: type) = enum { some(t), none }

extend(boxed) {
  let plus(self)(amount: i32): i32 = { self.value + amount }
}

extend(maybe(boxed), chain) {
  let item = boxed
  let rebind = maybe
  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (boxed): u with(e)): maybe(u) with(e) = {
self match {
  some(value) => maybe(u).some(transform(value)),
  none => maybe(u).none,
}
  }}

let main(): i32 = {
  let amount = 2
  let value = maybe(boxed).some(boxed { value: 40 })?.plus(amount)
  value match { some(answer) => answer, none => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "maybe".into(),
            arguments: vec![Ty::Struct("boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::flow::chain".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "chain");
    let mut analyzer = Analyzer::new(&program);
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected custom `?.` diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("custom chain operator method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("custom chain implementation must drive `?.`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn generic_trait_implementation_can_rebind_chain_constructor() {
    let program = resolve_text(
        r#"
let chain = core.flow.chain

let boxed = struct { value: i32 }
let maybe(comptime t: type) = enum { some(t), none }

extend(maybe(t), chain) {
  let item = t
  let rebind = maybe
  let chain(comptime e: effects, comptime u: type)
    (self)
    (transform: (t): u with(e)): maybe(u) with(e) = {
self match {
  some(value) => maybe(u).some(transform(value)),
  none => maybe(u).none,
}
  }}

let main(): i32 = {
  let value = maybe(boxed).some(boxed { value: 42 })?.value
  value match { some(answer) => answer, none => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "maybe".into(),
            arguments: vec![Ty::Struct("boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::flow::chain".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "chain");
    let mut analyzer = Analyzer::new(&program);
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected generic custom `?.` diagnostics: {:?}",
        analyzer.diagnostics
    );
    let implementation = analyzer
        .trait_impls
        .get(&key)
        .expect("generic maybe(boxed) instance must implement chain");
    assert_eq!(
        implementation.associated_type_sources["rebind"],
        Type::Named("maybe".into(), Vec::new())
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("generic custom chain method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("generic custom chain implementation must drive `?.`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn coalesce_probe_participates_in_outer_inference_and_nests_right_associatively() {
    compile_resolved_text(
        r#"
let option = core.option
let result = core.result

let identity(comptime t: type)(move value: t): t = { value }
let boxed(comptime t: type) = struct { value: t }
let main(): i32 = {
  let first = option(i32).none
  let second = option(i32).some(42)
  let scalar = identity(option.none ?? 42)
  let boxed = identity(option.none ?? boxed(i32) { value: 42 })
  let wide = identity(result(t: i64).err(false) ?? 42)
  identity(first ?? second ?? 0) + scalar + boxed.value - 84
}
"#,
    )
    .expect("coalesce payload type must be visible to outer inference");
}

#[test]
fn coalesce_infers_empty_standard_variants_from_expected_or_rhs_payloads() {
    compile_resolved_text(
        r#"
let option = core.option
let result = core.result

let main(): i32 = {
  let inferred_option = option.none ?? 40
  let inferred_result = result(e: bool).err(false) ?? 2
  let option_value = option.none ?? 40
  let result_value = result(e: bool).err(false) ?? 1
  let fully_inferred_result = result.err(false) ?? 1
  let wide = result(t: i64).err(false) ?? 42
  let nested = option(i32).none ?? option.none ?? 1
  inferred_option + inferred_result + option_value + result_value + fully_inferred_result + nested - 43
}
"#,
    )
    .expect("coalesce payload evidence must resolve empty variants");
}

#[test]
fn coalesce_does_not_guess_an_unconstrained_result_error_type() {
    let errors =
        compile_resolved_text("use core.result\nlet main(): i32 = { result.ok(40) ?? 2 }\n")
            .unwrap_err();
    assert!(errors.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("cannot infer compile-time argument")
            && diagnostic.message.contains("`e`")
            && diagnostic.message.contains("sort `type`")
            && diagnostic.message.contains("`core::result::result`")
    }));
}

#[test]
fn coalesce_reports_non_containers_mismatched_fallbacks_and_moves() {
    let non_container = compile_text("let main(): i32 = { 40 ?? 2 }\n").unwrap_err();
    assert!(non_container.iter().any(|diagnostic| diagnostic.message
        == "operator `??` requires `option(t)` or `result(e)(t)` on the left, found `i32`"));

    let mismatch =
        compile_resolved_text("use core.option\nlet main(): i32 = { option(i32).none ?? true }\n")
            .unwrap_err();
    assert!(mismatch
        .iter()
        .any(|diagnostic| diagnostic.message.contains("type mismatch")));

    let moved = compile_resolved_text(
        r#"
let result = core.result

let main(): i32 = {
  let value = result(bool)(i32).ok(42)
  let answer = value ?? 0
  value match { ok(item) => item, err(_) => answer }
}
"#,
    )
    .unwrap_err();
    assert!(moved
        .iter()
        .any(|diagnostic| diagnostic.message.contains("moved")));
}

#[test]
fn coalesce_joins_a_fallback_only_move_as_possibly_moved() {
    let errors = compile_resolved_text(
        r#"
let option = core.option

let boxed = struct { value: i32 }
let consume(move value: boxed): i32 = { value.value }
let main(): i32 = {
  let spare = boxed { value: 41 }
  let choice = option(i32).some(1)
  let answer = choice ?? consume(spare)
  consume(spare) + answer
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.message == "use of possibly moved or uninitialized value"));
}

#[test]
fn keeps_core_nominal_instances_structurally_isolated() {
    let program = resolve_text(
        r#"
let option = core.option
let result = core.result

let main(): i32 = {
  let number = option.some(42)
  let flag = option.some(true)
  let first: result(bool)(i32) = result.ok(42)
  let second: result(i32)(bool) = result.err(0)
  let value = number match { some(item) => item, none => 0 }
  let enabled = flag match { some(item) => item, none => false }
  let left = first match { ok(item) => item, err(_) => 0 }
  let right = second match { ok(_) => 0, err(item) => item }
  if enabled { value + left - right - 42 } else { 0 }
}
"#,
    );
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("multiple prelude instances hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );

    let option_i32 = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::option::option".into(),
        arguments: vec![Ty::I32],
    };
    let option_bool = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::option::option".into(),
        arguments: vec![Ty::Bool],
    };
    let option_i32_name = analyzer.nominal_instance_names[&option_i32].clone();
    let option_bool_name = analyzer.nominal_instance_names[&option_bool].clone();
    assert_ne!(option_i32_name, option_bool_name);
    assert_eq!(
        analyzer.enum_layouts[&option_i32_name].variants[0].fields[0].ty,
        Ty::I32
    );
    assert_eq!(
        analyzer.enum_layouts[&option_bool_name].variants[0].fields[0].ty,
        Ty::Bool
    );

    let result_keys = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "core::result::result")
        .map(|instance| instance.key.arguments.clone())
        .collect::<HashSet<_>>();
    assert!(result_keys.contains(&vec![Ty::I32, Ty::Bool]));
    assert!(result_keys.contains(&vec![Ty::Bool, Ty::I32]));
}

#[test]
fn allows_user_redefinitions_of_unimported_core_nominal_names() {
    for (source, name) in [
        (
            "let option = struct { value: i32 }\nlet main(): i32 = { 42 }\n",
            "option",
        ),
        (
            "let result(comptime t: type) = enum { value(value: t) }\nlet main(): i32 = { 42 }\n",
            "result",
        ),
    ] {
        let program = crate::parser::parse(source).expect("reserved-name source must parse");
        let analyzer = Analyzer::new(&program);
        assert!(
            !analyzer.diagnostics.iter().any(|diagnostic| {
                diagnostic.message == format!("duplicate top-level name `{name}`")
            }),
            "unimported core item `{name}` should not reserve a user top-level name"
        );
    }

    let program =
        crate::parser::parse("let never = struct { value: i32 }\nlet main(): i32 = { 42 }\n")
            .expect("reserved never source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(
        !analyzer
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message == "duplicate top-level name `never`" }),
        "prelude `never` should not reserve a user top-level name"
    );
    assert!(analyzer.enum_defs["core::never::never"].variants.is_empty());

    let program =
        crate::parser::parse("let add = struct { value: i32 }\nlet main(): i32 = { 42 }\n")
            .expect("unimported add source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(!analyzer
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.message == "duplicate top-level name `add`" }));
    assert!(analyzer.struct_defs.contains_key("add"));
    assert!(analyzer.traits["core::ops::arith::add"].valid);

    let errors = compile_text("let invalid(value: void): () = { () }\nlet main(): i32 = { 42 }\n")
        .expect_err("`void` must not resolve as a unit alias");
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown type `void`")));
}

#[test]
fn reserves_compiler_provided_control_contracts_for_core() {
    let errors = compile_unresolved_text(
        "let do(comptime t: type)(move action: (): t): t = { action() }\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic
        .message
        .contains("control lang-item name `do` is reserved")));

    let errors = compile_unresolved_text(
        "let throw(comptime error: type)(move error: error): never = { loop { continue() } }\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic
        .message
        .contains("control lang-item name `throw` is reserved")));

    let errors = compile_unresolved_text(
        "let external(value: i32): i32\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.message.contains("has no body")));
}

#[test]
fn finite_sorts_classify_compile_time_values() {
    compile_text(
        "let optimization = sort(1) { size speed }\n\
         let select(comptime o: optimization)(value: i32): i32 = { value }\n\
         let main(): i32 = { select(optimization.speed)(42) }\n",
    )
    .expect("finite sorts must classify their compile-time members");
}

#[test]
fn core_access_is_a_finite_sort_with_shared_and_mut_aliases() {
    compile_resolved_text(
        "use core.borrow.access\n\
         let select(comptime a: access)(value: i32): i32 = { value }\n\
         let main(): i32 = { select(access.mut)(0) + select(shared)(20) + select(mut)(22) }\n",
    )
    .expect("the shared and mut aliases must inhabit the access sort");
}

#[test]
fn rejects_user_abstract_sorts_and_removed_domain_syntax() {
    let errors = compile_unresolved_text(
        "let opaque: sort(2)\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic
        .message
        .contains("abstract sort `opaque` is compiler-owned")));

    for source in [
        "let legacy: domain\nlet main(): i32 = { 0 }\n",
        "let legacy = domain { old }\nlet main(): i32 = { 0 }\n",
    ] {
        let error = crate::parser::parse(source).unwrap_err();
        assert!(error.message.contains("`domain` was removed"));
    }
}

#[test]
fn constructor_sorts_preserve_parameter_group_boundaries_and_sorts() {
    compile_unresolved_text(
        r#"
let curried(comptime element: type)(comptime length: usize) = struct { value: element }
let accept(comptime f: (comptime element: type)(comptime length: usize): type)(): i32 = { 42 }
let main(): i32 = { accept(curried)() }
"#,
    )
    .expect("matching curried constructor sorts must be accepted");

    let wrong_grouping = compile_unresolved_text(
        r#"
let flat(comptime element: type, comptime length: usize) = struct { value: element }
let accept(comptime f: (comptime element: type)(comptime length: usize): type)(): i32 = { 42 }
let main(): i32 = { accept(flat)() }
"#,
    )
    .unwrap_err();
    assert!(
        wrong_grouping.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("expects sort `(type)(usize): type`")
                && diagnostic
                    .message
                    .contains("constructor `flat` has sort `(type, usize): type`")
        }),
        "{wrong_grouping:?}"
    );

    let wrong_parameter_sort = compile_unresolved_text(
        r#"
let by_type(comptime element: type)(comptime length: type) = struct { value: element }
let accept(comptime f: (comptime element: type)(comptime length: usize): type)(): i32 = { 42 }
let main(): i32 = { accept(by_type)() }
"#,
    )
    .unwrap_err();
    assert!(
        wrong_parameter_sort.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("constructor `by_type` has sort `(type)(type): type`")
        }),
        "{wrong_parameter_sort:?}"
    );
}

#[test]
fn ordinary_pure_functions_evaluate_naturally_in_dependent_types() {
    let llvm = compile_text(
        r#"
let next(value: usize): usize = { value + 1 }
let factorial(value: usize): usize = {
  if value == 0 { 1 } else { value * factorial(value - 1) }
}
let read(values: array(i32)(next(2) * 2)): i32 = { values[0] }
let read_factorial(values: array(i32)(factorial(4))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("ordinary pure functions should be available to ctfe");
    assert!(
        llvm.contains("[6 x i32]"),
        "dependent array length was not normalized by ctfe"
    );
    assert!(
        llvm.contains("[24 x i32]"),
        "structurally decreasing recursive ctfe did not normalize"
    );
}

#[test]
fn ctfe_calls_preserve_runtime_groups_labels_and_explicit_generic_arguments() {
    let llvm = compile_text(
        r#"
let combine(comptime bias: usize)
  (left: usize, right: usize)
  (scale: usize): usize = {
  (left + right + bias) * scale
}
let length(): usize = {
  combine(1)(left: 3, right: 2)(scale: 2)
}
let identity(comptime value: type)(move value: value): value = { value }
let inferred(): usize = {
  let value: usize = 7
  identity(value)
}
let read(values: array(i32)(length())): i32 = { values[0] }
let read_direct(
  values: array(i32)(combine(1)(left: 3, right: 2)(scale: 2))
): i32 = { values[0] }
let read_inferred(values: array(i32)(inferred())): i32 = { values[0] }
let read_direct_inferred(values: array(i32)(identity(7))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("ctfe calls should preserve groups, labels, and generic substitution");
    assert!(llvm.contains("[12 x i32]"), "{llvm}");
    assert!(llvm.contains("[7 x i32]"), "{llvm}");
}

#[test]
fn ctfe_calls_statically_resolved_members_and_propagates_return() {
    let llvm = compile_text(
        r#"
let counter = struct { value: usize }
let measurable = trait {
  let measure(move self)(): usize
}
extend(counter) {
  let new(value: usize): counter = { counter { value: value } }
  let add(move self)(amount: usize): usize = { self.value + amount }
}
extend(counter, measurable) {
  let measure(move self)(): usize = { self.value }
}
let select(left: usize): usize = { left }
let select(right: usize): usize = { right + 1 }
let choose(value: usize): usize = {
  if value == 0 {
    return counter.new(3).add(2) + select(right: 1) + counter.new(3).measure()
  }
  value
}
let read(values: array(i32)(choose(0))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("statically resolved members and return should execute during ctfe");
    assert!(llvm.contains("[10 x i32]"), "{llvm}");
}

#[test]
fn ctfe_calls_resolved_cross_module_declarations() {
    let program = crate::modules::resolve_sources(&[
        crate::modules::SourceUnit {
            path: "root.sc".to_owned(),
            module_path: Vec::new(),
            source: "use root.math.dimension\n\
                     let read(values: array(i32)(dimension(4))): i32 = { values[0] }\n\
                     let main(): i32 = { 0 }\n"
                .to_owned(),
            is_root: true,
        },
        crate::modules::SourceUnit {
            path: "math.sc".to_owned(),
            module_path: vec!["math".to_owned()],
            source: "pub let dimension(value: usize): usize = { value + 2 }\n".to_owned(),
            is_root: false,
        },
    ])
    .expect("cross-module ctfe source should resolve");
    let llvm = compile(&program).expect("resolved cross-module calls should execute during ctfe");
    assert!(llvm.contains("[6 x i32]"), "{llvm}");
}

#[test]
fn ctfe_calls_reject_effects_borrows_and_foreign_bodies() {
    let effectful = compile_text(
        r#"
let unsafe = core.unsafe.unsafe
let unsafe_length(): usize with(unsafe) = { unsafe { 1 } }
let read(values: array(i32)(unsafe_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        effectful.iter().any(|diagnostic| diagnostic
            .message
            .contains("effectful function `unsafe_length` cannot run during ctfe")),
        "{effectful:?}"
    );

    let borrowed = compile_text(
        r#"
let read_borrow(value: borrow(usize)): usize = { *value }
let borrowed_length(): usize = {
  let value: usize = 1
  read_borrow(borrow(value))
}
let read(values: array(i32)(borrowed_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        borrowed.iter().any(|diagnostic| {
            diagnostic.message.contains("not in the pure ctfe subset")
                || diagnostic.message.contains("cannot borrow runtime storage")
        }),
        "{borrowed:?}"
    );

    let foreign = compile_text(
        r#"
let abs(value: i32): i32 = foreign(c)
let foreign_length(): usize = { abs(1); 1 }
let read(values: array(i32)(foreign_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        foreign.iter().any(|diagnostic| diagnostic
            .message
            .contains("has no source body available to ctfe")),
        "{foreign:?}"
    );
}

#[test]
fn ctfe_normalizes_dependent_lengths_after_generic_substitution() {
    let llvm = compile_text(
        r#"
let next(value: usize): usize = { value + 1 }
let buffer(comptime element: type)(comptime length: usize) = struct {
  values: array(element)(next(length)),
}
let main(): i32 = {
  let buffer = buffer(i32)(2) { values: [40, 1, 1] }
  buffer.values[0]
}
"#,
    )
    .expect("dependent lengths should normalize after substituting generic static values");
    assert!(llvm.contains("[3 x i32]"));
}

#[test]
fn ctfe_supports_every_builtin_scalar_with_exact_width_semantics() {
    let llvm = compile_text(
        r#"
let keep_unit(value: ()): () = { value }
let keep_i8(value: i8): i8 = { value }
let keep_i16(value: i16): i16 = { value }
let keep_i32(value: i32): i32 = { value }
let keep_i64(value: i64): i64 = { value }
let keep_i128(value: i128): i128 = { value }
let keep_isize(value: isize): isize = { value }
let keep_u8(value: u8): u8 = { value }
let keep_u16(value: u16): u16 = { value }
let keep_u32(value: u32): u32 = { value }
let keep_u64(value: u64): u64 = { value }
let keep_u128(value: u128): u128 = { value }
let keep_usize(value: usize): usize = { value }
let keep_bool(value: bool): bool = { value }

let scalar_length(seed: usize): usize = {
  let unit_value: () = keep_unit(())
  let signed8: i8 = keep_i8(-128)
  let signed16: i16 = keep_i16(-32768)
  let signed32: i32 = keep_i32(-2147483648)
  let signed64: i64 = keep_i64(-9223372036854775808)
  let signed128: i128 = keep_i128(-170141183460469231731687303715884105728)
  let signed_pointer: isize = keep_isize(-9223372036854775808)
  let unsigned8: u8 = keep_u8(255)
  let unsigned16: u16 = keep_u16(65535)
  let unsigned32: u32 = keep_u32(4294967295)
  let unsigned64: u64 = keep_u64(18446744073709551615)
  let unsigned128: u128 = keep_u128(340282366920938463463374607431768211455)
  let unsigned_pointer: usize = keep_usize(18446744073709551615)
  let truth: bool = keep_bool(true)
  let unit_matches = match unit_value
    { _ -> true }
  let signed_matches = match signed8
    { -128 -> true }
    { _ -> false }
  let unsigned_matches = match unsigned128
    { 340282366920938463463374607431768211455 -> true }
    { _ -> false }
  if truth &&
    unit_matches &&
    signed_matches &&
    unsigned_matches &&
    signed16 + 32767 == -1 &&
    signed32 / 2 == -1073741824 &&
    signed64 / 2 == -4611686018427387904 &&
    signed128 + 1 < 0 &&
    signed_pointer / 2 == -4611686018427387904 &&
    (unsigned8 & 15) == 15 &&
    unsigned16 - 1 == 65534 &&
    unsigned32 - 1 == 4294967294 &&
    unsigned64 > 9223372036854775808 &&
    (unsigned128 >> 127) == 1 &&
    (unsigned_pointer >> 63) == 1 {
    seed
  } else {
    0
  }
}

let read(values: array(i32)(scalar_length(3))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("all builtin scalar families should evaluate during ctfe");
    assert!(llvm.contains("[3 x i32]"), "{llvm}");
}

#[test]
fn ctfe_checks_exact_width_overflow_and_shift_counts() {
    let overflow = compile_unresolved_text(
        r#"
let overflowing_length(seed: usize): usize = {
  let maximum: u8 = 255
  let invalid: u8 = maximum + 1
  seed + invalid
}
let read(values: array(i32)(overflowing_length(1))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        overflow.iter().any(|diagnostic| diagnostic
            .message
            .contains("integer arithmetic overflows `u8` during ctfe")),
        "{overflow:?}"
    );

    let shift = compile_unresolved_text(
        r#"
let invalid_shift_length(seed: usize): usize = {
  let value: u16 = 1
  let count: u16 = 16
  let invalid: u16 = value << count
  seed + invalid
}
let read(values: array(i32)(invalid_shift_length(1))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        shift.iter().any(|diagnostic| diagnostic
            .message
            .contains("shift count `16` is out of range for `u16` (16-bit) during ctfe")),
        "{shift:?}"
    );
}

#[test]
fn ctfe_evaluates_nested_tuples_and_fixed_arrays() {
    let llvm = compile_text(
        r#"
let unpack(value: (i8, (usize, bool))): usize = {
  match value
    { (_, (length, true)) -> length }
    { _ -> 0 }
}

let keep_array(
  values: array(usize)(3)
): array(usize)(3) = {
  values
}

let select(
  values: array(usize)(3),
  index: usize
): usize = {
  values[index]
}

let composite_length(seed: usize): usize = {
  let tuple: (i8, (usize, bool)) = (7, (seed, true))
  let projected: usize = tuple.1.0
  let values: array(usize)(3) = keep_array([0, unpack(tuple), 9])
  let combined: (array(usize)(3), (usize, bool)) =
    (values, (projected, true))
  match combined
    { (items, (length, true)) -> select(items, 1) + length - seed }
    { _ -> 0 }
}

let read(values: array(i32)(composite_length(3))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("nested tuples and fixed arrays should evaluate during ctfe");
    assert!(llvm.contains("[3 x i32]"), "{llvm}");
}

#[test]
fn ctfe_evaluates_concrete_struct_values_and_patterns() {
    let llvm = compile_text(
        r#"
let point = struct {
  x: usize,
  y: usize,
}

let tagged(comptime value: type) = struct {
  value: value,
  tag: usize,
}

let make(seed: usize): tagged(point) = {
  let result = tagged(point) { tag: 9,
    value: point { y: seed + 1, x: seed },
  }
  result
}

let select(value: tagged(point)): usize = {
  let projected: usize = value.value.x
  match value
    { tagged(value: point(x: left, y: right), tag: 9) ->
        left + right - projected
    }
    { _ -> 0 }
}

let read(values: array(i32)(select(make(3)))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .expect("concrete generic and non-generic structs should evaluate during ctfe");
    assert!(llvm.contains("[4 x i32]"), "{llvm}");
}

#[test]
fn ctfe_rejects_structs_that_require_runtime_storage_or_destruction() {
    let address_dependent = compile_unresolved_text(
        r#"
let address = struct { value: ptr(i32) }
let invalid_length(): usize = {
  let value: address = address { value: unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  } }
  1
}
let read(values: array(i32)(invalid_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        address_dependent.iter().any(|diagnostic| diagnostic
            .message
            .contains("depends on runtime storage or an address")),
        "{address_dependent:?}"
    );

    let custom_drop = compile_text(
        r#"
let resource = struct { value: usize }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {}
}
let invalid_length(): usize = {
  let value: resource = resource { value: 1 }
  value.value
}
let read(values: array(i32)(invalid_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        custom_drop.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("implements `droppable` and requires runtime destruction")
        }),
        "{custom_drop:?}"
    );

    let unsized_errors = compile_text(
        r#"
let slice = core.memory.slice
let view = struct { values: slice(i32) }
let invalid_length(): usize = {
  let value: view = view { values: loop {} }
  1
}
let read(values: array(i32)(invalid_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        unsized_errors.iter().any(|diagnostic| diagnostic
            .message
            .contains("depends on runtime storage or an address")),
        "{unsized_errors:?}"
    );

    let allocating = compile_text(
        r#"
let box = alloc.boxed.box
let owner = struct { value: box(usize) }
let invalid_length(): usize = {
  let value: owner = owner { value: box.new(1) }
  1
}
let read(values: array(i32)(invalid_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        allocating.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("implements `droppable` and requires runtime destruction")
        }),
        "{allocating:?}"
    );
}

#[test]
fn ctfe_evaluates_enum_values_guards_and_standard_fallible_types() {
    let llvm = compile_text(
        r#"
let option = core.option
let result = core.result

let point = struct { value: usize }
let choice(comptime value: type) = enum {
  empty,
  single(value),
  named(left: value, right: usize),
}

let make(seed: usize): choice(point) = {
  let value = choice(point).named(
    right: seed + 1,
    left: point { value: seed },
  )
  value
}

let evaluate(value: choice(point)): usize = {
  match value
    { choice.named(right: right, left: point(value: left)) if right > left ->
        right
    }
    { choice.single(point(value: item)) -> item }
    { choice.empty -> 0 }
    { _ -> 0 }
}

let standard(seed: usize): usize = {
  let present: option(usize) = option.some(seed)
  let absent: option(usize) = option.none
  let outcome: result(bool)(usize) = result.ok(
    match present
      { some(value) -> value }
      { none -> 0 }
  )
  let selected: usize = match outcome
    { ok(value) -> value }
    { err(_) -> 0 }
  let missing: usize = match absent
    { some(value) -> value }
    { none -> 0 }
  selected + missing
}

let unit_value(seed: usize): usize = {
  let value: choice(point) = choice(point).empty
  evaluate(value) + seed
}

let positional(seed: usize): usize = {
  evaluate(choice(point).single(point { value: seed }))
}

let read(
  values: array(i32)(
    evaluate(make(3)) + standard(2) + unit_value(1) + positional(1)
  )
): i32 = {
  values[0]
}
let main(): i32 = { 0 }
"#,
    )
    .expect("enum variants, guards, option, and result should evaluate during ctfe");
    assert!(llvm.contains("[8 x i32]"), "{llvm}");
}

#[test]
fn ctfe_rejects_enum_payloads_that_require_runtime_destruction() {
    let errors = compile_text(
        r#"
let box = alloc.boxed.box
let choice = enum {
  empty,
  owned(box(usize)),
}
let invalid_length(): usize = {
  let value: choice = choice.empty
  match value
    { empty -> 1 }
    { owned(_) -> 2 }
}
let read(values: array(i32)(invalid_length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("implements `droppable` and requires runtime destruction")
        }),
        "{errors:?}"
    );
}

#[test]
fn ctfe_rejects_dynamic_array_indexes_out_of_bounds() {
    let errors = compile_unresolved_text(
        r#"
let invalid_length(index: usize): usize = {
  let values: array(usize)(2) = [10, 20]
  values[index]
}
let read(values: array(i32)(invalid_length(2))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|diagnostic| diagnostic
            .message
            .contains("ctfe array index 2 is out of bounds for length 2")),
        "{errors:?}"
    );
}

#[test]
fn ctfe_rejects_an_executed_non_exhaustive_guarded_pattern() {
    let errors = compile_unresolved_text(
        r#"
let select(value: bool): usize = {
  match value
    { true if false -> 1 }
    { false -> 2 }
}
let read(values: array(i32)(select(true))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|diagnostic| diagnostic
            .message
            .contains("non-exhaustive match during ctfe")),
        "{errors:?}"
    );
}

#[test]
fn ctfe_rejects_runtime_mutation_and_reports_arithmetic_failures() {
    let mutation = compile_unresolved_text(
        r#"
let increment(value: usize): usize = {
  let mut result = value
  result = result + 1
  result
}
let read(values: array(i32)(increment(2))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        mutation.iter().any(|diagnostic| diagnostic
            .message
            .contains("mutable bindings are not permitted during ctfe")),
        "{mutation:?}"
    );

    let division_by_zero = compile_unresolved_text(
        r#"
let divide(left: usize, right: usize): usize = { left / right }
let read(values: array(i32)(divide(4, 0))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        division_by_zero
            .iter()
            .any(|diagnostic| diagnostic.message.contains("division by zero during ctfe")),
        "{division_by_zero:?}"
    );

    let nontermination = compile_unresolved_text(
        r#"
let repeat(value: usize): usize = { repeat(value) }
let read(values: array(i32)(repeat(1))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        nontermination.iter().any(|diagnostic| diagnostic
            .message
            .contains("recursive ctfe call `repeat` repeated with the same arguments")),
        "{nontermination:?}"
    );
}

#[test]
fn ctfe_enforces_deterministic_call_step_and_aggregate_budgets() {
    let active_calls = compile_unresolved_text(
        r#"
let descend(value: usize): usize = {
  if value == 0 { 1 } else { descend(value - 1) }
}
let read(values: array(i32)(descend(200))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        active_calls.iter().any(|diagnostic| diagnostic
            .message
            .contains("evaluation exceeded the 128-active-call limit")),
        "{active_calls:?}"
    );

    let fuel = compile_unresolved_text(
        r#"
let fibonacci(value: usize): usize = {
  if value < 2 {
    1
  } else {
    fibonacci(value - 1) + fibonacci(value - 2)
  }
}
let read(values: array(i32)(fibonacci(20))): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    );
    let fuel = fuel.unwrap_err();
    assert!(
        fuel.iter().any(|diagnostic| diagnostic
            .message
            .contains("evaluation exceeded the 16384-step limit")),
        "{fuel:?}"
    );

    let elements = compile_unresolved_text(
        r#"
let oversized = enum {
  empty,
  payload(array(u8)(65537)),
}
let length(): usize = {
  let value: oversized = oversized.empty
  match value
    { empty -> 1 }
    { payload(_) -> 2 }
}
let read(values: array(i32)(length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        elements.iter().any(|diagnostic| diagnostic
            .message
            .contains("ctfe array exceeds the 65536-element limit")),
        "{elements:?}"
    );

    let nodes = compile_unresolved_text(
        r#"
let oversized = enum {
  empty,
  payload(array(array(u8)(256))(256)),
}
let length(): usize = {
  let value: oversized = oversized.empty
  match value
    { empty -> 1 }
    { payload(_) -> 2 }
}
let read(values: array(i32)(length())): i32 = { values[0] }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        nodes.iter().any(|diagnostic| diagnostic
            .message
            .contains("ctfe value exceeds the 65536-node limit")),
        "{nodes:?}"
    );
}

#[test]
fn type_parameters_shadow_core_lang_item_names_in_type_heads() {
    for (name, source) in [
        (
            "option",
            "let choose(comptime option: type)(): i32 = { option.none ?? 42 }\n\
             let main(): i32 = { choose(i32)() }\n",
        ),
        (
            "result",
            "let choose(comptime result: type)(): i32 = { result.ok(1) ?? 42 }\n\
             let main(): i32 = { choose(i32)() }\n",
        ),
    ] {
        let errors = compile_text(source).unwrap_err();
        assert!(
            errors.iter().any(|diagnostic| diagnostic
                .message
                .contains(&format!("type parameter `{name}`"))),
            "type parameter `{name}` incorrectly acquired core semantics: {errors:?}"
        );
    }
}

#[test]
fn generic_function_validation_rolls_back_temporary_nominal_instances() {
    let program = crate::parser::parse(
        "let cell(comptime t: type) = struct { value: t }\n\
         let wrap(comptime t: type)(move value: t): cell(t) = { cell(t) { value: value } }\n\
         let main(): i32 = { wrap(i32)(42).value }\n",
    )
    .expect("generic function and nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );
    let baseline_nominals = [
        analyzer.lang_item_name(LangItemKind::Never),
        analyzer.lang_item_name(LangItemKind::PartialOrdering),
    ];
    assert!(analyzer
        .nominal_instances
        .values()
        .all(
            |instance| baseline_nominals.contains(&instance.key.template.as_str())
                || instance.key.template.contains("::")
        ));
    assert!(analyzer
        .nominal_instance_names
        .keys()
        .all(
            |key| baseline_nominals.contains(&key.template.as_str()) || key.template.contains("::")
        ));
    assert!(!analyzer.struct_layouts.contains_key("cell"));
    assert!(!analyzer.struct_order.iter().any(|name| name == "cell"));

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    analyzer.analyze().expect("closed generic nominal hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instances: Vec<_> = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "cell")
        .collect();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].key.arguments, vec![Ty::I32]);
    assert!(analyzer.nominal_instances.keys().all(|name| {
        !name.contains("$generic$") && markers.iter().all(|marker| !name.contains(marker))
    }));

    let ir = compile(&program).expect("closed generic nominal program must compile");
    for marker in markers {
        assert!(!ir.contains(&marker));
        assert!(!ir.contains(&hex_name(&marker)));
    }
}

#[test]
fn where_bound_validation_rolls_back_assumed_trait_implementations() {
    let program = crate::parser::parse(
        "let measure = trait { let measure(self: borrow(self))(): i32 }\n\
         let measured_value = struct { value: i32 }\n\
         extend(measured_value, measure) {\n\
           let measure(self: borrow(self))(): i32 = { self.value }\n\
         }\n\
         let read(comptime t: type)(value: borrow(t)): i32\n\
         where t: measure = { value.measure() }\n\
         let main(): i32 = { let value = measured_value { value: 42 }; read(value) }\n",
    )
    .expect("where-bound method source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer
            .signatures
            .keys()
            .all(|name| !name.contains("$generic$bound$")),
        "assumed method signatures escaped template validation"
    );
    assert!(analyzer.trait_impls.keys().all(|key| {
        !analyzer
            .abstract_type_parameters
            .keys()
            .any(|marker| nominal_name(&key.self_ty) == Some(marker.as_str()))
    }));

    analyzer
        .analyze()
        .expect("concrete monomorphization must select the real implementation");
    assert!(
        analyzer.diagnostics.is_empty(),
        "{:?}",
        analyzer.diagnostics
    );
}

#[test]
fn rejects_invalid_generic_nominal_forms_without_instantiating_them() {
    let cases = [
        (
            "let invalid(comptime t: type) = struct { next: invalid(t) }\n\
             let main(): i32 = { 42 }\n",
            "recursive generic value layout has infinite size",
        ),
        (
            "let wrap(comptime t: type) = struct { value: t }\n\
             let grow(comptime t: type) = struct { next: wrap(grow(wrap(t))) }\n\
             let main(): i32 = { 42 }\n",
            "recursive generic value layout has infinite size",
        ),
        (
            "let invalid(comptime t: type) = struct { value: missing }\n\
             let main(): i32 = { 42 }\n",
            "unknown type `missing`",
        ),
        (
            "let cell(comptime t: type) = struct { value: t }\n\
             let main(): i32 = { cell(u: i32) { value: cell(i32) { value: 42 } }.value.value }\n",
            "expects exactly one argument group",
        ),
        (
            "let cell(comptime t: type) = struct { value: t }\n\
             extend(cell) { let answer = 42 }\n\
             let main(): i32 = { 42 }\n",
            "generic extend target `cell` is not supported",
        ),
    ];
    for (source, expected) in cases {
        let diagnostics = compile_text(source).expect_err("source must be rejected");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn validation_rollback_does_not_keep_inferred_helpers_or_drop_real_instances() {
    let source = "let identity(comptime t: type)(move value: t): t = { value }\n\
                  let helper(value: i32) = { identity(i32)(value) }\n\
                  let preserve(comptime t: type)(move value: t): t = { helper(0); value }\n\
                  let main(): i32 = { preserve(i32)(42) }\n";
    let ir = compile_text(source).expect("validation rollback program must compile");
    let identity = function_instance_name(&FunctionInstanceKey {
        template: "identity".into(),
        arguments: vec![Ty::I32],
    });
    let symbol = function_symbol(&identity);
    assert!(ir.contains(&format!("define internal i32 @{symbol}(i32 %arg.0)")));
}

#[test]
fn creates_distinct_stable_names_for_generic_function_instances() {
    let i32_key = FunctionInstanceKey {
        template: "identity".into(),
        arguments: vec![Ty::I32],
    };
    let bool_key = FunctionInstanceKey {
        template: "identity".into(),
        arguments: vec![Ty::Bool],
    };
    assert_eq!(
        function_instance_name(&i32_key),
        function_instance_name(&i32_key)
    );
    assert_ne!(
        function_instance_name(&i32_key),
        function_instance_name(&bool_key)
    );
}

#[test]
fn emits_flattened_curried_call_and_i32_wrapper() {
    let add = function(
        "add",
        vec![vec![param("x", Type::I32)], vec![param("y", Type::I32)]],
        Type::I32,
        Expr::Binary(
            Box::new(Expr::Name("x".into())),
            BinaryOp::Add,
            Box::new(Expr::Name("y".into())),
        ),
    );
    let call = Expr::Call(
        Box::new(Expr::Call(
            Box::new(Expr::Name("add".into())),
            vec![arg(Expr::Integer(20))],
        )),
        vec![arg(Expr::Integer(22))],
    );
    let main = function("main", vec![vec![]], Type::I32, call);
    let ir = compile(&Program::new(vec![add, main])).unwrap();
    assert!(ir.contains("call i32 @sali.fn.616464(i32 20, i32 22)"));
    assert!(ir.contains("define i32 @main()"));
    assert!(ir.contains("ret i32 %status"));
}

#[test]
fn public_library_functions_use_package_qualified_export_symbols() {
    let ir = compile_library_text(
        "pub let answer(): i32 = { 42 }\n\
         let private_answer(): i32 = { answer() }\n",
    )
    .expect("public library function must emit");
    let exported = ir
        .lines()
        .find_map(|line| {
            line.strip_prefix("define i32 @sali.export.")
                .and_then(|suffix| suffix.split_once('('))
                .map(|(suffix, _)| format!("sali.export.{suffix}"))
        })
        .expect("answer export");
    assert!(ir.contains(&format!("define i32 @{exported}()")));
    assert!(ir.contains(&format!("call i32 @{exported}()")));
    assert!(ir.contains(&format!(
        "define internal i32 @{}()",
        function_symbol("private_answer")
    )));
    assert!(!ir.contains(&format!("define internal i32 @{exported}()")));
}

#[test]
fn public_library_globals_use_package_qualified_export_symbols() {
    let ir = compile_library_text(
        "pub let answer: i32 = 42\n\
         pub let read(): i32 = { answer }\n",
    )
    .expect("public library global must emit");
    let exported = ir
        .lines()
        .find_map(|line| {
            line.strip_prefix("@sali.export.")
                .and_then(|suffix| suffix.split_once(" ="))
                .map(|(suffix, _)| format!("sali.export.{suffix}"))
        })
        .expect("answer export");
    assert!(ir.contains(&format!("@{exported} = constant i32 42")));
    assert!(ir.contains(&format!("load i32, ptr @{exported}")));
    assert!(!ir.contains(&format!("@{exported} = internal")));
}

#[test]
fn public_generic_specializations_remain_consumer_owned_and_internal() {
    let ir = compile_library_text(
        "pub let identity(comptime t: type)(move value: t): t = { value }\n\
         pub let answer(): i32 = { identity(i32)(42) }\n",
    )
    .expect("public generic specialization must emit");
    assert_eq!(
        ir.lines()
            .filter(|line| line.contains("define ") && line.contains("@sali.export."))
            .count(),
        1,
        "{ir}"
    );
    assert!(
        ir.lines()
            .any(|line| line.starts_with("define internal i32 @sali.fn.")
                && line.contains("(i32 %arg.0)")),
        "{ir}"
    );
}

#[test]
fn rejects_unsized_native_call_parameters_and_returns_before_emission() {
    let diagnostics = compile_library_text(
        "let slice = core.memory.slice\n\
         let invalid(value: slice(i32)): slice(i32) = { loop {} }\n",
    )
    .expect_err("unsized native call boundary must fail");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("parameter `value` of `invalid` has unsized type")),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function `invalid` returns unsized type")),
        "{diagnostics:?}"
    );
}

#[test]
fn emits_global_if_mutation_and_short_circuit() {
    let global = Item::Global(Binding {
        value_source: None,
        mutable: false,
        name: "answer".into(),
        annotation: Some(Type::I64),
        value: Expr::Binary(
            Box::new(Expr::Integer(40)),
            BinaryOp::Add,
            Box::new(Expr::Integer(2)),
        ),
    });
    let body = Expr::Block(
        vec![
            Stmt::Let(Binding {
                value_source: None,
                mutable: true,
                name: "x".into(),
                annotation: Some(Type::I32),
                value: Expr::Integer(0),
            }),
            Stmt::Expr(Expr::If {
                condition: Box::new(Expr::Binary(
                    Box::new(Expr::Bool(true)),
                    BinaryOp::And,
                    Box::new(Expr::Bool(false)),
                )),
                then_branch: Box::new(Expr::Block(
                    vec![Stmt::Expr(Expr::Assign(
                        Box::new(Expr::Name("x".into())),
                        Box::new(Expr::Integer(1)),
                    ))],
                    None,
                )),
                else_branch: None,
            }),
        ],
        Some(Box::new(Expr::Name("x".into()))),
    );
    let main = function("main", vec![vec![]], Type::I32, body);
    let ir = compile(&Program::new(vec![global, main])).unwrap();
    assert!(ir.contains("@sali.global.616e73776572 = internal unnamed_addr constant i64 42"));
    assert!(ir.contains("phi i1"));
    assert!(ir.contains("store i32 1"));
}

#[test]
fn evaluates_constant_bitwise_operations_and_rejects_invalid_shifts() {
    let ir = compile_text(
        "let mask: i32 = (6 & 3) | (8 ^ 1)\n\
         let shifted: i32 = 8 >> 2\n\
         let main(): i32 = { mask + shifted }\n",
    )
    .expect("constant bitwise operations must compile");
    assert!(ir.contains("constant i32 11"));
    assert!(ir.contains("constant i32 2"));

    for count in ["-1", "32"] {
        let errors = compile_text(&format!(
            "let invalid: i32 = 1 << {count}\nlet main(): i32 = {{ 0 }}\n"
        ))
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error
                .message
                .contains(&format!("shift count `{count}` is out of range for `i32`"))
        }));
    }
}

#[test]
fn emits_nominal_aggregates_and_tag_switches() {
    let ir = compile_text(
        r#"
let pair = struct { left: i32, right: i32 }
let choice = enum {
  pair(left: pair),
  empty,
}
let global: pair = pair { left: 40, right: 2 }
let read(choice: choice): i32 = { choice match {
  choice.pair(left: pair) => pair.left + pair.right,
  choice.empty => 0,
} }
let main(): i32 = { read(choice.pair(left: global)) }
"#,
    )
    .unwrap();
    assert!(ir.contains(&format!("%{} = type {{ i32, i32 }}", type_symbol("pair"))));
    assert!(ir.contains(&format!(
        "%{} = type {{ i32, %{} }}",
        type_symbol("choice"),
        type_symbol("pair")
    )));
    assert!(ir.contains("switch i32"));
    assert!(ir.contains("@sali.global.676c6f62616c = internal unnamed_addr constant"));
}

#[test]
fn emits_typed_nominal_enum_global_constants() {
    let ir = compile_text(
        r#"
let pair = struct { left: i32, right: i32 }
let choice = enum {
  pair(pair),
  empty,
}
let global: choice = choice.pair(pair { left: 40, right: 2 })
let read(value: choice): i32 = { value match {
  pair(value) => value.left + value.right,
  empty => 0,
} }
let main(): i32 = { read(global) }
"#,
    )
    .expect("typed nominal enum globals must normalize and emit");
    assert!(ir.contains(&format!(
        "@sali.global.676c6f62616c = internal unnamed_addr constant %{} {{ i32 0, %{} {{ i32 40, i32 2 }} }}",
        type_symbol("choice"),
        type_symbol("pair")
    )));
}

#[test]
fn global_constants_use_the_shared_source_ctfe_call_path() {
    let llvm = compile_text(
        r#"
let pair(comptime value: type) = struct { left: value, right: value }
let make(comptime value: type)(left: value)(right: value): pair(value) = {
  pair(value) { left: left, right: right }
}
let sum(value: pair(i32)): i32 = { value.left + value.right }
let global: pair(i32) = make(i32)(40)(2)
let total: i32 = sum(global)
let main(): i32 = { total }
"#,
    )
    .expect("global constants should call eligible ordinary source functions");
    assert!(llvm.contains("{ i32 40, i32 2 }"));
    assert!(llvm.contains("constant i32 42"));
}

#[test]
fn string_literals_share_the_core_runtime_type_in_ctfe_and_codegen() {
    let ir = compile_text(
        "pub let greeting: string = \"hello\"\n\
         pub let main(): i32 = { 0 }\n",
    )
    .expect("ordinary string literals should compile as core string values");

    assert!(ir.contains(
        "@salicin.string.68656c6c6f = private unnamed_addr constant [5 x i8] c\"\\68\\65\\6C\\6C\\6F\""
    ));
    assert!(ir.contains("ptr @salicin.string.68656c6c6f, i64 5, i64 0"));
}

#[test]
fn shared_ctfe_consumers_ignore_paths_declaration_order_and_module_traversal() {
    fn analyze(program: &Program) -> HirProgram {
        let mut analyzer = Analyzer::new(program);
        let hir = analyzer
            .analyze()
            .unwrap_or_else(|| panic!("program should analyze: {:?}", analyzer.diagnostics));
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );
        hir
    }

    let model_forward = r#"
pub let packet(comptime t: type) = struct { pub value: t, pub width: usize }
pub let make(comptime t: type)(value: t)(width: usize): packet(t) = {
  packet(t) { value: value, width: width }
}
pub let dimension(value: usize): usize = { value + 2 }
pub let layout_size(comptime t: type)(): u64 = { size_of(t) }
"#;
    let model_reverse = r#"
pub let layout_size(comptime t: type)(): u64 = { size_of(t) }
pub let dimension(value: usize): usize = { value + 2 }
pub let make(comptime t: type)(value: t)(width: usize): packet(t) = {
  packet(t) { value: value, width: width }
}
pub let packet(comptime t: type) = struct { pub value: t, pub width: usize }
"#;
    let root_forward = r#"
use root.model.packet
use root.model.make
use root.model.dimension
use root.model.layout_size
let length: usize = dimension(4)
let global: packet(i32) = make(i32)(42)(length)
let bytes: u64 = layout_size(packet(i32))()
let read(values: array(i32)(dimension(4))): i32 = { values[0] }
let main(): i32 = { global.value + read([0, 0, 0, 0, 0, 0]) }
"#;
    let root_reverse = r#"
use root.model.packet
use root.model.make
use root.model.dimension
use root.model.layout_size
let bytes: u64 = layout_size(packet(i32))()
let global: packet(i32) = make(i32)(42)(length)
let length: usize = dimension(4)
let main(): i32 = { global.value + read([0, 0, 0, 0, 0, 0]) }
let read(values: array(i32)(dimension(4))): i32 = { values[0] }
"#;

    let forward = crate::modules::resolve_sources(&[
        source_unit("/checkout/one/src/main.sc", &[], root_forward, true),
        source_unit(
            "/checkout/one/src/model.sc",
            &["model"],
            model_forward,
            false,
        ),
    ])
    .expect("forward module traversal should resolve");
    let reverse = crate::modules::resolve_sources(&[
        source_unit(
            "/different/checkout/model.sc",
            &["model"],
            model_reverse,
            false,
        ),
        source_unit("/different/checkout/main.sc", &[], root_reverse, true),
    ])
    .expect("reverse module traversal should resolve");

    let forward_hir = analyze(&forward);
    let reverse_hir = analyze(&reverse);
    for name in ["length", "global", "bytes"] {
        assert_eq!(
            forward_hir.normalized_globals.get(name),
            reverse_hir.normalized_globals.get(name),
            "normalized global `{name}` changed with path or traversal order"
        );
    }
    let global = forward_hir
        .normalized_globals
        .get("global")
        .expect("normalized nominal global");
    assert!(
        matches!(
            (&global.ty, &global.kind),
            (
                Ty::Struct(ty_name),
                CtfeValueKind::Struct { name, fields },
            ) if ty_name == name
                && name.contains("model::packet")
                && name.contains("i32")
                && fields.len() == 2
        ),
        "unexpected exact nominal value: {global:?}"
    );
    let bytes = forward_hir
        .normalized_globals
        .get("bytes")
        .expect("normalized layout global");
    assert!(
        matches!(
            &bytes.kind,
            CtfeValueKind::LayoutQuery {
                queried,
                kind: LayoutQueryKind::Size,
            } if queried == &global.ty
        ),
        "layout query lost its exact target type: {bytes:?}"
    );

    let forward_ir = compile(&forward).expect("forward program should compile");
    let reverse_ir = compile(&reverse).expect("reverse program should compile");
    let global_constants = |ir: &str| {
        let mut lines = ir
            .lines()
            .filter(|line| line.starts_with("@sali.global."))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        lines.sort();
        lines
    };
    assert_eq!(
        global_constants(&forward_ir),
        global_constants(&reverse_ir),
        "emitted constants changed with path, declaration, or traversal order"
    );
    assert_eq!(
        compile(&forward).expect("repeated forward program should compile"),
        forward_ir,
        "CTFE LLVM IR changed between identical compilations"
    );
    assert!(forward_ir.contains("[6 x i32]"), "{forward_ir}");
    let bytes_constant = global_constants(&forward_ir)
        .into_iter()
        .find(|line| line.starts_with("@sali.global.6279746573"))
        .expect("emitted bytes global");
    assert!(
        bytes_constant.contains("ptrtoint (ptr getelementptr ("),
        "{bytes_constant}"
    );
}

#[test]
fn ctfe_rejection_diagnostics_ignore_paths_declaration_order_and_traversal() {
    let root = r#"
use root.math.invalid
let length: usize = invalid()
let read(values: array(i32)(length)): i32 = { values[0] }
let main(): i32 = { 0 }
"#;
    let math_forward = r#"
pub let unused(): usize = { 0 }
pub let invalid(): usize = {
  let maximum: u8 = 255
  let overflow: u8 = maximum + 1
  1
}
"#;
    let math_reverse = r#"
pub let invalid(): usize = {
  let maximum: u8 = 255
  let overflow: u8 = maximum + 1
  1
}
pub let unused(): usize = { 0 }
"#;
    let forward = crate::modules::resolve_sources(&[
        source_unit("/checkout/one/main.sc", &[], root, true),
        source_unit("/checkout/one/math.sc", &["math"], math_forward, false),
    ])
    .expect("forward rejection program should resolve");
    let reverse = crate::modules::resolve_sources(&[
        source_unit("/another/checkout/math.sc", &["math"], math_reverse, false),
        source_unit("/another/checkout/main.sc", &[], root, true),
    ])
    .expect("reverse rejection program should resolve");

    let messages = |program: &Program| {
        compile(program)
            .expect_err("overflowing CTFE program must fail")
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect::<Vec<_>>()
    };
    let forward = messages(&forward);
    let reverse = messages(&reverse);
    assert_eq!(
        forward, reverse,
        "CTFE diagnostics changed with input order"
    );
    assert!(
        forward
            .iter()
            .any(|message| message.contains("integer arithmetic overflows `u8` during ctfe")),
        "{forward:?}"
    );
    assert!(
        forward.iter().all(|message| !message.contains('$')),
        "CTFE diagnostics leaked generated identities: {forward:?}"
    );
}

#[test]
fn global_ctfe_preserves_signed_minimum_and_full_u128_values() {
    let llvm = compile_text(
        r#"
let minimum: i128 = -170141183460469231731687303715884105728
let maximum: u128 = 340282366920938463463374607431768211455
let high: u128 = maximum >> 127
let ordered: bool = maximum > 170141183460469231731687303715884105728
let main(): i32 = { if ordered && high == 1 && minimum < 0 { 42 } else { 0 } }
"#,
    )
    .expect("global ctfe should retain exact signed and unsigned 128-bit values");
    assert!(
        llvm.contains("constant i128 -170141183460469231731687303715884105728"),
        "{llvm}"
    );
    assert!(
        llvm.contains("constant i128 340282366920938463463374607431768211455"),
        "{llvm}"
    );
    assert!(llvm.contains("constant i128 1"), "{llvm}");
    assert!(llvm.contains("constant i1 1"), "{llvm}");
}

#[test]
fn rejects_private_fields_across_module_boundaries_for_read_construct_and_pattern() {
    let errors = compile_with_origins(
        r#"
pub let record = struct { value: i32 }
pub let make_record(): record = { record { value: 40 } }
pub let event = enum {
  value(value: i32),
  empty,
}
pub let make_event(): event = { event.value(value: 2) }
let read(): i32 = { make_record().value }
let build(): record = { record { value: 0 } }
let unpack(): i32 = { make_event() match {
  event.value(value: item) => item,
  event.empty => 0,
} }
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
        ],
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("field `record.value` is private")));
    assert!(errors.iter().any(|error| error
        .message
        .contains("field `event.value.value` is private")));
}

#[test]
fn permits_package_fields_in_sibling_modules_and_public_fields_across_packages() {
    compile_with_origins(
        r#"
pub let package_record = struct { pub(package) value: i32 }
pub let make_package(): package_record = { package_record { value: 40 } }
pub let public_record = struct { pub value: i32 }
pub let make_public(): public_record = { public_record { value: 2 } }
let sibling(): i32 = { make_package().value }
let external(): i32 = { make_public().value }
let main(): i32 = { sibling() }
"#,
        vec![
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["consumer"]),
            origin(2, &[]),
            origin(1, &[]),
        ],
    )
    .unwrap();
}

#[test]
fn rejects_package_fields_from_other_packages_and_keeps_public_positional_payloads_open() {
    let errors = compile_with_origins(
        r#"
pub let package_record = struct { pub(package) value: i32 }
pub let make_package(): package_record = { package_record { value: 40 } }
pub let event = enum { value(value: i32), empty }
pub let make_event(): event = { event.value(value: 2) }
let forbidden(): i32 = { make_package().value }
let allowed(): i32 = { make_event() match {
  event.value(value: value) => value,
  event.empty => 0,
} }
let main(): i32 = { allowed() }
"#,
        vec![
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(2, &[]),
            origin(2, &[]),
            origin(2, &[]),
        ],
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error
        .message
        .contains("field `package_record.value` is pub(package)")));
    assert!(!errors
        .iter()
        .any(|error| error.message.contains("event.value.0")));
}

#[test]
fn rejects_inferred_public_function_and_global_types_that_leak_private_nominals() {
    let errors = compile_resolved_with_origins(
        r#"
let option = core.option

let hidden = struct {}
pub let expose() = { hidden {} }
pub let wrapped() = { option(hidden).some(hidden {}) }
pub let shared = hidden {}
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
        ],
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("function `expose` return type")
            && error.message.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("function `wrapped` return type")
            && error.message.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("global `shared` type")
            && error.message.contains("private type `hidden`")
    }));
}

#[test]
fn permits_package_apis_to_infer_root_private_types() {
    compile_with_origins(
        r#"
let root_secret = struct {}
pub(package) let make() = { root_secret {} }
let main(): i32 = { 0 }
"#,
        vec![origin(1, &[]), origin(1, &["child"]), origin(1, &[])],
    )
    .unwrap();
}

#[test]
fn rejects_private_generic_fields_before_inference_and_through_optional_chaining() {
    let errors = compile_resolved_with_origins(
        r#"
let option = core.option

pub let cell(comptime t: type) = struct { value: t }
pub let make(): option(cell(i32)) = { option(cell(i32)).some(cell(i32) { value: 42 }) }
let infer(): cell(i32) = { cell { value: 0 } }
let chain(): option(i32) = { make()?.value }
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["data"]),
            origin(1, &["data"]),
            origin(1, &[]),
            origin(1, &[]),
            origin(1, &[]),
        ],
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .filter(|error| error.message.contains("field `cell.value` is private"))
            .count()
            >= 2,
        "{errors:?}"
    );
}

#[test]
fn short_unit_variants_do_not_bypass_private_enum_visibility() {
    let errors = compile_with_origins(
        r#"
let secret = enum { only }
let forbidden(): i32 = { only; 0 }
let main(): i32 = { forbidden() }
"#,
        vec![origin(1, &["hidden"]), origin(1, &[]), origin(1, &[])],
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unknown name `only`")),
        "{errors:?}"
    );
}

#[test]
fn inherent_members_inherit_the_target_api_boundary_for_leak_checks() {
    let errors = compile_unresolved_text(
        r#"
let hidden = struct {}
pub let public = struct {}
extend(public) {
  let reveal(self: borrow(self))() = { hidden {} }
  let secret = hidden {}
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("return type") && error.message.contains("private type `hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("global") && error.message.contains("private type `hidden`")
    }));
}

#[test]
fn generic_inherent_member_boundaries_include_concrete_type_arguments() {
    compile_unresolved_text(
        r#"
let hidden = struct { value: i32 }
pub let cell(comptime t: type) = struct { pub value: t }
extend(cell(t)) {
  let new(move value: t): cell(t) = { cell { value: value } }
  let take(move self)(): t = { self.value }
}
let use_hidden(): i32 = {
  let cell = cell.new(hidden { value: 42 })
  cell.take().value
}
let main(): i32 = { use_hidden() }
"#,
    )
    .expect("private type arguments must narrow concrete generic member ap_is");
}

#[test]
fn trait_impl_associated_types_cannot_widen_beyond_trait_and_target_access() {
    let errors = compile_unresolved_text(
        r#"
let hidden = struct {}
pub let public = struct {}
pub let convert = trait {
  let output: type
  let convert(self: borrow(self))(): output
}
extend(public, convert) {
  let output = hidden
  let convert(self: borrow(self))(): hidden = { hidden {} }}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("trait implementation")
            && error.message.contains("associated type `output`")
            && error.message.contains("private type `hidden`")
    }));

    compile_unresolved_text(
        r#"
let hidden = struct {}
let private = struct {}
pub let convert = trait {
  let output: type
  let convert(self: borrow(self))(): output
}
extend(private, convert) {
  let output = hidden
  let convert(self: borrow(self))(): hidden = { hidden {} }}
let main(): i32 = { 0 }
"#,
    )
    .unwrap();
}

#[test]
fn generic_trait_extensions_materialize_conditionally_with_associated_types() {
    compile_text(
        r#"
let read = trait {
  let read(self: borrow(self))(): i32
}
let leaf = struct { value: i32 }
extend(leaf, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read)
where t: read {
  let read(self: borrow(self))(): i32 = { self.value.read() }
}

let read_cell(comptime t: type)(cell: borrow(cell(t))): i32
where t: read = { cell.read() }

let value = trait {
  let item: type
  let take(move self)(): item
}
extend(cell(t), value) {
  let item = t
  let take(move self)(): t = { self.value }
}

let main(): i32 = {
  let cell_value = cell { value: leaf { value: 42 } }
  let read_value = read_cell(cell_value)
  let leaf_value = cell_value.take()
  let wrapped = cell { value: leaf_value }
  wrapped.read() + read_value - 42
}
"#,
    )
    .expect("generic trait extensions should instantiate for matching nominal arguments");

    let errors = compile_text(
        r#"
let read = trait {
  let read(self: borrow(self))(): i32
}
let leaf = struct { value: i32 }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read)
where t: read {
  let read(self: borrow(self))(): i32 = { self.value.read() }
}
let main(): i32 = {
  let cell = cell { value: leaf { value: 42 } }
  cell.read()
}
"#,
    )
    .expect_err("an unsatisfied blanket implementation must not materialize");
    assert!(errors
        .iter()
        .any(|error| error.message.contains("unknown method `read`")));

    let errors = compile_text(
        r#"
let read = trait {
  let read(self: borrow(self))(): i32
}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read) {
  let read(self: borrow(self))(): i32 = { 1 }
}
extend(cell(t), read) {
  let read(self: borrow(self))(): i32 = { 2 }
}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("overlapping blanket implementations must be rejected");
    assert!(errors.iter().any(|error| error
        .message
        .contains("overlapping generic trait implementation")));

    compile_text(
        r#"
let convert(comptime to: type) = trait {
  let convert(self: borrow(self))(): to
}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 1 }
}
extend(cell(t), convert(i64)) {
  let convert(self: borrow(self))(): i64 = { 2 }
}
let main(): i32 = {
  let cell = cell { value: true }
  42
}
"#,
    )
    .expect("blanket implementations with disjoint trait arguments should coexist");

    compile_text(
        r#"
let convert(comptime to: type) = trait { let convert(self: borrow(self))(): to }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), convert(t))
where t: copyable {
  let convert(self: borrow(self))(): t = { self.value }}
extend(cell(i32), convert(i64)) {
  let convert(self: borrow(self))(): i64 = { 42 }
}
let main(): i32 = { 42 }
"#,
    )
    .expect("a concrete implementation disjoint after target substitution should coexist");

    for source in [
        r#"
let convert(comptime to: type) = trait { let convert(self: borrow(self))(): to }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 1 }
}
extend(cell(i32), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 2 }
}
let main(): i32 = { 42 }
"#,
        r#"
let convert(comptime to: type) = trait { let convert(self: borrow(self))(): to }
let cell(comptime t: type) = struct { value: t }
extend(cell(i32), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 2 }
}
extend(cell(t), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 1 }
}
let main(): i32 = { 42 }
"#,
    ] {
        let errors = compile_text(source)
            .expect_err("blanket and concrete specializations must overlap in either order");
        assert!(errors.iter().any(|error| error.message.contains("overlap")));
    }

    compile_text(
        r#"
let read = trait { let read(self: borrow(self))(): i32 }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read)
where t: read {
  let read(self: borrow(self))(): i32 = { self.value.read() }
}
let main(): i32 = { 42 }
"#,
    )
    .expect("an uninstantiated blanket body should validate from its where proofs");

    let mismatch = compile_text(
        r#"
let read = trait { let read(self: borrow(self))(): i32 }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read) {
  let read(self: borrow(self))(): i64 = { 0 }
}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an uninstantiated blanket signature mismatch must be rejected");
    assert!(mismatch.iter().any(|error| error
        .message
        .contains("signature mismatch in generic implementation")));

    let invalid_body = compile_text(
        r#"
let read = trait { let read(self: borrow(self))(): i32 }
let cell(comptime t: type) = struct { value: t }
extend(cell(t), read) {
  let read(self: borrow(self))(): i32 = { missing }
}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an uninstantiated blanket method body must be checked");
    assert!(invalid_body
        .iter()
        .any(|error| error.message.contains("unknown name `missing`")));
}

#[test]
fn generic_copy_and_drop_extensions_follow_concrete_instance_semantics() {
    compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(t), copyable)
where t: copyable {}
let sum(copy cell: cell(i32)): i32 = { cell.value }
let main(): i32 = {
  let cell = cell { value: 42 }
  let first = cell
  sum(cell) + first.value - 42
}
"#,
    )
    .expect("a structurally valid blanket copyable instance should be copyable");

    compile_text(
        r#"
let maybe(comptime t: type) = enum {
  some(t),
  none,
}
extend(maybe(t), copyable)
where t: copyable {}
let read(copy value: maybe(i32)): i32 = { value match {
  some(number) => number,
  none => 0,
} }
let main(): i32 = {
  let value: maybe(i32) = maybe.some(42)
  let duplicate = value
  read(value) + read(duplicate) - 42
}
"#,
    )
    .expect("blanket copyable should validate and materialize for generic enums");

    compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { self.value = 0 }}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), copyable)
where t: copyable {}
let main(): i32 = {
  let cell = cell { value: resource { value: 42 } }
  let moved = cell
  moved.value.value
}
"#,
    )
    .expect("an unsatisfied blanket copyable predicate should leave the instance movable");

    let invalid = compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { self.value = 0 }}
let cell(comptime t: type) = struct { value: t }
extend(cell(t), copyable) {}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an unconditional blanket copyable must still be structurally valid");
    assert!(invalid
        .iter()
        .any(|error| error.message.contains("not structurally valid")));

    let conflict = compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(t), copyable)
where t: copyable {}
extend(cell(t), droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = {
  let cell = cell { value: 42 }
  42
}
"#,
    )
    .expect_err("a concrete instance cannot acquire both blanket copyable and droppable");
    assert!(conflict
        .iter()
        .any(|error| error.message.contains("both `copyable` and `droppable`")));

    let foreign_copy = compile_resolved_with_origins(
        r#"
pub let cell(comptime t: type) = struct { value: t }
extend(cell(t), copyable)
where t: copyable {}
let main(): i32 = { 42 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("blanket copyable must be owned by the target package");
    assert!(foreign_copy
        .iter()
        .any(|error| error.message.contains("package that defines the type")));

    let foreign_drop = compile_resolved_with_origins(
        r#"
pub let cell(comptime t: type) = struct { value: t }
extend(cell(t), droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { 42 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("blanket droppable must be owned by the target package");
    assert!(foreign_drop
        .iter()
        .any(|error| error.message.contains("package that defines the type")));

    let missing_drop = compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(t), droppable) {}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an unused blanket droppable implementation must still be complete");
    assert!(missing_drop.iter().any(|error| error
        .message
        .contains("missing trait method `core::marker::droppable.drop`")));
}

#[test]
fn rejects_recursive_value_layouts() {
    let errors = compile_text(
        r#"
let first = struct { next: second }
let second = enum {
  again(first),
  end,
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("recursive value layout")
            && error.message.contains("first -> second -> first")
    }));
}

#[test]
fn unit_entry_uses_i32_c_wrapper() {
    let main = function("main", vec![vec![]], Type::Unit, Expr::Unit);
    let ir = compile(&Program::new(vec![main])).unwrap();
    assert!(ir.contains("define internal void @sali.fn.6d61696e()"));
    assert!(ir.contains("call void @sali.fn.6d61696e()"));
    assert!(ir.contains("ret i32 0"));
}

#[test]
fn short_circuit_rhs_may_return_without_a_second_terminator() {
    let body = Expr::Block(
        vec![
            Stmt::Expr(Expr::Binary(
                Box::new(Expr::Bool(true)),
                BinaryOp::And,
                Box::new(Expr::Return(Some(Box::new(Expr::Integer(1))))),
            )),
            Stmt::Expr(Expr::Binary(
                Box::new(Expr::Bool(false)),
                BinaryOp::Or,
                Box::new(Expr::Return(Some(Box::new(Expr::Integer(2))))),
            )),
        ],
        Some(Box::new(Expr::Integer(0))),
    );
    let main = function("main", vec![vec![]], Type::I32, body);
    let ir = compile(&Program::new(vec![main])).unwrap();
    assert!(ir.contains("ret i32 1"));
    assert!(ir.contains("ret i32 2"));
    let main_start = ir.find("define i32 @main").expect("c entry wrapper");
    assert!(!ir[main_start..].contains("phi i1"));
}

#[test]
fn accepts_minimum_signed_integer_literals() {
    let minimum_i64 = function(
        "minimum_i64",
        vec![vec![]],
        Type::I64,
        Expr::Unary(
            UnaryOp::Neg,
            Box::new(Expr::Integer(9_223_372_036_854_775_808)),
        ),
    );
    let main = function(
        "main",
        vec![vec![]],
        Type::I32,
        Expr::Unary(UnaryOp::Neg, Box::new(Expr::Integer(2_147_483_648))),
    );
    let minimum_i128 = function(
        "minimum_i128",
        vec![vec![]],
        Type::I128,
        Expr::Unary(
            UnaryOp::Neg,
            Box::new(Expr::Integer(
                170_141_183_460_469_231_731_687_303_715_884_105_728,
            )),
        ),
    );
    let maximum_u128 = function(
        "maximum_u128",
        vec![vec![]],
        Type::U128,
        Expr::Integer(340_282_366_920_938_463_463_374_607_431_768_211_455),
    );
    let ir = compile(&Program::new(vec![
        minimum_i64,
        minimum_i128,
        maximum_u128,
        main,
    ]))
    .unwrap();
    assert!(ir.contains("sub i64 0, 9223372036854775808"));
    assert!(ir.contains("sub i128 0, -170141183460469231731687303715884105728"));
    assert!(ir.contains("ret i128 -1"));
    assert!(ir.contains("sub i32 0, 2147483648"));
}

#[test]
fn infers_if_integer_literal_from_the_other_branch() {
    let choose = function(
        "choose",
        vec![vec![param("flag", Type::Bool), param("wide", Type::I64)]],
        Type::I64,
        Expr::If {
            condition: Box::new(Expr::Name("flag".into())),
            then_branch: Box::new(Expr::Block(
                vec![],
                Some(Box::new(Expr::Name("wide".into()))),
            )),
            else_branch: Some(Box::new(Expr::Block(
                vec![],
                Some(Box::new(Expr::Integer(0))),
            ))),
        },
    );
    let main = function("main", vec![vec![]], Type::I32, Expr::Integer(0));
    let ir = compile(&Program::new(vec![choose, main])).unwrap();
    assert!(ir.contains("phi i64"));
}

#[test]
fn tracks_explicit_move_even_for_a_copy_type() {
    let consume = function(
        "consume",
        vec![vec![Param {
            mode: PassMode::Move,
            access: None,
            modifiers: Vec::new(),
            region: None,
            name: "value".into(),
            ty: Type::I32,
        }]],
        Type::I32,
        Expr::Name("value".into()),
    );
    let main = function(
        "main",
        vec![vec![]],
        Type::I32,
        Expr::Block(
            vec![
                Stmt::Let(Binding {
                    value_source: None,
                    mutable: false,
                    name: "value".into(),
                    annotation: None,
                    value: Expr::Integer(7),
                }),
                Stmt::Let(Binding {
                    value_source: None,
                    mutable: false,
                    name: "consumed".into(),
                    annotation: None,
                    value: Expr::Call(
                        Box::new(Expr::Name("consume".into())),
                        vec![arg(Expr::Name("value".into()))],
                    ),
                }),
            ],
            Some(Box::new(Expr::Name("value".into()))),
        ),
    );
    let errors = compile(&Program::new(vec![consume, main])).unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn source_backed_copy_nominals_support_reads_and_parameter_modes() {
    compile_text(
        r#"
let pair = struct { left: i32, right: i32 }
extend(pair, copyable) {}
let inferred(value: pair): i32 = { value.left + value.right }
let explicit(copy value: pair): i32 = { value.left + value.right }

let main(): i32 = {
  let pair = pair { left: 19, right: 23 }
  let duplicate = pair
  let first = inferred(pair)
  let second = explicit(pair)
  if first == 42 && second == 42 && duplicate.left + pair.right == 42 { 42 } else { 0 }
}
"#,
    )
    .expect("a validated core copyable implementation must preserve its source place");
}

#[test]
fn source_backed_drop_is_exclusive_local_and_not_directly_callable() {
    let conflict = compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, copyable) {}
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("copyable and droppable must be mutually exclusive");
    assert!(conflict
        .iter()
        .any(|error| error.message.contains("both `copyable` and `droppable`")));

    let orphan = compile_resolved_with_origins(
        r#"
pub let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("droppable must obey the nominal ownership rule");
    assert!(orphan.iter().any(|error| error
        .message
        .contains("must be implemented in the package that defines the type")));

    let direct = compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = {
  let mut value = resource { value: 0 }
  value.drop()
  0
}
"#,
    )
    .expect_err("droppable.drop must remain compiler-only");
    assert!(direct
        .iter()
        .any(|error| error.message.contains("cannot be called directly")));
}

#[test]
fn ordinary_trait_implementations_obey_the_package_orphan_rule() {
    let concrete = compile_with_origins(
        r#"
pub let read = trait {
  let read(self: borrow(self))(): i32
}
pub let foreign = struct { value: i32 }
extend(foreign, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["traits"]),
            origin(2, &["types"]),
            origin(3, &["consumer"]),
            origin(3, &["consumer"]),
        ],
    )
    .expect_err("a third package cannot join a foreign trait and type");
    assert!(concrete.iter().any(|error| error
        .message
        .contains("package that defines the trait or the type")));

    let generic = compile_with_origins(
        r#"
pub let read = trait {
  let read(self: borrow(self))(): i32
}
pub let cell(comptime t: type) = struct { value: t }
extend(cell(t), read) {
  let read(self: borrow(self))(): i32 = { 0 }
}
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["traits"]),
            origin(2, &["types"]),
            origin(3, &["consumer"]),
            origin(3, &["consumer"]),
        ],
    )
    .expect_err("blanket implementations must obey the same orphan rule");
    assert!(generic.iter().any(|error| error
        .message
        .contains("package that defines the trait or the type")));
}

#[test]
fn emits_recursive_drop_glue_from_source_backed_drop() {
    let source = r#"
let resource = struct { value: i32 }
let wrapper = struct { resource: resource, plain: i32 }
let choice = enum { some(wrapper), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { self.value = 0 }}
let main(): i32 = {
  let value = choice.some(wrapper { resource: resource { value: 42 }, plain: 1 })
  0
}
"#;
    let program = resolve_text(source);
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("drop source must lower");
    assert!(analyzer.diagnostics.is_empty());
    let resource = Ty::Struct("resource".to_owned());
    let wrapper = Ty::Struct("wrapper".to_owned());
    let choice = Ty::Enum("choice".to_owned());
    assert!(hir.needs_drop(&resource));
    assert!(hir.needs_drop(&wrapper));
    assert!(hir.needs_drop(&choice));
    let drop_method = hir.drop_methods[&resource].clone();

    let ir = compile(&program).expect("recursive drop glue must emit");
    assert!(ir.matches("define internal void @sali.drop.").count() >= 3);
    assert!(ir.contains(&format!(
        "define internal void @{}(ptr %value)",
        drop_glue_symbol(&resource)
    )));
    assert!(ir.contains(&format!(
        "call void @{}(ptr %value)",
        function_symbol(&drop_method)
    )));
    assert!(ir.contains(&format!(
        "call void @{}(ptr %field.0)",
        drop_glue_symbol(&resource)
    )));
    assert!(ir.contains("switch i32 %tag, label %done"));
    assert!(ir.contains("alloca i1 ; drop flag"));
    assert!(ir.contains(&format!("call void @{}(ptr", drop_glue_symbol(&choice))));
}

#[test]
fn permits_struct_and_enum_projection_drop_but_keeps_custom_drop_complete() {
    compile_text(
        r#"
let resource = struct { value: i32 }
let wrapper = struct { resource: resource, plain: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = {
  let wrapper = wrapper { resource: resource { value: 1 }, plain: 0 }
  let resource = wrapper.resource
  0
}
"#,
    )
    .expect("a field with droppable may move out of a containing structural wrapper");

    let custom_drop_field = compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let consume_i32(move value: i32): () = { () }
let main(): i32 = {
  let resource = resource { value: 1 }
  consume_i32(resource.value)
  0
}
"#,
    )
    .expect_err("custom droppable storage must remain complete");
    assert!(custom_drop_field
        .iter()
        .any(|error| error.message.contains("type with custom droppable")));

    compile_text(
        r#"
let resource = struct { value: i32 }
let choice = enum { some(resource), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { choice.some(resource { value: 1 }) match {
  some(resource) => 1,
  none => 0
} }
"#,
    )
    .expect("a direct enum payload with droppable may move into an unguarded binding");

    let custom_enum_move = compile_text(
        r#"
let resource = struct { value: i32 }
let choice = enum { some(resource), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
extend(choice, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { choice.some(resource { value: 1 }) match {
  some(resource) => 1,
  none => 0
} }
"#,
    )
    .expect_err("custom droppable enum storage must remain complete");
    assert!(custom_enum_move
        .iter()
        .any(|error| error.message.contains("implements `droppable`")));

    compile_text(
        r#"
let choice = enum { some(i32), none }
extend(choice, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { choice.some(1) match {
  whole if true => 1,
  _ => 0
} }
"#,
    )
    .expect("a guarded whole-value binding may preserve custom droppable ownership");

    compile_text(
        r#"
let resource = struct { value: i32 }
let choice = enum { some(resource), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { choice.some(resource { value: 1 }) match {
  some(resource) if false => 1,
  some(_) => 2,
  none => 0
} }
"#,
    )
    .expect("a guarded payload move remains speculative until its guard succeeds");

    let nested_custom_drop = compile_text(
        r#"
let resource = struct { value: i32 }
let wrapper = struct { resource: resource }
let choice = enum { some(wrapper), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
extend(wrapper, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let main(): i32 = { choice.some(wrapper { resource: resource { value: 1 } }) match {
  some(wrapper(resource: resource)) => 1,
  none => 0
} }
"#,
    )
    .expect_err("a nested custom droppable aggregate must remain complete");
    assert!(nested_custom_drop.iter().any(|error| {
        error.message.contains("nested pattern binding")
            && error.message.contains("implements `droppable`")
    }));
}

#[test]
fn explicit_move_still_consumes_a_copy_nominal() {
    let errors = compile_text(
        r#"
let pair = struct { left: i32, right: i32 }
extend(pair, copyable) {}
let consume(move value: pair): i32 = { value.left + value.right }
let main(): i32 = {
  let pair = pair { left: 19, right: 23 }
  let answer = consume(pair)
  answer + pair.left
}
"#,
    )
    .expect_err("an explicit move must override nominal copyable semantics");
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn reinitializes_a_moved_root_and_preserves_rhs_ordering() {
    compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = boxed { value: 21 }
  let first = consume(boxed)
  boxed = boxed { value: 21 }
  first + consume(boxed)
}
"#,
    )
    .expect("a mutable owned root may be initialized after a move");

    let errors = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = boxed { value: 42 }
  consume(boxed)
  boxed = boxed
  boxed.value
}
"#,
    )
    .expect_err("the assignment rhs must observe the moved state");
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn rebuilds_a_moved_struct_field_by_field() {
    compile_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let consume(move pair: pair): i32 = { pair.left.value + pair.right.value }
let main(): i32 = {
  let mut pair = pair { left: payload { value: 1 }, right: payload { value: 2 } }
  let old = consume(pair)
  pair.left = payload { value: 19 }
  let restored = pair.left.value
  pair.right = payload { value: 23 }
  old + restored + consume(pair) - 3 - 19
}
"#,
    )
    .expect("restoring every normalized leaf must restore its ancestors");

    let errors = compile_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let consume(move pair: pair): () = { () }
let main(): i32 = {
  let mut pair = pair { left: payload { value: 1 }, right: payload { value: 2 } }
  consume(pair)
  pair.left = payload { value: 42 }
  pair.left.value + pair.right.value
}
"#,
    )
    .expect_err("an incompletely rebuilt root must remain unavailable");
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn joins_reinitialization_across_branches_exactly() {
    compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): () = { () }
let choose(flag: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  consume(boxed)
  if flag { boxed = boxed { value: 19 } } else { boxed = boxed { value: 23 } }
  boxed.value
}
let main(): i32 = { choose(true) + choose(false) }
"#,
    )
    .expect("reinitialization on every reachable branch restores the value");

    let errors = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): () = { () }
let choose(flag: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  consume(boxed)
  if flag { boxed = boxed { value: 42 } }
  boxed.value
}
let main(): i32 = { choose(true) }
"#,
    )
    .expect_err("one restoring branch leaves a possible uninitialized state");
    assert!(errors
        .iter()
        .any(|error| error.message.contains("possibly moved")));

    let errors = compile_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let consume_payload(move payload: payload): () = { () }
let consume_pair(move pair: pair): () = { () }
let choose(flag: bool): () = {
  let pair = pair { left: payload { value: 19 }, right: payload { value: 23 } }
  if flag { consume_payload(pair.left) } else { consume_payload(pair.right) }
  consume_pair(pair)
}
let main(): i32 = { choose(true); 0 }
"#,
    )
    .expect_err("the root is unavailable on both correlated alternatives");
    assert!(errors
        .iter()
        .any(|error| error.message == "use of moved or uninitialized value"));
    assert!(!errors
        .iter()
        .any(|error| error.message.contains("possibly moved")));
}

#[test]
fn bounds_initialization_alternatives_with_a_conservative_widening() {
    let alternatives = (0..=MAX_INITIALIZATION_ALTERNATIVES).map(|field| {
        HashSet::from([PlaceKey {
            local: 0,
            projections: vec![field],
        }])
    });
    let widened = normalize_uninitialized_alternatives(alternatives);
    assert_eq!(widened.len(), 2);
    assert!(widened[0].is_empty());
    assert_eq!(widened[1].len(), MAX_INITIALIZATION_ALTERNATIVES + 1);

    let mut entry = FlowState {
        reachable: true,
        uninitialized: widened,
        loans: HashMap::new(),
    };
    let first = PlaceKey {
        local: 0,
        projections: vec![0],
    };
    assert_eq!(
        entry.initialization_status(std::slice::from_ref(&first)),
        InitializationStatus::MaybeUninitialized
    );

    let new_leaf = PlaceKey {
        local: 0,
        projections: vec![MAX_INITIALIZATION_ALTERNATIVES + 1],
    };
    let mut backedge = entry.clone();
    for alternative in &mut backedge.uninitialized {
        alternative.insert(new_leaf.clone());
    }
    backedge.normalize_uninitialized();
    assert!(!alternative_sets_equal(
        &projected_uninitialized_alternatives(&entry, 0),
        &projected_uninitialized_alternatives(&backedge, 0),
    ));

    for alternative in &mut backedge.uninitialized {
        alternative.remove(&new_leaf);
    }
    backedge.normalize_uninitialized();
    assert!(alternative_sets_equal(
        &projected_uninitialized_alternatives(&entry, 0),
        &projected_uninitialized_alternatives(&backedge, 0),
    ));

    for alternative in &mut entry.uninitialized {
        alternative.clear();
    }
    entry.normalize_uninitialized();
    assert_eq!(entry.uninitialized, vec![HashSet::new()]);
}

#[test]
fn records_assignment_initialization_kinds_in_hir() {
    let program = crate::parser::parse(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): () = { () }
let classify(flag: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  boxed = boxed { value: 1 }
  consume(boxed)
  boxed = boxed { value: 2 }
  if flag { consume(boxed) }
  boxed = boxed { value: 42 }
  boxed.value
}
let main(): i32 = { classify(false) }
"#,
    )
    .expect("assignment-kind source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("assignment-kind hir must lower");
    let function = hir
        .functions
        .iter()
        .find(|function| function.name == "classify")
        .expect("classify hir");
    let HirExprKind::Block(statements, _) = &function.body.kind else {
        panic!("classify must lower to a block");
    };
    let assignments: Vec<_> = statements
        .iter()
        .filter_map(|statement| {
            let HirStmt::Expr(HirExpr {
                kind: HirExprKind::Assign { assignment, .. },
                ..
            }) = statement
            else {
                return None;
            };
            Some(*assignment)
        })
        .collect();
    assert_eq!(
        assignments,
        vec![
            AssignmentKind::Overwrite,
            AssignmentKind::Initialize,
            AssignmentKind::MaybeOverwrite,
        ]
    );
}

#[test]
fn accepts_a_move_reinitialize_loop_backedge() {
    compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = boxed { value: 0 }
  let mut iteration = 0
  while { iteration < 2 } {
let previous = consume(boxed)
boxed = boxed { value: previous + 21 }
iteration = iteration + 1
  }
  consume(boxed)
}
"#,
    )
    .expect("a backedge that restores its entry initialization state is sound");
}

#[test]
fn match_guards_only_move_copy_pattern_bindings() {
    let errors = compile_text(
        r#"
let payload = struct { value: i32 }
let event = enum { value(value: payload), empty }
let accept(move payload: payload): bool = { payload.value == 42 }
let classify(event: event): i32 = { event match {
  event.value(value: payload) if accept(payload) => 42,
  event.value(value: _) => 0,
  event.empty => 0,
} }
let main(): i32 = { classify(event.value(value: payload { value: 42 })) }
"#,
    )
    .expect_err("a false guard must not consume a reusable non-copyable payload");
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("guard") && error.message.contains("move") }));

    compile_text(
        r#"
let event = enum { value(value: i32), empty }
let accept(move value: i32): bool = { value == 42 }
let classify(event: event): i32 = { event match {
  event.value(value: value) if accept(value) => 42,
  event.value(value: value) => value,
  event.empty => 0,
} }
let main(): i32 = { classify(event.value(value: 42)) }
"#,
    )
    .expect("copyable payload bindings may be consumed by a guard attempt");
}

#[test]
fn copy_validation_is_structural_transitive_and_source_order_independent() {
    compile_text(
        r#"
let inner = struct { value: i32 }
let outer = struct { inner: inner }
let choice = enum { empty, value(value: outer), named(value: inner) }
let holder = struct { values: array(outer)(2) }

extend(holder, copyable) {}
extend(choice, copyable) {}
extend(outer, copyable) {}
extend(inner, copyable) {}
let main(): i32 = {
  let values = [outer { inner: inner { value: 19 } }, outer { inner: inner { value: 23 } }]
  let holder = holder { values: values }
  let duplicate = holder
  let choice = choice.value(value: holder.values[1])
  let copied = choice
  let left = holder.values[0].inner.value
  let right = duplicate.values[1].inner.value
  copied match {
value(value: _) => left + right,
_ => 0,
  }
}
"#,
    )
    .expect("copyable dependencies, enums, and arrays must validate independently of source order");
}

#[test]
fn rejects_non_structural_copy_and_does_not_generalize_concrete_instances() {
    let structural = compile_text(
        r#"
let token = struct { value: i32 }
let invalid = struct { token: token }
extend(invalid, copyable) {}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("copyable must inspect every private representation field");
    assert!(structural.iter().any(|error| {
        error.message.contains("cannot implement `copyable`")
            && error.message.contains("invalid.token")
    }));

    let concrete = compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(i32), copyable) {}
let consume(value: cell(bool)): bool = { value.value }
let main(): i32 = {
  let cell = cell(bool) { value: true }
  let answer = consume(cell)
  if answer && cell.value { 42 } else { 0 }
}
"#,
    )
    .expect_err("cell(i32): copyable must not make cell(bool) copyable");
    assert!(concrete.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn copy_diagnostics_render_concrete_generic_source_types() {
    let parameter = compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
extend(cell(i32), copyable) {}
let read(copy cell: cell(i64)): i64 = { cell.value }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("a concrete instance without its own copyable impl must be rejected");
    assert!(parameter.iter().any(|error| {
        error
            .message
            .contains("nominal type `cell(i64)` does not implement copyable")
    }));
    assert!(parameter
        .iter()
        .all(|error| !error.message.contains("$mono$type$")));

    let structural = compile_text(
        r#"
let token = struct { value: i32 }
let cell(comptime t: type) = struct { value: t }
extend(cell(token), copyable) {}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("a concrete copyable impl must still validate its instantiated fields");
    assert!(structural.iter().any(|error| {
        error
            .message
            .contains("`cell(token)` cannot implement `copyable`")
            && error.message.contains("field `cell(token).value`")
    }));
    assert!(structural
        .iter()
        .all(|error| !error.message.contains("$mono$type$")));
}

#[test]
fn reports_a_value_moved_on_only_one_if_path_as_possibly_moved() {
    let errors = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = boxed { value: 42 }
  if flag {
consume(boxed)
  }
  boxed.value
}
let main(): i32 = { choose(false) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("possibly moved")));
}

#[test]
fn discards_moves_on_an_if_path_that_returns() {
    let ir = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = boxed { value: 42 }
  if flag {
consume(boxed)
return(0)
  }
  boxed.value
}
let main(): i32 = { choose(false) }
"#,
    )
    .unwrap();
    assert!(ir.contains("call i32 @sali.fn.63686f6f7365(i1 0)"));
}

#[test]
fn reports_a_value_moved_on_both_if_paths_as_moved() {
    let errors = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = boxed { value: 42 }
  if flag {
consume(boxed)
  } else {
consume(boxed)
  }
  boxed.value
}
let main(): i32 = { choose(false) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message == "use of moved or uninitialized value"));
    assert!(!errors
        .iter()
        .any(|error| error.message.contains("possibly moved")));
}

#[test]
fn reports_a_move_on_a_short_circuit_rhs_as_possible() {
    let errors = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): bool = { boxed.value == 42 }
let choose(flag: bool): i32 = {
  let boxed = boxed { value: 42 }
  flag && consume(boxed)
  boxed.value
}
let main(): i32 = { choose(false) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("possibly moved")));
}

#[test]
fn analyzes_mutually_exclusive_if_arms_from_the_same_entry_flow() {
    let ir = compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = boxed { value: 42 }
  if flag {
consume(boxed)
  } else {
boxed.value
  }
}
let main(): i32 = { choose(true) }
"#,
    )
    .unwrap();
    assert!(ir.contains("call i32 @sali.fn.63686f6f7365(i1 1)"));
}

#[test]
fn analyzes_mutually_exclusive_match_arms_from_variant_entry_flows() {
    compile_text(
        r#"
let choice = enum {
  first,
  second,
}
let boxed = struct { value: i32 }
let consume(move boxed: boxed): i32 = { boxed.value }
let choose(choice: choice): i32 = {
  let boxed = boxed { value: 42 }
  choice match {
choice.first => consume(boxed),
choice.second => boxed.value,
  }
}
let main(): i32 = { choose(choice.first) }
"#,
    )
    .unwrap();
}

#[test]
fn carries_guard_moves_into_later_match_candidates() {
    let errors = compile_text(
        r#"
let choice = enum {
  only,
}
let boxed = struct { value: i32 }
let consume(move boxed: boxed): bool = { boxed.value == 0 }
let choose(choice: choice): i32 = {
  let boxed = boxed { value: 42 }
  choice match {
choice.only if consume(boxed) => 0,
choice.only => boxed.value,
  }
}
let main(): i32 = { choose(choice.only) }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn lifts_a_local_closure_with_a_shared_scalar_capture() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let base = 40
  let add_base = { (increment: i32) -> base + increment }
  add_base(2)
}
"#,
    )
    .unwrap();
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!(
        "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1)"
    )));
    assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
}

#[test]
fn do_is_an_immediate_function_boundary_and_break_cannot_cross_it() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let outer = 40
  let local: i32 = do {
if outer == 40 { return(outer) }
0
  }
  local + 2
}
"#,
    )
    .expect("do should lower as an immediately invoked closure");
    assert!(ir.contains("@sali.fn.5f5f636c6f737572652e"));

    let errors = compile_text(
        r#"
let main(): i32 = { loop {
  do { break(42) }
} }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("`break` cannot be used outside")));
}

#[test]
fn do_transparently_forwards_failure_unsafe_and_custom_effects() {
    compile_resolved_text(
        r#"
let throwing = core.error.throwing
let unsafe = core.unsafe.unsafety

let ui = effect
let fail(flag: bool): i32 with(throwing(bool)) = {
  if flag { throw(true) }
  40
}
let render(value: i32): i32 with(ui) = { value }
let combined(pointer: ptr(i32)): i32 with(throwing(bool), unsafe, ui) = { do {
  let attempted = fail(false)
  let value = render(attempted)
  if value == 40 { return(*pointer) }
  0
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("do should forward the complete active effect row through its closure boundary");

    let errors = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let fail(): i32 with(throwing(i64)) = { throw(1) }
let outer(): i32 with(throwing(bool)) = { do { return(fail()) } }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("requires `throwing(i64)`") }));
}

#[test]
fn algebraic_effect_operations_are_typed_and_require_their_instantiated_row() {
    compile_text(
        r#"
let state(comptime s: type) = effect {
  let get(): s
  let put(move value: s): ()
}
let read(): i32 with(state(i32)) = { state(i32).get() }
let write(value: i32): () with(state(i32)) = { state(i32).put(value) }
let main(): i32 = { 0 }
"#,
    )
    .expect("typed operations may propagate their exact effect instance");

    let missing = compile_text(
        r#"
let state(comptime s: type) = effect { let get(): s }
let read(): i32 = { state(i32).get() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("an operation cannot run in a row that omits its effect");
    assert!(missing.iter().any(|error| {
        error.message.contains("requires custom effect") && error.message.contains("state(i32)")
    }));

    let wrong_instance = compile_text(
        r#"
let state(comptime s: type) = effect { let get(): s }
let read(): i32 with(state(i64)) = { state(i32).get() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("different effect applications have different identities");
    assert!(wrong_instance.iter().any(|error| {
        error.message.contains("requires custom effect") && error.message.contains("state(i32)")
    }));
}

#[test]
fn algebraic_handlers_preserve_operation_and_frame_residual_effects() {
    compile_text(
        r#"
let io = effect
let ask = effect { let value(): i32 with(io) }
let run(): i32 with(io) = { ask.handle value { (resume) -> resume(42) } action {
  ask.value()
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("handling ask should leave an operation's io requirement active");

    compile_text(
        r#"
let supply = effect { let seed(): i32 }
let ask = effect { let value(): i32 with(supply) }
let main(): i32 = {
  supply.handle seed { (resume) -> resume(0) } action {
ask.handle value { (resume) -> resume(42) } action { ask.value() }
  }
}
"#,
    )
    .expect("an outer algebraic handler should satisfy an inner operation's residual row");

    compile_text(
        r#"
let supply = effect { let seed(): i32 }
let ask = effect { let value(): i32 with(supply) }
let request(): i32 with(ask, supply) = { ask.value() }
let inner(): i32 with(supply) = {
  ask.handle value { (resume) -> resume(42) } action { request() }
}
let main(): i32 = {
  supply.handle seed { (resume) -> resume(0) } action { inner() }
}
"#,
    )
    .expect("lexical handler capabilities should cross generated named cps frames");

    compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let supply = effect { let seed(): i32 }
let ask = effect { let value(): i32 with(supply, throwing(bool)) }
let request(): i32 with(ask, supply, throwing(bool)) = { ask.value() }
let inner(): i32 with(supply, throwing(bool)) = {
  ask.handle value { (resume) -> resume(42) } action { request() }
}
let main(): i32 = {
  let result: result(bool)(i32) = try {
supply.handle seed { (resume) -> resume(0) } action { inner() }
  }
  result ?? 0
}
"#,
    )
    .expect("failure answers should compose across nested named handler frames");

    let missing_operation_effect = compile_text(
        r#"
let io = effect
let ask = effect { let value(): i32 with(io) }
let run(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  ask.value()
} }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling one effect must not erase an operation's other effects");
    assert!(missing_operation_effect
        .iter()
        .any(|error| error.message.contains("requires custom effect `io`")));

    let missing_frame_effect = compile_text(
        r#"
let io = effect
let ask = effect { let value(): i32 }
let request(): i32 with(ask, io) = { ask.value() }
let run(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  request()
} }
let main(): i32 = { run() }
"#,
    )
    .expect_err("a specialized resumable frame must retain effects other than ask");
    assert!(missing_frame_effect
        .iter()
        .any(|error| error.message.contains("requires custom effect `io`")));

    compile_resolved_text(
        r#"
let throwing = core.error.throwing
let unsafe = core.unsafe.unsafety

let ask_unsafe = effect { let value(): i32 with(unsafe) }
let unsafe_run(): i32 = { unsafe { ask_unsafe.handle value { (resume) -> resume(42) } action {
  ask_unsafe.value()
} } }
let ask_failure = effect { let value(): i32 with(throwing(bool)) }
let throwing_run(): i32 with(throwing(bool)) = {
  ask_failure.handle value { (resume) -> resume(42) } action { ask_failure.value() }
}
let ask_frame = effect { let value(): i32 }
let throwing_request(): i32 with(ask_frame, throwing(bool)) = { ask_frame.value() }
let throwing_frame(): i32 with(throwing(bool)) = {
  ask_frame.handle value { (resume) -> resume(42) } action { throwing_request() }
}
let main(): i32 = { 0 }
"#,
    )
    .expect("unsafe and failure requirements should survive operation handling");

    let missing_unsafe = compile_resolved_text(
        r#"
let result = core.result
let unsafe = core.unsafe.unsafety

let ask = effect { let value(): i32 with(unsafe) }
let run(): i32 = { ask.handle value { (resume) -> resume(42) } action { ask.value() } }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling an operation must not authorize its unsafe requirement");
    assert!(missing_unsafe
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));

    let missing_failure = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let ask = effect { let value(): i32 with(throwing(bool)) }
let run(): i32 = { ask.handle value { (resume) -> resume(42) } action { ask.value() } }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling an operation must not erase its failure requirement");
    assert!(missing_failure.iter().any(|error| {
        error.message.contains("requires `throwing(bool)`")
            && error.message.contains("throwing(bool)")
    }));
}

#[test]
fn algebraic_effect_operations_overload_only_by_argument_names() {
    compile_text(
        r#"
let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}
let choose(): i32 with(ask) = { ask.value(left: 19) + ask.value(right: 23) }
let main(): i32 = { ask.handle value { (left, resume) -> resume(left) } value { (right, resume) -> resume(right) } action { choose() } }
"#,
    )
    .expect("named arguments and clause parameters should select effect operation overloads");

    let positional = compile_text(
        r#"
let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}
let choose(): i32 with(ask) = { ask.value(42) }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("positional arguments cannot select an overloaded operation");
    assert!(positional.iter().any(|error| error
        .message
        .contains("overloaded effect operation `value` requires named arguments")));
}

#[test]
fn algebraic_effect_function_aliases_stay_static_and_handler_local() {
    compile_text(
        r#"
let ask = effect { let value(): i32 }
let ask(): i32 with(ask) = { ask.value() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let action = ask
  let forwarded = action
  forwarded()
} }
"#,
    )
    .expect("a chained local alias should retain its statically known effectful target");

    compile_text(
        r#"
let ask = effect { let value(): i32 }
let ask(): i32 with(ask) = { ask.value() }
let consume(action: (): i32 with(ask)): i32 with(ask) = { action() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let action = ask
  consume(action)
} }
"#,
    )
    .expect("a known function argument should specialize a higher-order effectful frame");

    compile_text(
        r#"
let ask = effect { let value(): i32 }
let ask_left(): i32 with(ask) = { ask.value() }
let ask_right(): i32 with(ask) = { ask.value() }
let consume(action: (): i32 with(ask)): i32 with(ask) = { action() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let action: (): i32 with(ask) = if true { ask_left } else { ask_right }
  let forwarded = action
  consume(forwarded)
} }
"#,
    )
    .expect("an aliased dynamic target should specialize a higher-order resumable frame");

    compile_text(
        r#"
let ask = effect { let value(): i32 }
let ask_left(): i32 with(ask) = { ask.value() }
let ask_right(): i32 with(ask) = { ask.value() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let action: (): i32 with(ask) = if true { ask_left } else { ask_right }
  let escaped = action
  escaped()
} }
"#,
    )
    .expect("a dynamic selection tag may be copied into an immutable handler-local alias");

    compile_text(
        r#"
let ask = effect { let value(): i32 }
let ask_left(): i32 with(ask) = { ask.value() }
let ask_right(): i32 with(ask) = { ask.value() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let action: (): i32 with(ask) = if true { ask_left } else { ask_right }
  let other: (): i32 with(ask) = if true { ask_right } else { ask_left }
  let mut changed = action
  changed = other
  changed()
} }
"#,
    )
    .expect("mutable dynamic aliases remap assignments between equal finite target sets");

    let incompatible_alias = compile_text(
        r#"
let ask = effect { let value(): i32 }
let first(): i32 with(ask) = { ask.value() }
let second(): i32 with(ask) = { ask.value() }
let third(): i32 with(ask) = { ask.value() }
let main(): i32 = { ask.handle value { (resume) -> resume(42) } action {
  let left: (): i32 with(ask) = if true { first } else { second }
  let right: (): i32 with(ask) = if true { first } else { third }
  let mut changed = left
  changed = right
  changed()
} }
"#,
    )
    .expect_err("mutable dynamic aliases require equal finite target sets");
    assert!(incompatible_alias
        .iter()
        .any(|error| error.message.contains("incompatible target set")));

    compile_text(
        r#"
let ask = effect {
  let choose(): bool
  let value(): i32
}
let main(): i32 = { ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(40) } action {
  let left_base = 1
  let right_base = 2
  let left: (): i32 with(ask) = { () -> ask.value() + left_base }
  let right: (): i32 with(ask) = { () -> ask.value() + right_base }
  let first: (): i32 with(ask) = if true { left } else { right }
  let second: (): i32 with(ask) = if false { right } else { left }
  let combined: (): i32 with(ask) = if ask.choose() { first } else { second }
  combined()
} }
"#,
    )
    .expect("effectful union selectors forward capturing closure environments");
}

#[test]
fn dynamic_resumable_closure_selection_preserves_fn_once_consumption() {
    let errors = compile_text(
        r#"
let ask = effect { let value(): i32 }
let payload = struct { value: i32 }
let consume(move payload: payload): i32 = { payload.value }
let main(): i32 = { ask.handle value { (resume) -> resume(1) } action {
  let left_payload = payload { value: 20 }
  let right_payload = payload { value: 21 }
  let left: (): i32 with(ask) = { () -> ask.value() + consume(left_payload) }
  let right: (): i32 with(ask) = { () -> ask.value() + consume(right_payload) }
  let action: (): i32 with(ask) = if true { left } else { right }
  let first = action()
  first + action()
} }
"#,
    )
    .expect_err("a selected fn_once resumable closure cannot be invoked twice");
    assert!(errors.iter().any(|error| {
        error.message.contains("closure")
            && (error.message.contains("consumed") || error.message.contains("moved"))
    }));
}

#[test]
fn effectful_guards_inspect_noncopy_inputs_without_committing_payload_moves() {
    compile_text(
        r#"
let ask = effect { let accept(): bool }
let payload = struct { value: i32 }
let event = enum { value(value: payload), empty }
let main(): i32 = { ask.handle accept { (resume) -> resume(false) } action {
  let event = event.value(value: payload { value: 42 })
  event match {
event.value(value: _) if ask.accept() => 0,
event.value(value: _) => 42,
event.empty => 0,
  }
} }
"#,
    )
    .expect("a binding-free effectful guard may inspect a non-copyable enum input");

    compile_text(
        r#"
let ask = effect { let accept(): bool }
let payload = struct { value: i32 }
let event = enum { value(value: payload), empty }
let consume(move payload: payload): i32 = { payload.value }
let main(): i32 = { ask.handle accept { (resume) -> resume(false) } action {
  let event = event.value(value: payload { value: 42 })
  event match {
event.value(value: payload) if ask.accept() => consume(payload),
event.value(value: payload) => consume(payload),
event.empty => 0,
  }
} }
"#,
    )
    .expect("a successful guard path can commit non-copyable bindings before entering its body");

    compile_text(
        r#"
let ask = effect { let accept(): bool }
let payload = struct { value: i32 }
let event = enum { value(value: payload), empty }
let main(): i32 = { ask.handle accept { (resume) -> resume(false) } action {
  let event = event.value(value: payload { value: 42 })
  event match {
event.value(value: payload) if ask.accept() && payload.value > 0 => 1,
event.value(value: _) => 42,
event.empty => 0,
  }
} }
"#,
    )
    .expect("a suspended guard reconstructs projected non-copyable bindings from its owned input");

    let moving_guard_binding = compile_text(
        r#"
let ask = effect { let accept(): bool }
let payload = struct { value: i32 }
let event = enum { value(value: payload), empty }
let consume(move payload: payload): bool = { payload.value > 0 }
let main(): i32 = { ask.handle accept { (resume) -> resume(false) } action {
  let event = event.value(value: payload { value: 42 })
  event match {
event.value(value: payload) if ask.accept() && consume(payload) => 1,
event.value(value: _) => 42,
event.empty => 0,
  }
} }
"#,
    )
    .expect_err("a guard may inspect but not move its reconstructed payload view");
    assert!(moving_guard_binding.iter().any(|error| error
        .message
        .contains("cannot move out of a borrowed value")));
}

#[test]
fn passes_and_invokes_a_non_capturing_function_value_indirectly() {
    let ir = compile_text(
        r#"
let increment(value: i32): i32 = { value + 1 }
let apply(action: (i32): i32)(value: i32): i32 = { action(value) }
let main(): i32 = { apply(increment)(41) }
"#,
    )
    .expect("a named function should pass through a callable parameter");
    assert!(ir.contains("call i32 %"));
    assert!(ir.contains("ptr @sali.fn.696e6372656d656e74"));
}

#[test]
fn custom_marker_effects_are_nominal_and_checked_at_calls() {
    compile_text(
        r#"
let ui = effect
let render(): i32 with(ui) = { 42 }
let invoke(comptime e: effects)(action: (): i32 with(e))(): i32 with(e) = { action() }
let screen(): i32 with(ui) = { invoke(render)() }
let main(): i32 = { 0 }
"#,
    )
    .expect("a function may forward a declared marker effect");

    let missing = compile_text(
        r#"
let ui = effect
let render(): i32 with(ui) = { 42 }
let screen(): i32 = { render() }
let main(): i32 = { screen() }
"#,
    )
    .expect_err("a pure caller cannot invoke a custom-effect function");
    assert!(missing
        .iter()
        .any(|error| error.message.contains("requires custom effect `ui`")));

    let unknown = compile_text(
        r#"
let render(): i32 with(ui) = { 42 }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("custom effects are nominal declarations");
    assert!(unknown
        .iter()
        .any(|error| error.message.contains("unknown custom effect `ui`")));

    let entry = compile_text(
        r#"
let ui = effect
let main(): i32 with(ui) = { 0 }
"#,
    )
    .expect_err("the native entry point cannot leave a custom effect unhandled");
    assert!(entry.iter().any(|error| error
        .message
        .contains("cannot expose unhandled custom effects")));
}

#[test]
fn callable_effect_requirements_are_covariant_rows() {
    compile_text(
        r#"
let ui = effect
let pure(): i32 = { 42 }
let render(): i32 with(ui) = { 42 }
let accept_ui(action: (): i32 with(ui))(): i32 with(ui) = { action() }
let screen(): i32 with(ui) = {
  let widened: (): i32 with(ui) = pure
  accept_ui(pure)() + widened()
}
let main(): i32 = { 0 }
"#,
    )
    .expect("a callable with fewer requirements may fill a wider effect slot");

    let narrowing = compile_text(
        r#"
let ui = effect
let render(): i32 with(ui) = { 42 }
let accept_pure(action: (): i32)(): i32 = { action() }
let screen(): i32 with(ui) = { accept_pure(render)() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("an effectful callable cannot fill a pure slot");
    assert!(narrowing.iter().any(|error| {
        error.message.contains("expected `(): i32`")
            && error.message.contains("found `(): i32 with(ui)`")
    }));

    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let pure(): i32 = { 42 }
let accept_unsafe(action: (): i32 with(unsafe))(): i32 with(unsafe) = { action() }
let main(): i32 = { unsafe { accept_unsafe(pure)() } }
"#,
    )
    .expect("pure named functions use the same pointer abi in unsafe callable slots");
}

#[test]
fn generic_custom_effect_materializes_a_direct_capturing_action() {
    compile_resolved_text(
        r#"
let ask = effect {
  let value(): i32
}

let forward(comptime e: effects)(move action: (): i32 with(e)): i32 with(e) = {
  action()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
    let captured = 0
    forward(ask)({ ask.value() + captured })
  }
}
"#,
    )
    .expect("a concrete custom effect should materialize a generic capturing action");

    let errors = compile_resolved_text(
        r#"
let ask = effect {
  let value(): i32
}

let tell = effect {
  let value(): i32
}

let forward(comptime e: effects)(move action: (): i32 with(e)): i32 with(e) = {
  action()
}

let main(): i32 = {
  ask.handle value { (resume) -> resume(42) } action {
    forward(tell)({ ask.value() })
  }
}
"#,
    )
    .expect_err("a selected unrelated effect must not be discharged by the current handler");
    assert!(errors.iter().any(|error| {
        error.message.contains("requires custom effect `tell`")
            || error.message.contains("unhandled custom effects")
    }));
}

#[test]
fn unsafetys_are_declared_forwarded_and_handled_at_calls() {
    let ir = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32)): i32 with(unsafe) = { *pointer }
let forward(pointer: ptr(i32)): i32 with(unsafe) = { read(pointer) }
let main(): i32 = {
  let value = 42
  unsafe { forward(ptr(borrow(value))) }
}
"#,
    )
    .expect("an unsafe handler should discharge the declared effect");
    assert!(ir.contains(&format!("call i32 @{}(", function_symbol("forward"))));

    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32)): i32 with(unsafe) = { *pointer }
let main(): i32 = {
  let value = 42
  read(ptr(borrow(value)))
}
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("call to unsafe function `read` requires an `unsafe` handler")
    }));
}

#[test]
fn unsafety_checks_survive_aliasing_and_partial_application() {
    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32))(offset: i32): i32 with(unsafe) = { *pointer + offset }
let main(): i32 = {
  let value = 40
  let named = read
  let pending = named(ptr(borrow(value)))
  pending(2)
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("requires an `unsafe` handler") }));

    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32))(offset: i32): i32 with(unsafe) = { *pointer + offset }
let main(): i32 = {
  let value = 40
  let pending = read(ptr(borrow(value)))
  unsafe { pending(2) }
}
"#,
    )
    .expect("the effect is required only when the final parameter group is applied");
}

#[test]
fn unsafetys_participate_in_method_and_trait_signatures() {
    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let reader = struct { pointer: ptr(i32) }
let read = trait {
  let read(self: borrow(self))(): i32 with(unsafe)
}
extend(reader, read) {
  let read(self: borrow(self))(): i32 with(unsafe) = { *self.pointer }
}
let main(): i32 = {
  let value = 42
  let reader = reader { pointer: ptr(borrow(value)) }
  unsafe { reader.read() }
}
"#,
    )
    .expect("methods should carry their declared unsafe effect");

    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let reader = struct { pointer: ptr(i32) }
let read = trait {
  let read(self: borrow(self))(): i32 with(unsafe)
}
extend(reader, read) {
  let read(self: borrow(self))(): i32 = { unsafe { *self.pointer } }
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("signature mismatch")));
}

#[test]
fn entry_point_cannot_export_an_unsafety() {
    let errors = compile_resolved_text(
        "let unsafe = core.unsafe.unsafety\nlet main(): i32 with(unsafe) = { 42 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("`main` cannot expose an unhandled `unsafe` effect")
    }));
}

#[test]
fn effect_compile_parameters_select_pure_or_unsafe_instances() {
    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }
let forward(comptime e: effects)(value: i32): i32 with(e) = { tagged(e)(value) }
let main(): i32 = { forward(20) + forward(pure)(20) + unsafe { forward(e: unsafe)(2) } }
"#,
    )
    .expect("effect arguments should specialize the function call requirement");

    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let identity(comptime e: effects, comptime t: type)(value: t): t with(e) = { value }
let main(): i32 = { identity(20) + unsafe { identity(e: unsafe, t: i32)(22) } }
"#,
    )
    .expect("effect and type parameters should coexist in one inferred compile group");

    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }
let forward(comptime e: effects)(value: i32): i32 with(e) = { tagged(e)(value) }
let main(): i32 = { forward(unsafe)(42) }
"#,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("requires an `unsafe` handler")),
        "{errors:?}"
    );

    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(comptime e: effects)(pointer: ptr(i32)): i32 with(e) = { *pointer }
let main(): i32 = {
  let value = 42
  unsafe { read(unsafe)(ptr(borrow(value))) }
}
"#,
    )
    .expect("the selected unsafe row should authorize the generic body");

    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let read(comptime e: effects)(pointer: ptr(i32)): i32 with(e) = { *pointer }
let main(): i32 = {
  let value = 42
  read(ptr(borrow(value)))
}
"#,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("requires an `unsafe`")),
        "{errors:?}"
    );

    let errors = compile_text(
        r#"
let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }
let main(): i32 = { tagged(e: copyable)(42) }
"#,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("argument `e`")
                && error.message.contains("sort `effects`")
                && error.message.contains("`tagged`")),
        "{errors:?}"
    );

    let errors = compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let always(comptime e: effects)(value: i32): i32 with(unsafe, e) = { value }
let main(): i32 = { always(pure)(42) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));
}

#[test]
fn effect_identities_and_effect_rows_have_distinct_sorts() {
    compile_resolved_text(
        r#"
let audit = effect {}
let require(comptime identity: effect)(value: i32): i32 with(identity) = { value }
let forward(comptime row: effects)(value: i32): i32 with(row) = { value }
let main(): i32 = { forward(pure)(42) }
"#,
    )
    .expect("effect must classify identities while effects classifies rows");

    let errors = compile_resolved_text(
        r#"
let require(comptime identity: effect)(value: i32): i32 with(identity) = { value }
let main(): i32 = { require(identity: pure)(42) }
"#,
    )
    .expect_err("the empty effect row must not inhabit the singular effect sort");
    assert!(
        errors.iter().any(|diagnostic| diagnostic
            .message
            .contains("expects one `effect` identity, not an `effects` row")),
        "{errors:?}"
    );
}

#[test]
fn compile_time_argument_diagnostics_name_binders_kinds_and_owners() {
    let unconstrained = compile_text(
        r#"
let make(comptime f: (comptime t: type): type)(): i32 = { 42 }
let main(): i32 = { make() }
"#,
    )
    .expect_err("an unconstrained constructor argument must be rejected");
    assert!(unconstrained.iter().any(|error| {
        error
            .message
            .contains("cannot infer compile-time argument `f`")
            && error.message.contains("sort `(type): type`")
            && error.message.contains("for `make`")
    }));

    let wrong_arity = compile_text(
        r#"
let pair(comptime a: type, comptime b: type) = struct { left: a, right: b }
let use(comptime f: (comptime t: type): type)(): i32 = { 42 }
let main(): i32 = { use(f: pair)() }
"#,
    )
    .expect_err("a constructor with the wrong sort must be rejected");
    assert!(wrong_arity.iter().any(|error| {
        error.message.contains("argument `f`")
            && error.message.contains("expects sort `(type): type`")
            && error
                .message
                .contains("constructor `pair` has sort `(type, type): type`")
    }));

    let wrong_effect = compile_text(
        r#"
let run(comptime e: effects)(): i32 with(e) = { 42 }
let main(): i32 = { run(e: copyable)() }
"#,
    )
    .expect_err("a non-effect compile-time argument must be rejected");
    assert!(wrong_effect.iter().any(|error| {
        error.message.contains("argument `e`")
            && error.message.contains("sort `effects`")
            && error.message.contains("in `run`")
    }));
}

#[test]
fn effect_parameters_specialize_inherent_methods() {
    compile_resolved_text(
        r#"
let unsafe = core.unsafe.unsafety

let value = struct { value: i32 }
extend(value) {
  let tagged(comptime e: effects)(self: borrow(self))(): i32 with(e) = { self.value }
}
let main(): i32 = {
  let item = value { value: 42 }
  unsafe { item.tagged(unsafe)() }
}
"#,
    )
    .expect("inherent methods should retain generic effect rows");
}

#[test]
fn failure_effects_lower_to_result_boundaries_and_propagate() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let fail(flag: bool): i32 with(throwing(bool)) = { if flag { throw(true) } else { 41 } }
let forward(flag: bool): i32 with(throwing(bool)) = { fail(flag) }
let main(): i32 = {
  let result: result(bool)(i32) = try { forward(false) }
  result ?? 0
}
"#,
    )
    .expect("failure effects should use a result abi with automatic propagation");
    assert!(ir.contains("define internal %sali.type."));
}

#[test]
fn failure_and_unsafe_share_one_effect_row() {
    compile_resolved_library_text(
        r#"
let throwing = core.error.throwing
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32), fail: bool): i32 with(throwing(bool), unsafe) = {
  if fail { throw(true) }
  *pointer
}
let forward(pointer: ptr(i32), fail: bool): i32 with(throwing(bool), unsafe) = {
  read(pointer, fail) }
"#,
    )
    .expect("standard failure should propagate while unsafety remains a separate call requirement");

    let errors = compile_resolved_text(
        r#"
let throwing = core.error.throwing
let unsafe = core.unsafe.unsafety

let read(pointer: ptr(i32)): i32 with(throwing(bool), unsafe) = { *pointer }
let main(): i32 = {
  let value = 42
  read(ptr(borrow(value)))
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));
}

#[test]
fn try_handles_failure_while_forwarding_unsafe_and_custom_effects() {
    compile_resolved_text(
        r#"
let result = core.result
let unsafe = core.unsafe.unsafety

let ui = effect
let render(value: i32): i32 with(ui) = { value }
let read(pointer: ptr(i32)): i32 with(unsafe) = { *pointer }
let handle(pointer: ptr(i32)): result(bool)(i32) with(unsafe, ui) = { try {
  let value = read(pointer)
  return(render(value))
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("try should remove only failure and forward the rest of the active effect row");
}

#[test]
fn moves_named_functions_partials_and_closures_between_local_bindings() {
    let ir = compile_text(
        r#"
let add(left: i32)(right: i32): i32 = { left + right }
let main(): i32 = {
  let base = 40
  let named = add
  let add_one = named(1)
  let moved_partial = add_one
  let add_base = { (increment: i32) -> base + increment }
  let alias = add_base
  alias(moved_partial(1))
}
"#,
    )
    .unwrap();
    let closure = function_symbol("__closure.0");
    assert!(ir.contains(&format!("call i32 @{closure}(ptr")));
    assert!(ir.contains(&format!("call i32 @{}(", function_symbol("add"))));
}

#[test]
fn returns_a_concrete_partial_environment_across_a_function_boundary() {
    let ir = compile_text(
        r#"
let add(left: i32)(right: i32): i32 = { left + right }
let make() = {
  let pending = add(40)
  pending
}
let main(): i32 = {
  let pending = make()
  pending(2)
}
"#,
    )
    .unwrap();
    assert!(ir.contains(" = type { i32 }"));
    assert!(ir.contains("call %sali.type."));
    assert!(ir.contains(&format!("call i32 @{}(", function_symbol("add"))));
}

#[test]
fn rejects_using_a_callable_after_moving_it_to_an_alias() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let base = 40
  let add_base = { (increment: i32) -> base + increment }
  let alias = add_base
  alias(2)
  add_base(2)
}
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn lifts_and_repeatedly_calls_an_fn_mut_scalar_closure() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let mut value = 40
  let mut next = {
value = value + 1
value
  }
  next()
  next()
}
"#,
    )
    .unwrap();
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!("define internal i32 @{symbol}(ptr %arg.0)")));
    assert_eq!(ir.matches(&format!("call i32 @{symbol}(ptr")).count(), 2);
    assert!(ir.contains("store i32"));
}

#[test]
fn requires_a_mutable_binding_for_an_fn_mut_closure() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let mut value = 40
  let next = {
value = value + 2
value
  }
  next()
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("fn_mut") && error.message.contains("mutable") }));
}

#[test]
fn keeps_an_fn_mut_capture_mutably_borrowed_for_its_scope() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let mut value = 40
  let mut next = {
value = value + 2
value
  }
  value
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("borrowed")));
}

#[test]
fn rejects_an_fn_mut_capture_that_conflicts_with_an_existing_borrow() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let mut value = 40
  let shared = borrow(value)
  let mut next = {
value = value + 2
value
  }
  0
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("borrowed")));
}

#[test]
fn stores_and_consumes_an_fn_once_nominal_capture() {
    let ir = compile_text(
        r#"
let payload = struct { value: i32 }
let take(move payload: payload): i32 = { payload.value }
let main(): i32 = {
  let payload = payload { value: 42 }
  let invoke = { take(payload) }
  invoke()
}
"#,
    )
    .unwrap();
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!(
        "define internal i32 @{symbol}(%{} %arg.0)",
        type_symbol("payload")
    )));
    assert!(ir.contains("; closure capture"));
    assert!(ir.contains(&format!("call i32 @{symbol}(%{}", type_symbol("payload"))));
}

#[test]
fn rejects_calling_an_fn_once_closure_twice() {
    let errors = compile_text(
        r#"
let payload = struct { value: i32 }
let take(move payload: payload): i32 = { payload.value }
let main(): i32 = {
  let payload = payload { value: 42 }
  let invoke = { take(payload) }
  invoke()
  invoke()
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("fn_once") && error.message.contains("consumed") }));
}

#[test]
fn rejects_calling_a_resource_partial_application_twice() {
    let errors = compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let finish(move resource: resource)(value: i32): i32 = { value }
let main(): i32 = {
  let pending = finish(resource { value: 1 })
  pending(1)
  pending(2)
}
"#,
    )
    .expect_err("a partial with a move capture must be fn_once");
    assert!(errors.iter().any(|error| {
        error.message.contains("fn_once partial application") && error.message.contains("consumed")
    }));
}

#[test]
fn moves_an_fn_once_source_when_the_closure_is_created() {
    let errors = compile_text(
        r#"
let payload = struct { value: i32 }
let take(move payload: payload): i32 = { payload.value }
let main(): i32 = {
  let payload = payload { value: 42 }
  let invoke = { take(payload) }
  payload.value
}
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn flattens_all_groups_of_a_curried_capturing_closure() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  add(1)(1)
}
"#,
    )
    .unwrap();
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!(
        "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1, i32 %arg.2)"
    )));
    assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
}

#[test]
fn partially_applies_a_curried_capturing_closure() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  let add_one = add(1)
  add_one(1)
}
"#,
    )
    .expect("curried closures should support partial application");
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
}

#[test]
fn emits_kind_discriminated_inherent_receiver_abis() {
    let ir = compile_text(
        r#"
let counter = struct { value: i32 }
extend(counter) {
  let read(self: borrow(self))(): i32 = { self.value }
  let reset(self: borrow(mut)(self))(): () = { self.value = 0 }
  let take(move self)(): i32 = { self.value }
  let answer = 1
}
let main(): i32 = {
  let mut value = counter { value: 42 }
  let read_value = value.read()
  value.reset()
  read_value + value.take() + counter.answer
}
"#,
    )
    .unwrap();
    let read = function_symbol("counter::method::read");
    let reset = function_symbol("counter::method::reset");
    let take = function_symbol("counter::method::take");
    let answer = global_symbol("counter::constant::answer");
    assert!(ir.contains(&format!("define internal i32 @{read}(ptr %arg.0)")));
    assert!(ir.contains(&format!("define internal void @{reset}(ptr %arg.0)")));
    assert!(ir.contains(&format!(
        "define internal i32 @{take}(%sali.type.636f756e746572 %arg.0)"
    )));
    assert!(ir.contains(&format!("call i32 @{read}(ptr")));
    assert!(ir.contains(&format!("call void @{reset}(ptr")));
    assert!(ir.contains(&format!("call i32 @{take}(%sali.type.636f756e746572")));
    assert!(ir.contains(&format!("@{answer} = internal unnamed_addr constant i32 1")));
}

#[test]
fn trailing_closure_can_supply_an_ordinary_functions_first_group() {
    compile_text(
        r#"
let run(move action: (): i32): i32 = { action() }
let main(): i32 = { run { 42 } }
"#,
    )
    .expect("a trailing closure must supply an ordinary function's first runtime group");
}

#[test]
fn multiple_and_named_trailing_closures_lower_as_successive_calls() {
    for call in [
        "choose(0) { true } { 42 }",
        "choose(0) condition { true } body { 42 }",
        "choose 0 { true } { 42 }",
        "choose 0 condition { true } body { 42 }",
    ] {
        let program = crate::parser::parse(&format!(
            r#"
let choose(seed: i32)
  (move condition: (): bool)
  (move body: (): i32): i32 = {{
  if condition() {{ body() }} else {{ seed }}
}}
let main(): i32 = {{ {call} }}
"#
        ))
        .expect("multiple trailing closure source must parse");
        compile(&program).expect("multiple trailing closures must lower as successive calls");
    }

    for while_expression in [
        "while { value < 42 } { value += 1 }",
        "while condition { value < 42 } do { value += 1 }",
    ] {
        let program = crate::parser::parse(&format!(
            "let main(): i32 = {{ let mut value = 0; {while_expression}; value }}\n"
        ))
        .expect("multi-trailing-closure while source must parse");
        compile(&program).expect("multi-trailing-closure while must lower");
    }

    compile_text(
        "let main(): i32 = {\n\
           let mut value = 0\n\
           do { value += 1 } while { value < 3 }\n\
           value\n\
         }\n",
    )
    .expect("labeled do-while overload must lower");

    for if_expression in [
        "if true { 42 } { 0 }",
        "if true then { 42 } else { 0 }",
        "if false { 0 } else if true { 42 } else { 0 }",
    ] {
        let program = crate::parser::parse(&format!("let main(): i32 = {{ {if_expression} }}\n"))
            .expect("multi-trailing-closure if source must parse");
        compile(&program).expect("multi-trailing-closure if must lower");
    }
}

#[test]
fn registers_generic_trait_metadata_and_emits_static_method_dispatch() {
    let program = crate::parser::parse(
        r#"
let convert(comptime rhs: type) = trait {
  let output: type
  let convert(self: borrow(self))(move rhs: rhs): output
}
let number = struct { value: i32 }
extend(number, convert(i32)) {
  let output = i32
  let convert(self: borrow(self))(move rhs: i32): i32 = { self.value + rhs }
}
let main(): i32 = {
  let number = number { value: 40 }
  number.convert(2)
}
"#,
    )
    .expect("generic trait source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected collection diagnostics: {:?}",
        analyzer.diagnostics
    );
    let (key, implementation) = analyzer
        .trait_impls
        .iter()
        .find(|(key, _)| key.trait_ref.name == "convert")
        .expect("program convert implementation");
    assert_eq!(key.self_ty, Ty::Struct("number".into()));
    assert_eq!(key.trait_ref.name, "convert");
    assert_eq!(key.trait_ref.arguments, vec![Ty::I32]);
    assert_eq!(implementation.associated_types["output"], Ty::I32);
    let canonical = trait_method_name(key, "convert");
    assert_eq!(implementation.methods["convert"], canonical);

    analyzer.analyze().expect("concrete trait program hir");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let ir = compile(&program).expect("concrete trait program must compile");
    let symbol = function_symbol(&canonical);
    assert!(ir.contains(&format!(
        "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1)"
    )));
    assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
}

#[test]
fn higher_kinded_trait_method_signatures_validate() {
    let program = crate::parser::parse(
        r#"
		let functor = trait(comptime self: (comptime value: type): type) {
		  let map(comptime e: effects, comptime a: type, comptime b: type)(
		    move self: self(a),
		  )(
		    transform: (a): b with(e),
		  ): self(b) with(e)
		}
	let chain = trait {
	  let item: type
	  let rebind(comptime value: type): type
	  let chain(comptime e: effects, comptime u: type)(
	    move self
	  )(
	    transform: (item): u with(e)
	  ): rebind(u) with(e)
	}
	let main(): i32 = { 0 }
	"#,
    )
    .expect("higher-kinded trait source must parse");

    let analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected hkt trait diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(
        analyzer.traits["functor"].self_parameter.kind,
        Sort::TypeConstructor {
            parameter_groups: vec![vec![Sort::Type]],
        }
    );

    compile(&program).expect("higher-kinded trait declaration must compile");
}

#[test]
fn higher_kinded_trait_inheritance_requires_constructor_supertraits() {
    let source = r#"
	let functor = trait(comptime self: (comptime value: type): type) {
	  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: self(a),
	  )(
	    transform: (a): b with(e),
	  ): self(b) with(e)
	}
let applicative = trait(comptime self: (comptime value: type): type)
where self: functor {
  let pure(comptime a: type)(move value: a): self(a)}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, applicative) {
  let pure(comptime a: type)(move value: a): carrier(a) = {
carrier(a) { value: value }
  }}
let main(): i32 = { 0 }
"#;
    let errors = compile_text(source)
        .expect_err("applicative without its inherited functor implementation must fail");
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("requires `functor` for `carrier`") }));

    compile_text(
        r#"
	let functor = trait(comptime self: (comptime value: type): type) {
	  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: self(a),
	  )(
	    transform: (a): b with(e),
	  ): self(b) with(e)
	}
let applicative = trait(comptime self: (comptime value: type): type)
where self: functor {
  let pure(comptime a: type)(move value: a): self(a)}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, applicative) {
  let pure(comptime a: type)(move value: a): carrier(a) = {
carrier(a) { value: value }
  }}
	extend(carrier, functor) {
  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: carrier(a),
	  )(
	    transform: (a): b with(e),
	  ): carrier(b) with(e) = {
	    carrier(b) { value: transform(self.value) }
	  }}
let main(): i32 = { 0 }
"#,
    )
    .expect("constructor supertrait implementations should be source-order independent");
}

#[test]
fn generic_functions_accept_explicit_type_constructor_arguments() {
    let ir = compile_text(
        r#"
let monad = trait(comptime self: (comptime value: type): type) {}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, monad) {}
let keep(comptime m: (comptime value: type): type, comptime a: type)(move value: m(a)): m(a)
where m: monad = {
  value
}

let main(): i32 = {
  let kept = keep(m: carrier)(carrier(i32) { value: 42 })
  kept.value
}
"#,
    )
    .expect("generic function should accept explicit constructor arguments");
    let identity = function_instance_name(&FunctionInstanceKey {
        template: "keep".into(),
        arguments: vec![Ty::Struct(type_constructor_marker("carrier")), Ty::I32],
    });
    let symbol = function_symbol(&identity);
    assert!(ir.contains("define internal %sali.type"));
    assert!(ir.contains(&symbol));
}

#[test]
fn higher_kinded_trait_method_signatures_report_kind_errors() {
    for (source, expected) in [
        (
            r#"
let bad = trait(comptime self: (comptime value: type): type) {
  let read(move value: self): ()
}
let main(): i32 = { 0 }
"#,
            "expects 1 type arguments, found 0",
        ),
        (
            r#"
let bad = trait {
  let read(comptime e: effects)(move value: e): ()
}
let main(): i32 = { 0 }
"#,
            "cannot be used as a runtime type",
        ),
        (
            r#"
let bad = trait(comptime self: type) {
  let read(comptime e: (comptime error: type): effect)(): () with(e)
}
"#,
            "expects 1 type arguments, found 0",
        ),
    ] {
        let diagnostics = match crate::parser::parse(source) {
            Ok(program) => Analyzer::new(&program)
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>(),
            Err(error) => vec![error.message],
        };
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn constructor_trait_implementation_headers_support_marker_traits() {
    let program = crate::parser::parse(
        r#"
let higher = trait(comptime self: (comptime value: type): type) {}
let tagged(comptime tag: type) = trait(comptime self: (comptime value: type): type) {}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, higher) {}
extend(carrier, tagged(i32)) {}
let main(): i32 = { 0 }
"#,
    )
    .expect("constructor trait implementation source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected constructor trait implementation diagnostics: {:?}",
        analyzer.diagnostics
    );
    let carrier_headers = analyzer
        .constructor_trait_impl_headers
        .iter()
        .filter(|key| key.target.name == "carrier")
        .collect::<Vec<_>>();
    assert_eq!(carrier_headers.len(), 2);
    assert!(analyzer.constructor_trait_impl_headers.iter().any(|key| {
        key.target.name == "carrier"
            && key.target.parameter_count == 1
            && key.trait_ref.name == "higher"
            && key.trait_ref.arguments.is_empty()
    }));
    assert!(analyzer.constructor_trait_impl_headers.iter().any(|key| {
        key.target.name == "carrier"
            && key.trait_ref.name == "tagged"
            && key.trait_ref.arguments == vec![Ty::I32]
    }));

    compile(&program).expect("marker constructor trait implementations must compile");
}

#[test]
fn constructor_trait_implementation_methods_register_generic_templates() {
    let program = crate::parser::parse(
        r#"
	let functor = trait(comptime self: (comptime value: type): type) {
	  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: self(a),
	  )(
	    transform: (a): b with(e),
	  ): self(b) with(e)
	}
let carrier(comptime t: type) = struct { value: t }
	extend(carrier, functor) {
  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: carrier(a),
	  )(
	    transform: (a): b with(e),
	  ): carrier(b) with(e) = {
	    carrier(b) { value: transform(self.value) }
	  }}
let main(): i32 = { 0 }
"#,
    )
    .expect("constructor trait method implementation source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected constructor trait method diagnostics: {:?}",
        analyzer.diagnostics
    );
    let carrier_headers = analyzer
        .constructor_trait_impl_headers
        .iter()
        .filter(|key| key.target.name == "carrier")
        .collect::<Vec<_>>();
    assert_eq!(carrier_headers.len(), 1);
    let key = analyzer
        .constructor_trait_impl_headers
        .iter()
        .find(|key| key.target.name == "carrier")
        .expect("constructor trait impl header")
        .clone();
    let canonical = constructor_trait_method_name(&key, "map");
    assert_eq!(
        analyzer.constructor_trait_impl_methods[&key]["map"],
        canonical
    );
    assert!(analyzer.function_templates.contains_key(&canonical));
    assert_eq!(
        analyzer.function_templates[&canonical].return_type,
        Some(Type::Named(
            "carrier".into(),
            vec![Type::Named("b".into(), Vec::new())]
        ))
    );

    compile(&program).expect("constructor trait method templates must validate");
}

#[test]
fn constructor_trait_receiver_methods_dispatch_from_instances() {
    let program = crate::parser::parse(
        r#"
	let functor = trait(comptime self: (comptime value: type): type) {
	  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: self(a),
	  )(
	    transform: (a): b with(e),
	  ): self(b) with(e)
	}
let carrier(comptime t: type) = struct { value: t }
	extend(carrier, functor) {
  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: carrier(a),
	  )(
	    transform: (a): b with(e),
	  ): carrier(b) with(e) = {
	    carrier(b) { value: transform(self.value) }
	  }}
let add_one(x: i32): i32 = { x + 1 }
let main(): i32 = {
	  let value = carrier(i32) { value: 41 }.map(add_one)
  value.value
}
"#,
    )
    .expect("constructor trait dispatch source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected constructor trait dispatch diagnostics: {:?}",
        analyzer.diagnostics
    );
    let key = analyzer
        .constructor_trait_impl_headers
        .iter()
        .find(|key| key.target.name == "carrier")
        .expect("constructor trait impl header");
    let template = constructor_trait_method_name(key, "map");
    analyzer
        .analyze()
        .expect("constructor trait dispatch program must lower");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected constructor trait dispatch lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("constructor trait method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("constructor trait dispatch program must compile");
    let symbol = function_symbol(&instance);
    assert!(
        ir.lines()
            .any(|line| { line.contains(" = call ") && line.contains(&format!("@{symbol}(")) }),
        "expected call to constructor trait method instance in ir:\n{ir}"
    );
}

#[test]
fn core_option_and_result_implement_monad() {
    compile_resolved_text(
        r#"
let option = core.option
let result = core.result
let monad = std.functional.monad

let add_one(value: i32): i32 = {
  value + 1
}

let option_next(value: i32): option(i32) = {
  option(i32).some(value + 1)
}

let result_next(value: i32): result(bool)(i32) = {
  result(bool)(i32).ok(value + 2)
}

let read_option(value: option(i32)): i32 = {
  value match {
some(number) => number,
none => 0,
  }
}

let read_result(value: result(bool)(i32)): i32 = {
  value match {
ok(number) => number,
err(_) => 0,
  }
}

let main(): i32 = {
  let option_value = option(i32).some(39).flat_map(option_next)
  let result_value = result(bool)(i32).ok(1).flat_map(result_next)
  let mapped_option = option(i32).some(1).map(add_one)
  let mapped_result = result(bool)(i32).ok(2).map(add_one)
  let pure_option: option(i32) = option.pure(3)
  let pure_result: result(bool)(i32) = result.pure(4)
  let option_transform: option((i32): i32) = option.some(add_one)
  let result_transform: result(bool)((i32): i32) = result.ok(add_one)
  let applied_option = option_transform.apply(option(i32).some(5))
  let applied_result = result_transform.apply(result(bool)(i32).ok(6))
  read_option(option_value) + read_result(result_value) + read_option(mapped_option) + read_result(mapped_result) + read_option(pure_option) + read_result(pure_result) + read_option(applied_option) + read_result(applied_result) - 70
}
"#,
    )
    .expect("core option and result should dispatch monad.flat_map");
}

#[test]
fn option_and_result_common_helpers_preserve_borrows_and_forward_effects() {
    let source = r#"
let option = core.option
let result = core.result
let unsafety = core.unsafe.unsafety

let option_ref(comptime r: region)
  (value: borrow(r)(option(i32))): option(borrow(r)(i32)) = {
  value.as_ref()
}

let option_ref_mut(comptime r: region)
  (value: borrow(mut)(r)(option(i32))): option(borrow(mut)(r)(i32)) = {
  value.as_ref(mut)()
}

let result_ref(comptime r: region)
  (value: borrow(r)(result(bool)(i32))): result(borrow(r)(bool))(borrow(r)(i32)) = {
  value.as_ref()
}

let result_ref_mut(comptime r: region)
  (value: borrow(mut)(r)(result(bool)(i32))): result(borrow(mut)(r)(bool))(borrow(mut)(r)(i32)) = {
  value.as_ref(mut)()
}

let add_one(value: i32): i32 with(unsafety) = { value + 1 }
let keep_positive(value: i32): option(i32) with(unsafety) = {
  if value > 0 { option.some(value) } else { option.none }
}
let keep_result(value: i32): result(bool)(i32) with(unsafety) = {
  result.ok(value)
}
let map_error(value: bool): i32 with(unsafety) = {
  if value { 1 } else { 0 }
}
let option_fallback(): i32 with(unsafety) = { 40 }
let error_fallback(value: bool): i32 with(unsafety) = {
  if value { 41 } else { 40 }
}
let make_error(): bool with(unsafety) = { true }

let main(): i32 = { unsafe {
  let mut maybe = option.some(1)
  let mut outcome: result(bool)(i32) = result.ok(2)
  do {
    let option_shared = option_ref(maybe)
  }
  do {
    let option_mut = option_ref_mut(maybe)
  }
  do {
    let result_shared = result_ref(outcome)
  }
  do {
    let result_mut = result_ref_mut(outcome)
  }

  let states =
    if maybe.is_some() && !maybe.is_none() &&
       outcome.is_ok() && !outcome.is_err() { 1 } else { 0 }
  let mapped = option.some(1).map(add_one).and_then(keep_positive).unwrap_or(0)
  let mapped_result =
    result(bool)(i32).ok(2).map(add_one).and_then(keep_result).unwrap_or(0)
  let mapped_error =
    result(bool)(i32).err(true).map_error(map_error).err().unwrap_or(0)
  let lazy_option = option(i32).none.unwrap_or_else(option_fallback)
  let lazy_result = result(bool)(i32).err(false).unwrap_or_else(error_fallback)
  let eager_error = option(i32).none.ok_or(true).err().unwrap_or(false)
  let lazy_error = option(i32).none.ok_or_else(make_error).err().unwrap_or(false)
  let success = result(bool)(i32).ok(1).ok().unwrap_or(0)
  if eager_error && lazy_error {
    states + mapped + mapped_result + mapped_error + lazy_option + lazy_result + success - 46
  } else {
    0
  }
} }
"#;

    compile_resolved_text(source)
        .expect("option and result helpers should preserve loans and forward callback effects");
}

#[test]
fn option_borrowed_view_cannot_escape_its_source() {
    let diagnostics = compile_resolved_text(
        r#"
let option = core.option

let escape(): option(borrow(i32)) = {
  let value = option.some(42)
  value.as_ref()
}

let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("borrow")),
        "{diagnostics:?}"
    );
}

#[test]
fn constructor_trait_implementation_headers_report_current_limits() {
    for (source, expected) in [
        (
            r#"
let higher = trait(comptime self: (comptime value: type): type) {}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, higher) {}
extend(carrier, higher) {}
let main(): i32 = { 0 }
"#,
            "duplicate constructor trait implementation",
        ),
        (
            r#"
let higher = trait(comptime self: (comptime left: type, comptime right: type): type) {}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, higher) {}
let main(): i32 = { 0 }
"#,
            "expects sort `(type, type): type`",
        ),
        (
            r#"
let curried = trait(comptime self: (comptime left: type)(comptime right: type): type) {}
let flat(comptime left: type, comptime right: type) = struct { left: left, right: right }
extend(flat, curried) {}
let main(): i32 = { 0 }
"#,
            "has sort `(type, type): type`",
        ),
        (
            r#"
	let functor = trait(comptime self: (comptime value: type): type) {
	  let map(comptime e: effects, comptime a: type, comptime b: type)(
	    move self: self(a),
	  )(
	    transform: (a): b with(e),
	  ): self(b) with(e)
	}
let carrier(comptime t: type) = struct { value: t }
extend(carrier, functor) {}
let main(): i32 = { 0 }
"#,
            "requires a body",
        ),
    ] {
        let program = crate::parser::parse(source)
            .expect("constructor trait implementation error source must parse");
        let diagnostics = Analyzer::new(&program)
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>();
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn lowers_core_add_trait_to_a_static_call() {
    let program = resolve_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(number)) {
  let output = i32
  let add(self)(rhs: number): i32 = { self.value + rhs.value }
}
let main(): i32 = { number { value: 40 } + number { value: 2 } }
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::add".into(),
            arguments: vec![Ty::Struct("number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "add"));
    let ir = compile(&program).expect("core add source must compile");
    assert!(ir.contains(&format!("call i32 @{symbol}(")));
    assert!(
        ir.contains("add i32"),
        "integer addition must stay built in"
    );
}

#[test]
fn lowers_all_core_arithmetic_traits_to_their_static_methods() {
    let program = resolve_text(
        r#"
let sub = core.ops.sub
let mul = core.ops.mul
let div = core.ops.div
let rem = core.ops.rem
let number = struct { value: i32 }
extend(number, sub(number)) {
  let output = number
  let sub(self)(rhs: number): number = { number { value: self.value - rhs.value } }
}
extend(number, mul(number)) {
  let output = number
  let mul(self)(rhs: number): number = { number { value: self.value * rhs.value } }
}
extend(number, div(number)) {
  let output = number
  let div(self)(rhs: number): number = { number { value: self.value / rhs.value } }
}
extend(number, rem(number)) {
  let output = number
  let rem(self)(rhs: number): number = { number { value: self.value % rhs.value } }
}
let main(): i32 = {
  let a = number { value: 18 }
  let b = number { value: 5 }
  (((a - b) * number { value: 2 }) / number { value: 3 } % number { value: 4 }).value
}
"#,
    );
    let ir = compile(&program).expect("arithmetic trait dispatch must compile");
    for (trait_name, method) in [
        ("core::ops::arith::sub", "sub"),
        ("core::ops::arith::mul", "mul"),
        ("core::ops::arith::div", "div"),
        ("core::ops::arith::rem", "rem"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("number".into()),
            trait_ref: TraitRefKey {
                name: trait_name.into(),
                arguments: vec![Ty::Struct("number".into())],
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, method));
        assert!(
            ir.lines()
                .any(|line| line.contains(" = call ") && line.contains(&format!("@{symbol}("))),
            "expected lowered call to {method} trait method"
        );
    }
}

#[test]
fn lowers_core_eq_and_ne_to_one_borrowing_static_call() {
    let program = resolve_text(
        r#"
let eq = core.ops.eq
let number = struct { value: i32 }
extend(number, eq(number)) {
  let eq(self: borrow(self))(rhs: borrow(number)): bool = { self.value == rhs.value }
}
let main(): i32 = {
  let left = number { value: 21 }
  let right = number { value: 21 }
  if left == right && !(left != right) { 42 } else { 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::cmp::eq".into(),
            arguments: vec![Ty::Struct("number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "eq"));
    let ir = compile(&program).expect("core eq source must compile");
    assert_eq!(ir.matches(&format!("call i1 @{symbol}(")).count(), 2);
    assert!(ir.contains("xor i1"), "`!=` must negate the eq result");
}

#[test]
fn lowers_partial_ord_operators_through_four_state_results() {
    let program = resolve_text(
        r#"
let partial_ord = core.ops.partial_ord
let partial_ordering = core.ops.partial_ordering
let number = struct { value: i32, unordered: bool }
extend(number, partial_ord(number)) {
  let partial_cmp(self: borrow(self))(rhs: borrow(number)): partial_ordering = {
if self.unordered || rhs.unordered { unordered }
else if self.value < rhs.value { less }
else if self.value > rhs.value { greater }
else { equal } }
}
let main(): i32 = {
  let low = number { value: 1, unordered: false }
  let high = number { value: 2, unordered: false }
  let none = number { value: 0, unordered: true }
  if low < high && low <= high && high > low && high >= low &&
!(none < low) && !(none <= low) && !(none > low) && !(none >= low) {
42
  } else { 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::cmp::partial_ord".into(),
            arguments: vec![Ty::Struct("number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "partial_cmp"));
    let ir = compile(&program).expect("core partial_ord source must compile");
    assert_eq!(
        ir.lines()
            .filter(|line| line.contains(" call ") && line.contains(&format!("@{symbol}(")))
            .count(),
        8
    );
    assert!(ir.contains("switch i32"));
}

#[test]
fn lowers_unary_operator_traits_to_auto_static_calls() {
    let program = resolve_text(
        r#"
let neg = core.ops.neg
let not = core.ops.not
let number = struct { value: i32 }
let flag = struct { value: bool }
extend(number, neg) {
  let output = i32
  let neg(self)(): i32 = { -self.value }}
extend(flag, not) {
  let output = i32
  let not(self)(): i32 = { if self.value { 0 } else { 42 } }
}
let negate(comptime t: type)(move value: t): t where t: neg(output = t) = { -value }
let invert(comptime t: type)(move value: t): t where t: not(output = t) = { !value }
let main(): i32 = { if invert(false) {
  !flag { value: false } + -number { value: 0 } + negate(0)
} else { 0 } }
"#,
    );
    let ir = compile(&program).expect("core unary operator source must compile");
    for (ty, trait_name, method) in [
        ("number", "core::ops::arith::neg", "neg"),
        ("flag", "core::ops::bit::not", "not"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct(ty.into()),
            trait_ref: TraitRefKey {
                name: trait_name.to_owned(),
                arguments: Vec::new(),
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, method));
        assert_eq!(ir.matches(&format!("call i32 @{symbol}(")).count(), 1);
    }
}

#[test]
fn lowers_bitwise_operator_traits_and_builtin_integer_ops() {
    let program = resolve_text(
        r#"
let bit_and = core.ops.bit_and
let bit_or = core.ops.bit_or
let bit_xor = core.ops.bit_xor
let shl = core.ops.shl
let shr = core.ops.shr
let bits = struct { value: i32 }
extend(bits, bit_and(bits)) {
  let output = bits
  let bit_and(self)(rhs: bits): bits = { bits { value: self.value & rhs.value } }
}
extend(bits, bit_or(bits)) {
  let output = bits
  let bit_or(self)(rhs: bits): bits = { bits { value: self.value | rhs.value } }
}
extend(bits, bit_xor(bits)) {
  let output = bits
  let bit_xor(self)(rhs: bits): bits = { bits { value: self.value ^ rhs.value } }
}
extend(bits, shl(bits)) {
  let output = bits
  let shl(self)(rhs: bits): bits = { bits { value: self.value << rhs.value } }
}
extend(bits, shr(bits)) {
  let output = bits
  let shr(self)(rhs: bits): bits = { bits { value: self.value >> rhs.value } }
}
let mask(comptime t: type)(move left: t)(move right: t): t
where t: bit_and(t, output = t) = { left & right }
let unsigned_shift(value: u32): u32 = { value >> 2 }
let main(): i32 = {
  let value = ((((mask(bits { value: 6 })(bits { value: 3 }) | bits { value: 8 }) ^ bits { value: 3 }) << bits { value: 1 }) >> bits { value: 1 }).value
  let builtins = (6 & 3) == 2 && (2 | 8) == 10 && (10 ^ 3) == 9 &&
(9 << 1) == 18 && (-8 >> 2) == -2 && unsigned_shift(8) == 2
  if value == 9 && builtins { 42 } else { 0 }
}
"#,
    );
    let ir = compile(&program).expect("bitwise operator source must compile");
    for (trait_name, method) in [
        ("bit_and", "bit_and"),
        ("bit_or", "bit_or"),
        ("bit_xor", "bit_xor"),
        ("shl", "shl"),
        ("shr", "shr"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("bits".into()),
            trait_ref: TraitRefKey {
                name: format!("core::ops::bit::{trait_name}"),
                arguments: vec![Ty::Struct("bits".into())],
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, method));
        assert!(ir.contains(&format!("@{symbol}(")));
    }
    for instruction in [
        "and i32", "or i32", "xor i32", "shl i32", "ashr i32", "lshr i32",
    ] {
        assert!(ir.contains(instruction), "missing `{instruction}`");
    }
    assert!(ir.contains("shift.trap"));
}

#[test]
fn unary_operator_traits_report_missing_output_and_auto_move_errors() {
    let missing = compile_resolved_text(
        "let number = struct { value: i32 }\nlet main(): i32 = { (-number { value: 1 }).value }\n",
    )
    .unwrap_err();
    assert!(missing.iter().any(|error| {
        error
            .message
            .contains("no matching `neg` implementation for unary `-`")
    }));

    let mismatch = compile_resolved_text(
        r#"
let neg = core.ops.neg
let number = struct { value: i32 }
extend(number, neg) {
  let output = i32
  let neg(self)(): i32 = { -self.value }}
let main(): bool = { -number { value: 1 } }
"#,
    )
    .unwrap_err();
    assert!(mismatch.iter().any(|error| {
        error
            .message
            .contains("no matching `neg` implementation for unary `-`")
    }));

    let moved = compile_resolved_text(
        r#"
let neg = core.ops.neg
let resource = struct { value: i32 }
extend(resource, neg) {
  let output = resource
  let neg(self)(): resource = { self }}
let main(): i32 = {
  let value = resource { value: 42 }
  let negated = -value
  negated.value + value.value
}
"#,
    )
    .unwrap_err();
    assert!(moved
        .iter()
        .any(|error| error.message.contains("moved or uninitialized value")));
}

#[test]
fn builtin_integer_arithmetic_keeps_direct_signed_and_unsigned_llvm_ops() {
    let ir = compile_text(
        r#"
let signed(x: i32, y: i32): i32 = { (x - y) * (x / y) + (x % y) }
let unsigned(x: u32, y: u32): u32 = { (x / y) + (x % y) }
let main(): i32 = { signed(9, 3) }
"#,
    )
    .expect("built-in integer arithmetic must compile");

    for instruction in ["sub i32", "mul i32", "sdiv i32", "srem i32"] {
        assert!(ir.contains(instruction), "missing `{instruction}`");
    }
    assert!(ir.contains("udiv i32"));
    assert!(ir.contains("urem i32"));
    assert!(!ir.contains("trait::"));
}

#[test]
fn builtin_division_and_remainder_guard_llvm_undefined_cases() {
    let ir = compile_text(
        r#"
let signed_div(x: i32, y: i32): i32 = { x / y }
let signed_rem(x: i32, y: i32): i32 = { x % y }
let unsigned_div(x: u32, y: u32): u32 = { x / y }
let unsigned_rem(x: u32, y: u32): u32 = { x % y }
let main(): i32 = { signed_div(84, 2) + signed_rem(85, 43) }
"#,
    )
    .expect("guarded built-in division and remainder must compile");

    assert!(ir.matches("call void @llvm.trap()").count() >= 4);
    assert!(ir.contains("icmp eq i32"));
    assert!(ir.contains("-2147483648"));
    let zero_check = ir.find("icmp eq i32").unwrap();
    let trap = ir.find("call void @llvm.trap()").unwrap();
    let division = ir.find("sdiv i32").unwrap();
    assert!(zero_check < trap && trap < division);
}

#[test]
fn constant_signed_minimum_remainder_by_negative_one_is_rejected() {
    let errors = compile_text(
        r#"
let invalid: i32 = -2147483648 % -1
let main(): i32 = { invalid }
"#,
    )
    .expect_err("signed min % -1 must not reach llvm srem undefined behavior");

    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("integer arithmetic overflows `i32` during ctfe")
    }));
}

#[test]
fn a_unique_operator_candidate_must_match_the_expected_output() {
    let errors = compile_resolved_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(i32)) {
  let output = bool
  let add(self)(rhs: i32): bool = { self.value == rhs }
}
let main(): i32 = { number { value: 42 } + 42 }
"#,
    )
    .expect_err("a unique add implementation cannot ignore its expected output");

    assert!(errors.iter().any(|error| {
        error.message.contains("no matching `add` implementation")
            && error.message.contains("producing `i32`")
    }));
}

#[test]
fn uninhabited_operator_output_coerces_when_no_exact_output_exists() {
    compile_resolved_text(
        r#"
let sub = core.ops.sub
let number = struct { value: i32 }
extend(number, sub(i32)) {
  let output = never
  let sub(self)(rhs: i32): never = { loop {} }
}
let main(): i32 = { number { value: 42 } - 1 }
"#,
    )
    .expect("an uninhabited operator output must coerce to the expected type");
}

#[test]
fn exact_operator_output_takes_precedence_over_uninhabited_output() {
    let program = resolve_text(
        r#"
let sub = core.ops.sub
let number = struct { value: i32 }
extend(number, sub(i32)) {
  let output = never
  let sub(self)(rhs: i32): never = { loop {} }
}
extend(number, sub(i64)) {
  let output = i32
  let sub(self)(rhs: i64): i32 = { 42 }
}
let main(): i32 = { number { value: 42 } - 1 }
"#,
    );
    let exact = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::sub".into(),
            arguments: vec![Ty::I64],
        },
    };
    let uninhabited = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::sub".into(),
            arguments: vec![Ty::I32],
        },
    };
    let ir = compile(&program).expect("exact output must resolve the operator candidate");
    assert!(ir.contains(&format!(
        "call i32 @{}(",
        function_symbol(&trait_method_name(&exact, "sub"))
    )));
    let uninhabited_symbol = function_symbol(&trait_method_name(&uninhabited, "sub"));
    assert!(!ir
        .lines()
        .any(|line| line.contains("call ") && line.contains(&format!("@{uninhabited_symbol}("))));
}

#[test]
fn operator_candidates_probe_bindings_in_nonempty_rhs_blocks() {
    let program = resolve_text(
        r#"
let sub = core.ops.sub
let number = struct { value: i32 }
extend(number, sub(i32)) {
  let output = i32
  let sub(self)(rhs: i32): i32 = { self.value - rhs }
}
extend(number, sub(bool)) {
  let output = i32
  let sub(self)(rhs: bool): i32 = { if rhs { 42 } else { 0 } }
}
let main(): i32 = { number { value: 1 } - do {
  let flag = true
  flag
} }
"#,
    );
    let boolean = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::sub".into(),
            arguments: vec![Ty::Bool],
        },
    };
    let integer = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::sub".into(),
            arguments: vec![Ty::I32],
        },
    };
    let ir = compile(&program).expect("the rhs block type must select sub(bool)");
    let boolean_symbol = function_symbol(&trait_method_name(&boolean, "sub"));
    let integer_symbol = function_symbol(&trait_method_name(&integer, "sub"));
    assert!(ir
        .lines()
        .any(|line| line.contains("call ") && line.contains(&format!("@{boolean_symbol}("))));
    assert!(!ir
        .lines()
        .any(|line| line.contains("call ") && line.contains(&format!("@{integer_symbol}("))));
}

#[test]
fn non_add_output_participates_in_outer_generic_inference() {
    let ir = compile_resolved_text(
        r#"
let sub = core.ops.sub
let number = struct { value: i32 }
extend(number, sub(i32)) {
  let output = i64
  let sub(self)(rhs: i32): i64 = { 42 }
}
let identity(comptime t: type)(move value: t): t = { value }
let main(): i32 = {
  let answer = identity(number { value: 44 } - 2)
  if answer == 42 { 42 } else { 0 }
}
"#,
    )
    .expect("sub output must be visible to generic inference");
    let identity = function_instance_name(&FunctionInstanceKey {
        template: "identity".into(),
        arguments: vec![Ty::I64],
    });
    assert!(ir.contains(&format!("call i64 @{}(", function_symbol(&identity))));
}

#[test]
fn add_output_participates_in_outer_generic_inference() {
    let ir = compile_resolved_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(i32)) {
  let output = i32
  let add(self)(rhs: i32): i32 = { self.value + rhs }
}
let identity(comptime t: type)(move value: t): t = { value }
let main(): i32 = { identity(number { value: 40 } + 2) }
"#,
    )
    .expect("add output must be visible to generic inference");
    let identity = function_instance_name(&FunctionInstanceKey {
        template: "identity".into(),
        arguments: vec![Ty::I32],
    });
    assert!(ir.contains(&format!("call i32 @{}(", function_symbol(&identity))));
}

#[test]
fn add_literal_range_eliminates_incompatible_rhs_candidates() {
    let program = resolve_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(i32)) {
  let output = i64
  let add(self)(rhs: i32): i64 = { 0 }
}
extend(number, add(i64)) {
  let output = i64
  let add(self)(rhs: i64): i64 = { rhs }
}
let main(): i32 = {
  let answer: i64 = number { value: 0 } + do { 2147483648 }
  if answer == 2147483648 { 42 } else { 0 }
}
"#,
    );
    let i32_key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::add".into(),
            arguments: vec![Ty::I32],
        },
    };
    let i64_key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::add".into(),
            arguments: vec![Ty::I64],
        },
    };
    let i32_symbol = function_symbol(&trait_method_name(&i32_key, "add"));
    let i64_symbol = function_symbol(&trait_method_name(&i64_key, "add"));
    let ir = compile(&program).expect("large literal must select add { value: i64 }");
    assert!(ir.contains(&format!("call i64 @{i64_symbol}(")));
    assert!(!ir.contains(&format!("call i64 @{i32_symbol}(")));
}

#[test]
fn add_lowering_is_independent_of_inferred_producer_declaration_order() {
    let program = resolve_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(number)) {
  let output = number
  let add(self)(rhs: number): number = { number { value: self.value + rhs.value } }
}
let main(): i32 = {
  let answer = make() + number { value: 2 }
  answer.value
}
let make() = { number { value: 40 } }
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::arith::add".into(),
            arguments: vec![Ty::Struct("number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "add"));
    let ir = compile(&program).expect("later inferred producer must support overloaded add");
    assert!(ir.contains(&format!("call %{} @{symbol}(", type_symbol("number"))));
}

#[test]
fn builtin_add_is_independent_of_inferred_producer_declaration_order() {
    let ir = compile_text(
        r#"
let main(): i32 = { make() + 2 }
let make() = { 40 }
"#,
    )
    .expect("later inferred integer producer must support built-in add");
    assert!(ir.contains("add i32"));
}

#[test]
fn add_reports_when_no_ambiguous_candidate_has_the_expected_output() {
    let errors = compile_resolved_text(
        r#"
let add = core.ops.add
let number = struct { value: i32 }
extend(number, add(i32)) {
  let output = bool
  let add(self)(rhs: i32): bool = { false }
}
extend(number, add(i64)) {
  let output = bool
  let add(self)(rhs: i64): bool = { true }
}
let main(): i32 = { number { value: 40 } + 2 }
"#,
    )
    .expect_err("expected output must reject every add candidate");
    assert!(errors.iter().any(|error| {
        error.message.contains("no matching `add` implementation")
            && error.message.contains("producing `i32`")
    }));
}

#[test]
fn trait_method_bodies_resolve_concrete_trait_type_substitutions() {
    let ir = compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
let factory(comptime t: type) = trait {
  let output: type
  let make(self: borrow(self))(move value: t): output
}
let maker = struct { seed: i32 }
extend(maker, factory(i32)) {
  let output = cell(i32)
  let make(self: borrow(self))(move value: i32): cell(i32) = { cell(t) { value: value + self.seed } }
}
let main(): i32 = {
  let maker = maker { seed: 0 }
  maker.make(42).value
}
"#,
    )
    .expect("trait method compile-time substitutions must resolve");
    let instance = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "cell".into(),
        arguments: vec![Ty::I32],
    };
    assert!(ir.contains(&hex_name(&nominal_instance_name(&instance))));
}

#[test]
fn trait_associated_functions_dispatch_from_the_implementing_type() {
    let ir = compile_text(
        r#"
let construct(comptime t: type) = trait {
  let construct(move value: t): self
}
let number = struct { value: i32 }
extend(number, construct(i32)) {
  let construct(move value: i32): number = { number { value: value } }
}
let main(): i32 = { number.construct(42).value }
"#,
    )
    .expect("receiver-free trait function must dispatch through its implementing type");

    assert!(ir.contains("ret i32 42") || ir.contains("i32 42"));
}

#[test]
fn nominal_error_types_propagate_through_throwing() {
    let ir = compile_resolved_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let failure = struct { code: i32 }
let read(fail: bool): i32 with(throwing(failure)) = {
  if fail { throw(failure { code: 1 }) } else { 40 } }
let run(fail: bool): i32 with(throwing(failure)) = { read(fail) + 2 }
let main(): i32 = {
  let result: result(failure)(i32) = try { run(false) }
  result match { ok(value) => value, err(_) => 0 }
}
"#,
    )
    .expect("nominal errors should use the same automatic failure propagation");

    assert!(ir.contains("add i32"));
    assert!(ir.contains("switch i32"));
}

#[test]
fn inherent_methods_take_precedence_over_trait_candidates() {
    let program = crate::parser::parse(
        r#"
let answer = trait {
  let answer(self: borrow(self))(): i32
}
let number = struct { value: i32 }
extend(number, answer) {
  let answer(self: borrow(self))(): i32 = { 1 }
}
extend(number) {
  let answer(self: borrow(self))(): i32 = { self.value }
}
let main(): i32 = {
  let number = number { value: 42 }
  number.answer()
}
"#,
    )
    .expect("method precedence source must parse");
    let analyzer = Analyzer::new(&program);
    let trait_key = analyzer.trait_impls.keys().next().unwrap();
    let trait_symbol = function_symbol(&trait_method_name(trait_key, "answer"));
    let inherent_symbol = function_symbol(&inherent_method_name("number", "answer"));
    let ir = compile(&program).expect("method precedence source must compile");
    assert!(ir.contains(&format!("call i32 @{inherent_symbol}(ptr")));
    assert!(!ir.contains(&format!("call i32 @{trait_symbol}(ptr")));
}

#[test]
fn rejects_unsupported_gats_and_associated_cycles() {
    let cases = [
        (
            r#"
	let generic = trait {
	  let item(comptime t: type): type
	}
	let node = struct { value: i32 }
	extend(node, generic) {
	  let item = i32
		}
		let main(): i32 = { 0 }
		"#,
            "must name a generic type constructor",
        ),
        (
            r#"
let cycle = trait {
  let a: type
  let b: type
}
let node = struct { value: i32 }
extend(node, cycle) {
  let a = b
  let b = a
}
let main(): i32 = { 0 }
"#,
            "associated type cycle",
        ),
        (
            r#"
let broken = trait {
  let read(self: borrow(self))(): missing
}
let main(): i32 = { 0 }
"#,
            "unknown type `missing`",
        ),
        (
            r#"
let conflict(comptime t: type) = trait {
  let t: type
}
let main(): i32 = { 0 }
"#,
            "conflicts with a trait type parameter",
        ),
        (
            r#"
let read = trait {
  let read(self: borrow(self))(): i32
}
let number = struct { value: i32 }
extend(number, read) {
  let read(value: borrow(number))(): i32 = { value.value }
}
let main(): i32 = { 0 }
"#,
            "signature mismatch",
        ),
        (
            r#"
let boxed = struct { value: i32 }
let invalid_copy = trait {
  let consume(self: borrow(self))(copy value: boxed): i32
}
let main(): i32 = { 0 }
"#,
            "requires `copyable`",
        ),
        (
            r#"
let read = trait {
  let read(self: borrow(self))(): i32
}
let number = struct { value: i32 }
extend(number, read) {}
extend(number, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}
let main(): i32 = { 0 }
"#,
            "duplicate trait implementation",
        ),
    ];
    for (source, expected) in cases {
        let diagnostics = compile_text(source).expect_err("trait source must be rejected");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

#[test]
fn validates_unused_default_trait_method_bodies() {
    let errors = compile_text(
        r#"
let broken = trait {
  let value(self: borrow(self))(): i32 = { missing }
}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("unused default methods must be checked at the trait definition");
    assert!(errors
        .iter()
        .any(|error| error.message.contains("unknown name `missing`")));
}

#[test]
fn trait_copy_parameters_accept_validated_concrete_copy_nominals() {
    compile_text(
        r#"
let cell(comptime t: type) = struct { value: t }
let reader = trait {
  let read(self: borrow(self))(copy value: cell(i32)): i32
}
let host = struct { value: i32 }
extend(host, reader) {
  let read(self: borrow(self))(copy value: cell(i32)): i32 = { self.value + value.value }
}
extend(cell(i32), copyable) {}
let main(): i32 = {
  let host = host { value: 19 }
  let cell = cell(i32) { value: 23 }
  host.read(cell) + cell.value - 23
}
"#,
    )
    .expect("trait schemas must see concrete copyable implementations collected later in source");
}

#[test]
fn structural_move_accepts_resources_and_generic_relocation() {
    compile_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }
}
let relocate(comptime t: type)(move value: t): t
where t: movable = {
  value
}

let main(): i32 = {
  let value = resource { value: 42 }
  let relocated = relocate(resource)(value)
  relocated.value
}
"#,
    )
    .expect("ordinary resources must satisfy the structural movable marker");
}

#[test]
fn async_blocks_materialize_cold_state_and_tail_await_future_values() {
    compile_text(
        r#"
let main(): i32 = {
  let future = async { 42 }
  42
}
"#,
    )
    .expect("an async block without suspension must materialize cold state");

    compile_text(
        r#"
let main(): i32 = {
  let value = 42
  let future = async { value }
  value
}
"#,
    )
    .expect("a copyable async capture must be stored by value without moving its source");

    compile_text(
        r#"
let future = core.async.future

let main(): i32 = {
  let future = async { await child() }
  0
}
let child() = { async { 1 } }
"#,
    )
    .expect("tail await must materialize a suspended parent future");

    let diagnostics = compile_text(
        r#"
let main(): i32 = {
  let future = async { await 1 }
  0
}
"#,
    )
    .expect_err("await operands must implement future");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("does not implement `future`")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| !diagnostic.message.contains("$lang$")));

    compile_text(
        r#"
let main(): i32 = {
  let future = async {
    loop {
      let value = await child()
      if value == 0 { continue() } else { () }
    }
  }
  0
}
let child() = { async { 1 } }
"#,
    )
    .expect("a recurring suspended loop without a break has never output");

    let diagnostics = compile_text(
        r#"
let main(): i32 = {
  let future = async {
    while { true } {
      let value = await child()
      if value == 0 { break(1) } else { continue() }
    }
  }
  0
}
let child() = { async { 1 } }
"#,
    )
    .expect_err("a recurring async while remains unit-valued");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value-producing `break` is not allowed in a recurring async `while`")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| !diagnostic.message.contains("$async$")));

    compile_text(
        r#"
let main(): i32 = {
  let future = async {
    if true { await child() } else { 0 }
  }
  0
}
let child() = { async { 1 } }
"#,
    )
    .expect("an if branch may suspend while another branch completes immediately");

    let diagnostics = compile_text(
        r#"
let main(): i32 = {
  let future = async {
    let value = 41
    let reference: borrow(i32) = borrow(value)
    let awaited = await child()
    reference + awaited
  }
  0
}
let child() = { async { 1 } }
"#,
    )
    .expect_err("self-referential async state must be rejected");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("stored in the same future across `await`")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("cannot implement `movable`")));

    let diagnostics = compile_text(
        r#"
let poll = core.async.poll
let future = core.async.future
let number = struct {}
let flag = struct {}
extend(number, future(())) {
  let output = i32
  let poll(comptime r: region)(self: borrow(mut)(r)(self))(): poll(i32) = {
    poll(i32).ready(42)
  }
}
extend(flag, future(())) {
  let output = bool
  let poll(comptime r: region)(self: borrow(mut)(r)(self))(): poll(bool) = {
    poll(bool).ready(true)
  }
}
let main(): i32 = {
  let future = async {
    if true { await number {} } else { await flag {} }
  }
  0
}
"#,
    )
    .expect_err("control-flow await branches must agree on their output");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("must produce the same `future.output` type")));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| !diagnostic.message.contains("$async$branch$")));
}

#[test]
fn async_loop_steps_are_private_compiler_owned_nominals() {
    let program = resolve_text("let main(): i32 = { 0 }\n");
    let mut analyzer = Analyzer::new(&program);
    let step = analyzer.register_async_loop_step(
        Ty::Tuple(vec![Ty::I32, Ty::Bool]),
        Ty::Struct("output".to_owned()),
        "$test$continue".to_owned(),
        "$test$break".to_owned(),
    );

    let Ty::Enum(step_name) = &step.ty else {
        panic!("loop step must be an enum");
    };
    assert_eq!(step.carry, Ty::Tuple(vec![Ty::I32, Ty::Bool]));
    assert_eq!(step.output, Ty::Struct("output".to_owned()));
    assert_eq!(analyzer.enum_order.last(), Some(step_name));
    assert_eq!(
        analyzer.nominal_accesses[step_name].visibility,
        Visibility::Private
    );
    let layout = &analyzer.enum_layouts[step_name];
    assert_eq!(layout.variants.len(), 2);
    assert_eq!(layout.variants[0].name, "iteration_skip");
    assert_eq!(layout.variants[0].payload_offset, 0);
    assert_eq!(layout.variants[0].fields[0].ty, step.carry);
    assert_eq!(layout.variants[1].name, "loop_exit");
    assert_eq!(layout.variants[1].payload_offset, 1);
    assert_eq!(layout.variants[1].fields[0].ty, step.output);
}

#[test]
fn cold_async_future_polls_to_the_standard_ready_variant() {
    compile_text(
        r#"
let poll = core.async.poll
let future = core.async.future

let poll_once(comptime e: effects, comptime f: type, comptime t: type)
  (future: borrow(mut)(f)): poll(t) with(e)
where f: future(e, output = t) = {
  future.poll()
}

let main(): i32 = {
  let mut future = async { 42 }
  let result: poll(i32) = poll_once(future)
  match result
    { ready(value) -> value }
    { pending -> 0 }
}
"#,
    )
    .expect("a compiler-generated future must implement future and poll to poll.ready");
}

#[test]
fn cold_async_poll_preserves_its_residual_unsafety() {
    compile_text(
        r#"
let future = core.async.future
let unsafe = core.unsafe.unsafety

let dangerous(): i32 with(unsafe) = { unsafe { 42 } }

let main(): i32 = {
  let mut future = async { dangerous() }
  unsafe {
    match future.poll()
      { ready(value) -> value }
      { pending -> 0 }
  }
}
"#,
    )
    .expect("polling under the residual unsafety effect must compile");

    let diagnostics = compile_text(
        r#"
let future = core.async.future
let unsafe = core.unsafe.unsafety

let dangerous(): i32 with(unsafe) = { unsafe { 42 } }

let main(): i32 = {
  let mut future = async { dangerous() }
  let result = future.poll()
  0
}
"#,
    )
    .expect_err("polling outside the residual unsafety effect must fail");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("requires an `unsafe` handler")));
}

#[test]
fn tail_await_forwards_the_child_futures_unsafety() {
    compile_text(
        r#"
let future = core.async.future
let unsafe = core.unsafe.unsafety

let dangerous(): i32 with(unsafe) = { unsafe { 42 } }

let main(): i32 = { unsafe {
  let mut future = async {
    await async { dangerous() }
  }
  match future.poll()
    { ready(value) -> value }
    { pending -> 0 }
} }
"#,
    )
    .expect("tail await must forward the child future's residual unsafety effect");
}

#[test]
fn cold_async_accepts_a_captureless_residual_algebraic_effect() {
    compile_text(
        r#"
let ask = effect {
  let ask(): i32
}

let request(): i32 with(ask) = { ask.ask() }

let main(): i32 = {
  let future = async { request() }
  0
}
"#,
    )
    .expect("constructing a captureless residual future remains cold and pure");
}

#[test]
fn parameter_group_expansion_requires_a_parameters_schema() {
    let errors = compile_text(
        r#"
let bad = trait {
  let args(comptime t: type): type
  let call(comptime t: type)(...move args: args(t)): ()
}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("ordinary associated types cannot be expanded as parameter groups");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("not declared as an associated `parameters` schema")
    }));
}

#[test]
fn emits_resource_array_drop_glue_for_unconstructed_layout_fields() {
    let ir = compile_text(
        r#"
let payload = struct { value: i32 }
extend(payload, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let holder = struct { values: array(payload)(1) }
let main(): i32 = { 42 }
"#,
    )
    .expect("resource arrays are valid even when their containing layout is not constructed");
    let payload = Ty::Struct("payload".to_owned());
    let array = Ty::Array(Box::new(payload.clone()), 1);
    assert!(ir.contains(&format!(
        "define internal void @{}",
        drop_glue_symbol(&payload)
    )));
    assert!(ir.contains(&format!(
        "define internal void @{}",
        drop_glue_symbol(&array)
    )));
}

#[test]
fn substitutes_self_in_associated_function_parameters_and_results() {
    compile_text(
        r#"
let boxed = struct { value: i32 }
extend(boxed) {
  let identity(value: self): self = { value }
}
let main(): i32 = { boxed.identity(boxed { value: 42 }).value }
"#,
    )
    .unwrap();
}

#[test]
fn keeps_same_named_method_and_associated_function_symbols_distinct() {
    let ir = compile_text(
        r#"
let number = struct { raw: i32 }
extend(number) {
  let value(self: borrow(self))(): i32 = { self.raw }
  let value(): i32 = { 2 }
}
let main(): i32 = {
  let number = number { raw: 40 }
  number.value() + number.value()
}
"#,
    )
    .unwrap();
    let method = function_symbol("number::method::value");
    let function = function_symbol("number::function::value");
    assert_ne!(method, function);
    assert!(ir.contains(&format!("@{method}")));
    assert!(ir.contains(&format!("@{function}")));
}

#[test]
fn emits_dynamic_array_bounds_check_before_inbounds_gep_and_hoists_allocas() {
    let ir = compile_text(
        r#"
let read(values: array(i32)(2), index: usize): i32 = { values[index] }
let main(): i32 = { read([40, 2], 1) }
"#,
    )
    .unwrap();
    let function_start = ir.find("define internal i32 @sali.fn.72656164").unwrap();
    let function_tail = &ir[function_start..];
    let function_end = function_tail.find("\n}\n").unwrap() + 3;
    let function = &function_tail[..function_end];
    let bounds = function.find("icmp ult i64").unwrap();
    let trap = function.find("call void @llvm.trap()").unwrap();
    let gep = function.find("getelementptr inbounds").unwrap();
    assert!(bounds < trap && trap < gep);
    assert!(function.rfind("alloca").unwrap() < function.find("br i1").unwrap());

    let wrong_index = compile_text(
        "let read(values: array(i32)(2), index: i32): i32 = { values[index] }\n\
         let main(): i32 = { read([40, 2], 1) }\n",
    )
    .unwrap_err();
    assert!(
        wrong_index
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected `usize`, found `i32`")),
        "{wrong_index:?}"
    );
}

#[test]
fn instantiates_and_infers_usize_compile_parameters_from_arrays() {
    compile_text(
        r#"
let identity(comptime l: usize)
  (move values: array(i32)(l)): array(i32)(l) = { values }
let main(): i32 = {
  let explicit = identity(2)([19, 1])
  let inferred = identity([20, 2])
  explicit[0] + inferred[0]
}
"#,
    )
    .expect("usize parameters should instantiate explicitly and infer from array arguments");

    let errors = compile_text(
        r#"
let same(comptime l: usize)
  (left: array(i32)(l), right: array(i32)(l)): () = { () }
let main(): i32 = { same([1], [2, 3]); 0 }
"#,
    )
    .expect_err("one usize parameter cannot infer two different array lengths");
    assert!(errors.iter().any(|error| {
        error.message.contains("conflicting inference")
            && error.message.contains("usize")
            && error.message.contains("l")
    }));
}

#[test]
fn rejects_array_lengths_beyond_the_first_version_limit() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let values: array(i32)(2147483648) = [42]
  0
}
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("array length") && error.message.contains("limit")
    }));
}

#[test]
fn rejects_an_outer_move_on_a_loop_backedge_even_for_a_copy_type() {
    let errors = compile_text(
        r#"
let consume(move value: i32): () = { () }
let main(): i32 = {
  let value = 42
  while { true } {
consume(value)
  }
  0
}
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("move") && error.message.contains("loop backedge")
    }));
}

#[test]
fn permits_a_move_that_only_reaches_a_break_exit() {
    compile_text(
        r#"
let boxed = struct { value: i32 }
let consume(move value: boxed): i32 = { value.value }
let main(): i32 = {
  let boxed = boxed { value: 42 }
  loop {
break(consume(boxed))
  }
}
"#,
    )
    .unwrap();
}

#[test]
fn nested_breaks_target_the_innermost_loop() {
    let ir = compile_text(
        r#"
let main(): i32 = {
  let mut answer = 40
  loop {
loop {
  break()
}

answer = answer + 2
break(answer)
  }
}
"#,
    )
    .unwrap();
    assert_eq!(ir.matches("loop.body").count(), 4);
    assert_eq!(ir.matches("loop.end").count(), 4);
}

#[test]
fn lowers_for_through_validated_iteration_lang_items() {
    let ir = compile_resolved_text(
        "use core.option\n\
         let iterator = core.iter.iterator
         let into_iterator = core.iter.into_iterator
         let owned_item = core.iter.owned_item
         let counter = struct { current: i32, end: i32 }\n\
         extend(counter) {\n\
           let into_iter(self: borrow(self))(): i32 = { self.current }\n\
           let next(self: borrow(self))(): bool = { false }\n\
         }\n\
         extend(counter, iterator) {\n\
           let item = owned_item(i32)\n\
           let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(i32) = {\n\
             if self.current < self.end {\n\
               let value = self.current\n\
               self.current = self.current + 1\n\
               some(value)\n\
             } else { none }\n\
           }\n}\n\
         extend(counter, into_iterator) {\n\
           let iter = counter\n\
           let into_iter(move self)(): counter = { self }\n}\n\
         let main(): i32 = {\n\
           let mut sum = 0\n\
           for counter { current: 0, end: 4 } { value ->\n\
             sum = sum + value\n\
           }\n\
           sum\n\
         }\n",
    )
    .expect("for loop must lower through into_iterator and iterator");
    assert!(ir.contains("define internal i32 @sali.fn.6d61696e"));
    for (trait_name, method) in [
        ("core::iter::iterator", "next"),
        ("core::iter::into_iterator", "into_iter"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("counter".to_owned()),
            trait_ref: TraitRefKey {
                name: trait_name.to_owned(),
                arguments: Vec::new(),
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, method));
        assert!(ir.contains(&symbol));
    }
}

#[test]
fn emits_local_non_escaping_partial_application() {
    let add = function(
        "add",
        vec![vec![param("x", Type::I32)], vec![param("y", Type::I32)]],
        Type::I32,
        Expr::Binary(
            Box::new(Expr::Name("x".into())),
            BinaryOp::Add,
            Box::new(Expr::Name("y".into())),
        ),
    );
    let main = function(
        "main",
        vec![vec![]],
        Type::I32,
        Expr::Block(
            vec![Stmt::Let(Binding {
                value_source: None,
                mutable: false,
                name: "add_one".into(),
                annotation: None,
                value: Expr::Call(
                    Box::new(Expr::Name("add".into())),
                    vec![arg(Expr::Integer(1))],
                ),
            })],
            Some(Box::new(Expr::Call(
                Box::new(Expr::Name("add_one".into())),
                vec![arg(Expr::Integer(41))],
            ))),
        ),
    );
    let ir = compile(&Program::new(vec![add, main])).unwrap();
    assert!(ir.contains("call i32 @sali.fn.616464(i32"));
}

#[test]
fn lowers_standard_optional_fields_and_methods_without_flattening() {
    let ir = compile_resolved_text(
        r#"
let option = core.option
let result = core.result

let payload = struct { value: i32, nested: option(i32) }
extend(payload) {
  let add(self: borrow(self))(amount: i32): i32 = { self.value + amount }
}
let read(value: option(payload)): option(i32) = { value?.value }
let nested(value: option(payload)): option(option(i32)) = { value?.nested }
let call(value: result(bool)(payload)): result(bool)(i32) = { value?.add(2) }
let main(): i32 = {
  let boxed = payload { value: 40, nested: option(i32).some(42) }
  let left = read(option(payload).some(boxed)) ?? 0
  let right = call(result(bool)(payload).ok(payload { value: 0, nested: option(i32).none })) ?? 0
  left + right
}
"#,
    )
    .unwrap();
    assert!(ir.contains("switch i32"));
    assert!(ir.contains(&function_symbol("payload::method::add")));
}

#[test]
fn optional_chain_expected_result_only_constrains_the_base_error_type() {
    compile_resolved_text(
        r#"
let result = core.result

let boxed = struct { value: i32 }
let read(): result(bool)(i32) = { result.ok(boxed { value: 42 })?.value }
let main(): i32 = { read() ?? 0 }
"#,
    )
    .unwrap();
}

#[test]
fn rejects_unsupported_optional_chain_receivers_and_calls() {
    let cases = [
        (
            r#"
let payload = struct { value: i32 }
let read(value: borrow(option(payload))): option(i32) = { value?.value }
let main(): i32 = { 0 }
"#,
            "owned",
        ),
        (
            r#"
let payload = struct { value: i32 }
extend(payload) { let reset(self: borrow(mut)(self))(): i32 = { self.value } }
let read(value: option(payload)): option(i32) = { value?.reset() }
let main(): i32 = { 0 }
"#,
            "mutable-borrow receiver",
        ),
        (
            r#"
let payload = struct { value: i32 }
extend(payload) { let add(self: borrow(self))(x: i32)(y: i32): i32 = { self.value + x + y } }
let read(value: option(payload)): option(i32) = { value?.add(1) }
let main(): i32 = { 0 }
"#,
            "fully applied",
        ),
        (
            r#"
let payload = struct { value: i32 }
let read(value: payload): option(i32) = { value?.value }
let main(): i32 = { 0 }
"#,
            "requires an owned `option(t)`, `result(e)(t)`, or `chain` value",
        ),
    ];
    for (source, expected) in cases {
        let source = format!("use core.option\n{source}");
        let errors = compile_resolved_text(&source).unwrap_err();
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "expected `{expected}` in {errors:?}"
        );
    }
}

#[test]
fn cleanup_plan_tracks_nested_normal_and_return_scope_exits() {
    let plan = cleanup_plan_text(
        r#"
let choose(flag: bool): i32 = {
  let outer = 40
  if true {
let inner = 2
if flag { return(outer + inner) }
  }
  outer
}
"#,
        "choose",
    );
    let lexical_scopes = plan
        .scopes
        .iter()
        .filter(|scope| scope.kind == CleanupScopeKind::Lexical)
        .count();
    assert!(lexical_scopes >= 3);
    assert!(plan.blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            Some(CleanupTerminator::Goto(edge)) if !edge.exited_scopes.is_empty()
        )
    }));
    let return_depths: Vec<_> = plan
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator {
            Some(CleanupTerminator::Return { exited_scopes }) => Some(exited_scopes.len()),
            _ => None,
        })
        .collect();
    assert!(return_depths.len() >= 2);
    assert!(return_depths.iter().any(|depth| *depth >= 3));
}

#[test]
fn cleanup_plan_builds_if_join_and_loop_break_edges() {
    let if_plan = cleanup_plan_text(
        "let choose(flag: bool): i32 = { if flag { 1 } else { 2 } }\n",
        "choose",
    );
    assert!(if_plan
        .blocks
        .iter()
        .any(|block| { matches!(block.terminator, Some(CleanupTerminator::Branch { .. })) }));
    let mut incoming_gotos = HashMap::new();
    for block in &if_plan.blocks {
        if let Some(CleanupTerminator::Goto(edge)) = &block.terminator {
            *incoming_gotos.entry(edge.target).or_insert(0_usize) += 1;
        }
    }
    assert!(incoming_gotos.values().any(|incoming| *incoming >= 2));

    let loop_plan = cleanup_plan_text(
        r#"
let choose(flag: bool): i32 = { loop {
  if flag { break(7) }
  break(9)
} }
"#,
        "choose",
    );
    let loop_scope = loop_plan
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::Loop)
        .expect("loop scope")
        .id;
    assert!(loop_plan.blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            Some(CleanupTerminator::Goto(edge)) if edge.exited_scopes.contains(&loop_scope)
        )
    }));
}

#[test]
fn cleanup_plan_materializes_resource_constructor_fields() {
    let plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let spin(): i32 = { loop {} }
let make(): boxed = { boxed { value: spin() } }
"#,
        "make",
    );
    assert!(plan.move_paths.iter().any(|path| {
        matches!(
            path.place.projections.as_slice(),
            [CleanupProjection::Field(0)]
        )
    }));
}

#[test]
fn cleanup_plan_transfers_resource_loop_break_between_scopes() {
    let plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let make(flag: bool): boxed = { loop {
  if flag { break(boxed { value: 41 }) }
  break(boxed { value: 42 })
} }
"#,
        "make",
    );
    let loop_scope = plan
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::Loop)
        .expect("loop scope")
        .id;
    let break_transfers: Vec<_> = plan
        .blocks
        .iter()
        .filter_map(|block| {
            let exits_loop = matches!(
                &block.terminator,
                Some(CleanupTerminator::Goto(edge))
                    if edge.exited_scopes.contains(&loop_scope)
            );
            exits_loop.then(|| {
                block
                    .operations
                    .iter()
                    .find_map(|operation| match operation {
                        CleanupOp::Transfer {
                            source,
                            destination,
                            kind: TransferKind::Initialize,
                        } => Some((*source, *destination)),
                        _ => None,
                    })
            })?
        })
        .collect();
    assert_eq!(break_transfers.len(), 2);
    assert!(break_transfers
        .iter()
        .all(|(source, destination)| source != destination));
    assert!(break_transfers
        .windows(2)
        .all(|pair| pair[0].1 == pair[1].1));
}

#[test]
fn cleanup_plan_nested_break_abandons_the_outer_partial_value() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let make(): pair = { loop {
  break(pair { left: payload { value: 1 }, right: break(pair { left: payload { value: 2 }, right: payload { value: 3 } }) })
} }
"#,
        "make",
    );
    let loop_scope = plan
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::Loop)
        .expect("loop scope")
        .id;
    let exiting_blocks: Vec<_> = plan
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                &block.terminator,
                Some(CleanupTerminator::Goto(edge))
                    if edge.exited_scopes.contains(&loop_scope)
            )
        })
        .collect();
    assert_eq!(exiting_blocks.len(), 1);

    let partial_local = plan
        .locals
        .iter()
        .filter(|local| local.kind == CleanupLocalKind::Temporary)
        .find(|local| {
            let root = plan
                .move_paths
                .iter()
                .find(|path| path.place.local == local.id && path.place.projections.is_empty());
            let initialized_first_field = plan.move_paths.iter().any(|path| {
                path.place.local == local.id
                    && path.place.projections.as_slice() == [CleanupProjection::Field(0)]
                    && plan
                        .blocks
                        .iter()
                        .any(|block| block.operations.contains(&CleanupOp::Init(path.id)))
            });
            initialized_first_field
                && root.is_some_and(|root| {
                    plan.blocks
                        .iter()
                        .all(|block| !block.operations.contains(&CleanupOp::Init(root.id)))
                })
        })
        .expect("partially constructed outer break value");

    let exit = exiting_blocks[0];
    assert!(exit
        .operations
        .contains(&CleanupOp::StorageDead(partial_local.id)));
    let break_transfers: Vec<_> = exit
        .operations
        .iter()
        .filter_map(|operation| match operation {
            CleanupOp::Transfer {
                source,
                destination,
                kind: TransferKind::Initialize,
            } => Some((*source, *destination)),
            _ => None,
        })
        .collect();
    assert_eq!(break_transfers.len(), 1);
    let source = &plan.move_paths[break_transfers[0].0.index()];
    let destination = &plan.move_paths[break_transfers[0].1.index()];
    assert_ne!(source.place.local, partial_local.id);
    assert_eq!(
        plan.locals[source.place.local.index()].kind,
        CleanupLocalKind::Temporary
    );
    assert!(source.place.projections.is_empty());
    assert!(plan
        .blocks
        .iter()
        .any(|block| block.operations.contains(&CleanupOp::Init(source.id))));
    assert_ne!(source.place.local, destination.place.local);
    assert!(plan.blocks.iter().all(|block| {
        block.operations.iter().all(|operation| {
            !matches!(operation, CleanupOp::Transfer { source, .. }
                if plan.move_paths[source.index()].place.local == partial_local.id)
        })
    }));
}

#[test]
fn cleanup_plan_materializes_discarded_and_global_resource_reads() {
    let discarded = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let discard(move payload: payload): () = {
  payload
  ()
}
"#,
        "discard",
    );
    assert!(discarded.locals.iter().any(|local| {
        local.kind == CleanupLocalKind::Temporary
            && discarded.blocks.iter().any(|block| {
                block.operations.iter().any(|operation| {
                    matches!(operation, CleanupOp::Transfer { destination, .. }
                        if discarded.move_paths[destination.index()].place.local == local.id)
                })
            })
    }));

    let global = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let payload = payload { value: 42 }
let get(): payload = { payload }
"#,
        "get",
    );
    let return_local = global
        .locals
        .iter()
        .find(|local| local.kind == CleanupLocalKind::ReturnPlace)
        .expect("return place");
    assert!(global
        .blocks
        .iter()
        .any(|block| block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Transfer { destination, .. }
            if global.move_paths[destination.index()].place.local == return_local.id)
        })));
}

#[test]
fn cleanup_plan_marks_mutation_through_a_mutable_borrow() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload }
let replace(target: borrow(mut)(pair), move replacement: payload): () = {
  target.left = replacement
}
"#,
        "replace",
    );
    let target = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("target"))
        .expect("mutable borrow parameter");
    assert_eq!(target.ownership, CleanupLocalOwnership::MutableBorrow);
    assert!(plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::MoveOut(path)
                if plan.move_paths[path.index()].place.local != target.id)
        })
    }));
}

#[test]
fn cleanup_plan_starts_storage_for_every_planner_temporary() {
    let plan = cleanup_plan_text(
        "let choose(left: bool, right: bool): i32 = { if left { 1 } else if right { 2 } else { 3 } }\n",
        "choose",
    );
    let temporaries: Vec<_> = plan
        .locals
        .iter()
        .filter(|local| local.kind == CleanupLocalKind::Temporary)
        .map(|local| local.id)
        .collect();
    assert!(temporaries.len() >= 2);
    for temporary in temporaries {
        assert!(plan.blocks.iter().any(|block| {
            block
                .operations
                .contains(&CleanupOp::StorageLive(temporary))
        }));
    }
}

#[test]
fn cleanup_plan_try_and_throw_returns_exit_match_arm_scopes() {
    let plan = cleanup_plan_text(
        r#"
let result = core.result
let throwing = core.error.throwing

let read(fail: bool): i32 with(throwing(bool)) = { if fail { throw(true) } else { 42 } }
let propagate(fail: bool): i32 with(throwing(bool)) = {
  let item = read(fail)
  if item == 0 { throw(true) }
  item
}
let main(): i32 = {
  let result: result(bool)(i32) = try { propagate(false) }
  result ?? 0
}
"#,
        "main",
    );
    let match_arm_scopes: HashSet<_> = plan
        .scopes
        .iter()
        .filter(|scope| scope.kind == CleanupScopeKind::MatchArm)
        .map(|scope| scope.id)
        .collect();
    assert!(!match_arm_scopes.is_empty());
    assert!(plan.blocks.iter().any(|block| {
        let exits_match_arm = |edge: &CleanupEdge| {
            edge.exited_scopes
                .iter()
                .any(|scope| match_arm_scopes.contains(scope))
        };
        match &block.terminator {
            Some(CleanupTerminator::Goto(edge)) => exits_match_arm(edge),
            Some(CleanupTerminator::Branch {
                then_edge,
                else_edge,
                ..
            }) => exits_match_arm(then_edge) || exits_match_arm(else_edge),
            Some(CleanupTerminator::Return { exited_scopes }) => exited_scopes
                .iter()
                .any(|scope| match_arm_scopes.contains(scope)),
            _ => false,
        }
    }));
    assert!(plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            let destination = match operation {
                CleanupOp::Init(path) => Some(*path),
                CleanupOp::Transfer { destination, .. } => Some(*destination),
                _ => None,
            };
            destination.is_some_and(|destination| {
                plan.locals[plan.move_paths[destination.index()].place.local.index()].kind
                    == CleanupLocalKind::Pattern
            })
        })
    }));
}

#[test]
fn cleanup_plan_commits_guarded_pattern_transfers_after_variant_refinement() {
    let plan = cleanup_plan_text(
        r#"
let resource = struct { value: i32 }
let bundle = struct { left: resource, right: resource }
let choice = enum { some(bundle), none }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let consume(move value: resource): () = { () }
let inspect(move choice: choice): i32 = { choice match {
  some(bundle(left: left, right: _)) if left.value == 1 => do {
consume(left)
1
  },
  some(_) => 2,
  none => 0
} }
"#,
        "inspect",
    );
    let mut pattern_transfers = Vec::new();
    for block in &plan.blocks {
        for operation in &block.operations {
            let CleanupOp::Transfer {
                source,
                destination,
                kind: TransferKind::Initialize,
            } = operation
            else {
                continue;
            };
            if plan.locals[plan.move_paths[destination.index()].place.local.index()].kind
                == CleanupLocalKind::Pattern
            {
                pattern_transfers.push((block, *source));
            }
        }
    }
    assert_eq!(pattern_transfers.len(), 1);
    let (_, source) = pattern_transfers
        .iter()
        .find(|(_, source)| {
            plan.move_paths[source.index()].place.projections
                == [
                    CleanupProjection::Downcast(0),
                    CleanupProjection::Field(0),
                    CleanupProjection::Field(0),
                ]
        })
        .copied()
        .expect("the named `left` pattern must transfer its source field");
    assert!(plan.blocks.iter().any(|block| {
        block
            .operations
            .iter()
            .any(|operation| matches!(operation, CleanupOp::AssumeDiscriminant { variant: 0, .. }))
    }));
    assert_eq!(
        plan.move_paths[source.index()].place.projections,
        [
            CleanupProjection::Downcast(0),
            CleanupProjection::Field(0),
            CleanupProjection::Field(0),
        ]
    );
}

#[test]
fn cleanup_plan_keeps_borrow_aliases_out_of_owned_cleanup() {
    let plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let read(): i32 = {
  let value = boxed { value: 42 }
  let alias = borrow(value)
  alias.value
}
"#,
        "read",
    );
    let alias = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("alias"))
        .expect("borrow alias local");
    assert_eq!(alias.ownership, CleanupLocalOwnership::SharedBorrow);
    assert!(plan
        .move_paths
        .iter()
        .all(|path| path.place.local != alias.id));
    assert!(plan
        .blocks
        .iter()
        .any(|block| { block.operations.contains(&CleanupOp::StorageLive(alias.id)) }));
    assert!(plan
        .blocks
        .iter()
        .all(|block| { !block.operations.contains(&CleanupOp::StorageDead(alias.id)) }));
}

#[test]
fn cleanup_plan_does_not_materialize_a_borrow_as_an_owned_resource() {
    let plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let inspect(): () = {
  let value = boxed { value: 42 }
  let alias = borrow(value)
  ()
}
"#,
        "inspect",
    );
    assert!(plan
        .locals
        .iter()
        .all(|local| local.kind != CleanupLocalKind::Temporary));
    let alias = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("alias"))
        .expect("borrow alias");
    assert_eq!(alias.ownership, CleanupLocalOwnership::SharedBorrow);
    assert!(plan
        .move_paths
        .iter()
        .all(|path| path.place.local != alias.id));
}

#[test]
fn cleanup_plan_uses_struct_array_enum_and_capture_projections() {
    let enum_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let choice = enum { value(value: payload), empty }
let make(): choice = { choice.value(value: payload { value: 7 }) }
"#,
        "make",
    );
    assert!(enum_plan.blocks.iter().any(|block| {
        block
            .operations
            .iter()
            .any(|operation| matches!(operation, CleanupOp::SetDiscriminant { variant: 0, .. }))
    }));
    assert!(enum_plan.move_paths.iter().any(|path| {
        matches!(
            path.place.projections.as_slice(),
            [CleanupProjection::Downcast(0), CleanupProjection::Field(0)]
        )
    }));

    let array_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
extend(payload, copyable) {}
let make(): array(payload)(2) = { [payload { value: 1 }, payload { value: 2 }] }
"#,
        "make",
    );
    assert!(array_plan.move_paths.iter().any(|path| {
        matches!(
            path.place.projections.as_slice(),
            [CleanupProjection::ConstantIndex(1)]
        )
    }));

    let closure_plan = cleanup_plan_text(
        r#"
let run(base: i32): i32 = {
  let add = { (increment: i32) -> base + increment }
  add(1)
}
"#,
        "run",
    );
    assert!(closure_plan.move_paths.iter().any(|path| {
        matches!(
            path.place.projections.as_slice(),
            [CleanupProjection::Capture(0)]
        )
    }));
}

#[test]
fn cleanup_plan_pre_registers_complete_owned_move_path_forests() {
    let plan = cleanup_plan_text(
        r#"
let empty = struct {}
extend(empty, copyable) {}
let pair = struct { left: i32, right: empty }
extend(pair, copyable) {}
let choice = enum { first(next: pair), second(i32), unit }
let inspect(move empty: empty, move pair: pair, move choice: choice, move values: array(pair)(3), alias: borrow(pair)): () = { () }
"#,
        "inspect",
    );
    let local_paths = |name: &str| {
        let local = plan
            .locals
            .iter()
            .find(|local| local.debug_name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("missing cleanup local `{name}`"));
        plan.move_paths
            .iter()
            .filter(|path| path.place.local == local.id)
            .map(|path| path.place.projections.clone())
            .collect::<HashSet<_>>()
    };

    assert_eq!(local_paths("empty"), HashSet::from([Vec::new()]));
    assert_eq!(
        local_paths("pair"),
        HashSet::from([
            Vec::new(),
            vec![CleanupProjection::Field(0)],
            vec![CleanupProjection::Field(1)],
        ])
    );
    assert_eq!(
        local_paths("choice"),
        HashSet::from([
            Vec::new(),
            vec![CleanupProjection::Downcast(0)],
            vec![CleanupProjection::Downcast(0), CleanupProjection::Field(0),],
            vec![
                CleanupProjection::Downcast(0),
                CleanupProjection::Field(0),
                CleanupProjection::Field(0),
            ],
            vec![
                CleanupProjection::Downcast(0),
                CleanupProjection::Field(0),
                CleanupProjection::Field(1),
            ],
            vec![CleanupProjection::Downcast(1)],
            vec![CleanupProjection::Downcast(1), CleanupProjection::Field(0),],
            vec![CleanupProjection::Downcast(2)],
        ])
    );
    let expected_array_paths = std::iter::once(Vec::new())
        .chain((0..3).flat_map(|index| {
            [
                vec![CleanupProjection::ConstantIndex(index)],
                vec![
                    CleanupProjection::ConstantIndex(index),
                    CleanupProjection::Field(0),
                ],
                vec![
                    CleanupProjection::ConstantIndex(index),
                    CleanupProjection::Field(1),
                ],
            ]
        }))
        .collect::<HashSet<_>>();
    assert_eq!(local_paths("values"), expected_array_paths);
    assert!(local_paths("alias").is_empty());
}

#[test]
fn cleanup_plan_stages_field_bases_before_transfer_and_copies_index_results() {
    let field_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let take(): payload = { pair { left: payload { value: 1 }, right: payload { value: 2 } }.left }
"#,
        "take",
    );
    assert!(field_plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| match operation {
            CleanupOp::Transfer { source, .. } => {
                let path = &field_plan.move_paths[source.index()];
                field_plan.locals[path.place.local.index()].kind == CleanupLocalKind::Temporary
                    && matches!(
                        path.place.projections.as_slice(),
                        [CleanupProjection::Field(0)]
                    )
            }
            _ => false,
        })
    }));

    let index_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
extend(payload, copyable) {}
let take(): payload = { [payload { value: 1 }, payload { value: 2 }][1] }
"#,
        "take",
    );
    let staged_array = index_plan
        .locals
        .iter()
        .filter(|local| local.kind == CleanupLocalKind::Temporary)
        .find(|local| {
            index_plan.move_paths.iter().any(|path| {
                path.place.local == local.id
                    && path.place.projections == [CleanupProjection::ConstantIndex(1)]
            })
        })
        .expect("staged array temporary");
    assert!(index_plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Init(path)
                if index_plan.move_paths[path.index()].place.local == staged_array.id
                    && index_plan.move_paths[path.index()].place.projections.is_empty())
        })
    }));
    assert!(index_plan.blocks.iter().all(|block| {
        block.operations.iter().all(|operation| match operation {
            CleanupOp::MoveOut(path) | CleanupOp::Transfer { source: path, .. } => {
                let source = &index_plan.move_paths[path.index()];
                source.place.local != staged_array.id || source.place.projections.is_empty()
            }
            _ => true,
        })
    }));
}

#[test]
fn cleanup_plan_dynamic_index_uses_only_pre_registered_constant_paths() {
    let plan = cleanup_plan_text(
        "let take(values: array(i32)(3), index: usize): i32 = { values[index] }\n",
        "take",
    );
    assert!(plan.move_paths.iter().all(|path| {
        path.place
            .projections
            .iter()
            .all(|projection| !matches!(projection, CleanupProjection::Index(_)))
    }));
    let staged_array = plan
        .locals
        .iter()
        .filter(|local| local.kind == CleanupLocalKind::Temporary)
        .find(|local| {
            (0..3).all(|index| {
                plan.move_paths.iter().any(|path| {
                    path.place.local == local.id
                        && path.place.projections == [CleanupProjection::ConstantIndex(index)]
                })
            })
        })
        .expect("dynamic index stages a complete array forest");
    assert!(plan.blocks.iter().all(|block| {
        block.operations.iter().all(|operation| match operation {
            CleanupOp::MoveOut(path) | CleanupOp::Transfer { source: path, .. } => {
                let source = &plan.move_paths[path.index()];
                source.place.local != staged_array.id || source.place.projections.is_empty()
            }
            _ => true,
        })
    }));
}

#[test]
fn cleanup_plan_moves_a_resource_out_of_a_temporary_array() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let take(): payload = { [payload { value: 1 }, payload { value: 2 }][1] }
"#,
        "take",
    );
    assert!(plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Transfer { source, .. }
                if plan.move_paths[source.index()].place.projections
                    == [CleanupProjection::ConstantIndex(1)])
        })
    }));
}

#[test]
fn cleanup_plan_maps_local_array_places_to_constant_index_paths() {
    let plan = cleanup_plan_text(
        r#"
let consume(move value: i32): () = { () }
let main(): i32 = {
  let mut values = [20, 2]
  consume(values[0])
  values[0] = 40
  values[0] + values[1]
}
"#,
        "main",
    );
    let values = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("values"))
        .expect("array binding");
    let first = plan
        .move_paths
        .iter()
        .find(|path| {
            path.place.local == values.id
                && path.place.projections == [CleanupProjection::ConstantIndex(0)]
        })
        .expect("first array element move path");
    assert!(plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Transfer { destination, .. }
                if *destination == first.id)
        })
    }));
    assert!(plan.move_paths.iter().all(|path| {
        path.place.local != values.id
            || path
                .place
                .projections
                .iter()
                .all(|projection| !matches!(projection, CleanupProjection::Field(_)))
    }));
}

#[test]
fn cleanup_plan_reports_the_move_path_budget() {
    let errors =
        compile_library_text("let too_wide(move values: array(i32)(65536)): () = { () }\n")
            .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("cleanup move-path limit")
            && error.message.contains(&MAX_CLEANUP_MOVE_PATHS.to_string())
    }));
}

#[test]
fn cleanup_plan_keeps_staged_early_call_arguments_when_a_later_call_diverges() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let stop(): never = { loop {} }
let choose(move value: payload, code: i32): payload = { value }
let make(): payload = { choose(payload { value: 1 }, stop()) }
"#,
        "make",
    );
    let unreachable = plan
        .blocks
        .iter()
        .find(|block| matches!(block.terminator, Some(CleanupTerminator::Unreachable)))
        .expect("never call must end its block");
    let staged_resource = unreachable
        .operations
        .iter()
        .find_map(|operation| match operation {
            CleanupOp::Init(path)
                if plan.locals[plan.move_paths[path.index()].place.local.index()].kind
                    == CleanupLocalKind::Temporary =>
            {
                Some(*path)
            }
            _ => None,
        });
    assert!(staged_resource.is_some());
    assert!(!unreachable
        .operations
        .iter()
        .any(|operation| matches!(operation, CleanupOp::MoveOut(_))));
}

#[test]
fn cleanup_plan_does_not_overwrite_assignment_destination_when_rhs_returns() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let update(): pair = {
  let mut old = pair { left: payload { value: 0 }, right: payload { value: 1 } }
  old = pair { left: payload { value: 2 }, right: return(pair { left: payload { value: 3 }, right: payload { value: 4 } }) }
  old
}
"#,
        "update",
    );
    let old = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("old"))
        .expect("assignment destination");
    assert!(!plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Transfer { destination, .. }
                if plan.move_paths[destination.index()].place.local == old.id)
        })
    }));
    assert!(plan.blocks.iter().any(|block| {
        matches!(block.terminator, Some(CleanupTerminator::Return { .. }))
            && block.operations.contains(&CleanupOp::StorageDead(old.id))
    }));
}

#[test]
fn cleanup_plan_keeps_partial_aggregate_state_out_of_the_return_place() {
    fn assert_partial_child_without_root(plan: &CleanupPlan, child: CleanupProjection) {
        let function_scope = plan
            .scopes
            .iter()
            .find(|scope| scope.kind == CleanupScopeKind::FunctionBody)
            .expect("function scope")
            .id;
        let body_stage = plan
            .locals
            .iter()
            .find(|local| {
                local.kind == CleanupLocalKind::Temporary && local.scope == function_scope
            })
            .expect("implicit body stage");
        let root = plan
            .move_paths
            .iter()
            .find(|path| path.place.local == body_stage.id && path.place.projections.is_empty())
            .expect("body root");
        let child = plan
            .move_paths
            .iter()
            .find(|path| {
                path.place.local == body_stage.id
                    && path.place.projections.first() == Some(&child)
                    && plan
                        .blocks
                        .iter()
                        .any(|block| block.operations.contains(&CleanupOp::Init(path.id)))
            })
            .expect("partially initialized child");
        assert!(plan
            .blocks
            .iter()
            .any(|block| { block.operations.contains(&CleanupOp::Init(child.id)) }));
        assert!(plan
            .blocks
            .iter()
            .all(|block| { !block.operations.contains(&CleanupOp::Init(root.id)) }));
        assert!(plan.blocks.iter().any(|block| {
            matches!(block.terminator, Some(CleanupTerminator::Return { .. }))
                && block
                    .operations
                    .contains(&CleanupOp::StorageDead(body_stage.id))
        }));
    }

    let struct_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let pair = struct { left: payload, right: payload }
let make(): pair = { pair { left: payload { value: 1 }, right: return(pair { left: payload { value: 2 }, right: payload { value: 3 } }) } }
"#,
        "make",
    );
    assert_partial_child_without_root(&struct_plan, CleanupProjection::Field(0));

    let enum_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let choice = enum { pair(payload, payload) }
let make(): choice = { choice.pair(payload { value: 1 }, return(choice.pair(payload { value: 2 }, payload { value: 3 }))) }
"#,
        "make",
    );
    assert_partial_child_without_root(&enum_plan, CleanupProjection::Downcast(0));
    assert!(enum_plan.blocks.iter().any(|block| {
        block
            .operations
            .iter()
            .any(|operation| matches!(operation, CleanupOp::SetDiscriminant { variant: 0, .. }))
    }));

    let array_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
extend(payload, copyable) {}
let make(): array(payload)(2) = { [payload { value: 1 }, return([payload { value: 2 }, payload { value: 3 }])] }
"#,
        "make",
    );
    assert_partial_child_without_root(&array_plan, CleanupProjection::ConstantIndex(0));
}

#[test]
fn cleanup_plan_stages_implicit_and_explicit_returns_before_return_place() {
    fn return_transfer(
        plan: &CleanupPlan,
    ) -> (&crate::cleanup::MovePath, &crate::cleanup::MovePath) {
        let return_local = plan
            .locals
            .iter()
            .find(|local| local.kind == CleanupLocalKind::ReturnPlace)
            .expect("return place");
        plan.blocks
            .iter()
            .find_map(|block| {
                block
                    .operations
                    .iter()
                    .find_map(|operation| match operation {
                        CleanupOp::Transfer {
                            source,
                            destination,
                            kind: TransferKind::Initialize,
                        } if plan.move_paths[destination.index()].place.local
                            == return_local.id =>
                        {
                            Some((
                                &plan.move_paths[source.index()],
                                &plan.move_paths[destination.index()],
                            ))
                        }
                        _ => None,
                    })
            })
            .expect("staged return transfer")
    }

    let implicit = cleanup_plan_text(
        "let payload = struct { value: i32 }\nlet make(): payload = { payload { value: 1 } }\n",
        "make",
    );
    let explicit = cleanup_plan_text(
        "let payload = struct { value: i32 }\nlet make(): payload = { return(payload { value: 1 }) }\n",
        "make",
    );
    for plan in [&implicit, &explicit] {
        let (source, destination) = return_transfer(plan);
        assert_ne!(source.place.local, destination.place.local);
        assert_eq!(
            plan.locals[source.place.local.index()].kind,
            CleanupLocalKind::Temporary
        );
    }
    let function_scope = explicit
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::FunctionBody)
        .expect("function scope")
        .id;
    assert!(explicit.locals.iter().all(|local| {
        !(local.kind == CleanupLocalKind::Temporary && local.scope == function_scope)
    }));
}

#[test]
fn cleanup_plan_forwards_one_destination_through_block_if_and_match() {
    fn body_root(plan: &CleanupPlan) -> CleanupMovePathId {
        let function_scope = plan
            .scopes
            .iter()
            .find(|scope| scope.kind == CleanupScopeKind::FunctionBody)
            .expect("function scope")
            .id;
        let body_stage = plan
            .locals
            .iter()
            .find(|local| {
                local.kind == CleanupLocalKind::Temporary && local.scope == function_scope
            })
            .expect("body stage");
        plan.move_paths
            .iter()
            .find(|path| path.place.local == body_stage.id && path.place.projections.is_empty())
            .expect("body root")
            .id
    }

    let if_plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let choose(flag: bool): payload = {
  if flag { payload { value: 1 } } else { payload { value: 2 } }
}
"#,
        "choose",
    );
    let if_root = body_root(&if_plan);
    assert_eq!(
        if_plan
            .blocks
            .iter()
            .filter(|block| block.operations.contains(&CleanupOp::Init(if_root)))
            .count(),
        2
    );

    let match_plan = cleanup_plan_text(
        r#"
let choice = enum { first, second }
let payload = struct { value: i32 }
let choose(choice: choice): payload = { choice match {
  choice.first => payload { value: 1 },
  choice.second => payload { value: 2 },
} }
"#,
        "choose",
    );
    let match_root = body_root(&match_plan);
    assert_eq!(
        match_plan
            .blocks
            .iter()
            .filter(|block| block.operations.contains(&CleanupOp::Init(match_root)))
            .count(),
        2
    );
}

#[test]
fn cleanup_plan_nested_loops_keep_distinct_shared_break_destinations() {
    let plan = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let choose(flag: bool): payload = { loop {
  let inner = loop {
if flag { break(payload { value: 1 }) }
break(payload { value: 2 })
  }
  break(inner)
} }
"#,
        "choose",
    );
    let loop_scopes: Vec<_> = plan
        .scopes
        .iter()
        .filter(|scope| scope.kind == CleanupScopeKind::Loop)
        .map(|scope| scope.id)
        .collect();
    assert_eq!(loop_scopes.len(), 2);
    let mut destinations = Vec::new();
    for loop_scope in loop_scopes {
        let loop_destinations: HashSet<_> = plan
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    &block.terminator,
                    Some(CleanupTerminator::Goto(edge))
                        if edge.exited_scopes.contains(&loop_scope)
                )
            })
            .flat_map(|block| block.operations.iter())
            .filter_map(|operation| match operation {
                CleanupOp::Transfer {
                    source,
                    destination,
                    ..
                } if plan.locals[plan.move_paths[source.index()].place.local.index()].kind
                    == CleanupLocalKind::Temporary =>
                {
                    Some(*destination)
                }
                _ => None,
            })
            .collect();
        assert_eq!(loop_destinations.len(), 1);
        destinations.push(*loop_destinations.iter().next().expect("break destination"));
    }
    assert_ne!(destinations[0], destinations[1]);
}

#[test]
fn cleanup_plan_materializes_discarded_calls_and_partial_captures() {
    let discarded = cleanup_plan_text(
        r#"
let payload = struct { value: i32 }
let make(): payload = { payload { value: 1 } }
let discard(): () = {
  make()
  ()
}
"#,
        "discard",
    );
    assert!(discarded.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| match operation {
            CleanupOp::Init(path) => {
                discarded.locals[discarded.move_paths[path.index()].place.local.index()].kind
                    == CleanupLocalKind::Temporary
            }
            _ => false,
        })
    }));

    let partial = cleanup_plan_text(
        r#"
let add(x: i32)(y: i32): i32 = { x + y }
let run(): i32 = {
  let add_one = add(1)
  add_one(2)
}
"#,
        "run",
    );
    assert!(partial.move_paths.iter().any(|path| {
        matches!(
            path.place.projections.as_slice(),
            [CleanupProjection::Capture(0)]
        )
    }));

    let closure = cleanup_plan_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let consume(move value: resource): () = { () }
let run(): () = {
  let resource = resource { value: 1 }
  let once = { consume(resource) }
}
"#,
        "run",
    );
    assert!(closure.move_paths.iter().any(|path| {
        path.needs_drop && path.place.projections.as_slice() == [CleanupProjection::Capture(0)]
    }));
}

#[test]
fn cleanup_plan_transfers_and_consumes_callable_alias_environments() {
    let plan = cleanup_plan_text(
        r#"
let resource = struct { value: i32 }
extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let finish(move resource: resource)(value: i32): i32 = { value }
let main(): i32 = {
  let pending = finish(resource { value: 1 })
  let alias = pending
  alias(42)
}
"#,
        "main",
    );
    let capture_roots = plan
        .move_paths
        .iter()
        .filter_map(|path| {
            matches!(
                path.place.projections.last(),
                Some(CleanupProjection::Capture(_))
            )
            .then_some(path.parent)
            .flatten()
        })
        .collect::<HashSet<_>>();
    let alias_destination = plan.blocks.iter().find_map(|block| {
        block
            .operations
            .iter()
            .find_map(|operation| match operation {
                CleanupOp::Transfer {
                    source,
                    destination,
                    kind: TransferKind::Initialize,
                } if capture_roots.contains(source) => Some(*destination),
                _ => None,
            })
    });
    let alias_destination = alias_destination.expect("callable alias must transfer its root");
    assert!(plan.blocks.iter().any(|block| {
        block
            .operations
            .contains(&CleanupOp::MoveOut(alias_destination))
    }));
}

#[test]
fn cleanup_plan_makes_uninhabited_parameter_entries_unreachable() {
    let plan = cleanup_plan_text("let absurd(move value: never): i32 = { value }\n", "absurd");
    let function_entry = plan
        .blocks
        .iter()
        .find(|block| plan.scopes[block.scope.index()].kind == CleanupScopeKind::FunctionBody)
        .expect("function entry");
    assert!(matches!(
        function_entry.terminator,
        Some(CleanupTerminator::Unreachable)
    ));
    assert!(function_entry.operations.is_empty());
    assert!(plan.blocks.iter().all(|block| {
        !matches!(block.terminator, Some(CleanupTerminator::Return { .. }))
            && block.operations.iter().all(|operation| {
                !matches!(operation, CleanupOp::Init(_) | CleanupOp::Transfer { .. })
            })
    }));
}

#[test]
fn cleanup_plan_evaluates_projected_empty_operands_without_a_result_store() {
    let field_plan = cleanup_plan_text(
        r#"
let empty = enum {}
let holder = struct { value: empty }
let absurd(move holder: holder): i32 = { holder.value }
"#,
        "absurd",
    );
    let return_local = field_plan
        .locals
        .iter()
        .find(|local| local.kind == CleanupLocalKind::ReturnPlace)
        .expect("return place");
    assert!(field_plan
        .blocks
        .iter()
        .any(|block| { matches!(block.terminator, Some(CleanupTerminator::Unreachable)) }));
    assert!(field_plan
        .move_paths
        .iter()
        .any(|path| { path.place.projections == [CleanupProjection::Field(0)] }));
    assert!(field_plan.blocks.iter().all(|block| {
        !matches!(block.terminator, Some(CleanupTerminator::Return { .. }))
            && block.operations.iter().all(|operation| match operation {
                CleanupOp::Init(path) | CleanupOp::Overwrite(path) => {
                    field_plan.move_paths[path.index()].place.local != return_local.id
                }
                CleanupOp::Transfer { destination, .. } => {
                    field_plan.move_paths[destination.index()].place.local != return_local.id
                }
                _ => true,
            })
    }));

    let index_plan = cleanup_plan_text(
        r#"
let empty = enum {}
extend(empty, copyable) {}
let identity(move values: array(empty)(1)): array(empty)(1) = { values }
let absurd(move values: array(empty)(1)): i32 = { identity(values)[0] }
"#,
        "absurd",
    );
    assert!(index_plan
        .blocks
        .iter()
        .any(|block| { matches!(block.terminator, Some(CleanupTerminator::Unreachable)) }));
    assert!(index_plan
        .move_paths
        .iter()
        .any(|path| { path.place.projections == [CleanupProjection::ConstantIndex(0)] }));
    assert!(index_plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(operation, CleanupOp::Transfer { source, .. }
                if index_plan.move_paths[source.index()].place.projections.is_empty())
        })
    }));
}

#[test]
fn cleanup_plan_does_not_move_out_discarded_copy_projections() {
    let field_plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let make(): boxed = { boxed { value: 42 } }
let discard(): () = {
  make().value
  ()
}
"#,
        "discard",
    );
    let field = field_plan
        .move_paths
        .iter()
        .find(|path| path.place.projections == [CleanupProjection::Field(0)])
        .expect("discarded field projection");
    assert!(field_plan
        .blocks
        .iter()
        .all(|block| !block.operations.contains(&CleanupOp::MoveOut(field.id))));

    let index_plan = cleanup_plan_text(
        r#"
let discard(): () = {
  [42][0]
  ()
}
"#,
        "discard",
    );
    let element = index_plan
        .move_paths
        .iter()
        .find(|path| path.place.projections == [CleanupProjection::ConstantIndex(0)])
        .expect("discarded index projection");
    assert!(index_plan
        .blocks
        .iter()
        .all(|block| !block.operations.contains(&CleanupOp::MoveOut(element.id))));
}

#[test]
fn cleanup_plan_initializes_while_and_empty_break_results() {
    let while_plan = cleanup_plan_text(
        r#"
let bind(): () = {
  let done = while { false } {}
  done
}
"#,
        "bind",
    );
    let done = while_plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("while binding");
    let done_path = while_plan
        .move_paths
        .iter()
        .find(|path| path.place.local == done.id && path.place.projections.is_empty())
        .expect("while binding path");
    assert!(while_plan
        .blocks
        .iter()
        .any(|block| block.operations.contains(&CleanupOp::Init(done_path.id))));

    let loop_plan = cleanup_plan_text(
        r#"
let bind(): () = {
  let done = loop { break() }
  done
}
"#,
        "bind",
    );
    let done = loop_plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("loop binding");
    let done_path = loop_plan
        .move_paths
        .iter()
        .find(|path| path.place.local == done.id && path.place.projections.is_empty())
        .expect("loop binding path");
    assert!(loop_plan
        .blocks
        .iter()
        .any(|block| block.operations.contains(&CleanupOp::Init(done_path.id))));

    let assignment_plan = cleanup_plan_text(
        r#"
let assign(): () = {
  let mut done = ()
  done = while { false } {}
  done
}
"#,
        "assign",
    );
    assert!(assignment_plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| {
            matches!(
                operation,
                CleanupOp::Transfer {
                    kind: TransferKind::Overwrite,
                    ..
                }
            )
        })
    }));
}

#[test]
fn cleanup_plan_restarts_iteration_temporary_lifetimes() {
    let while_plan = cleanup_plan_text(
        r#"
let cycle(): () = { while { false } { 1; () } }
"#,
        "cycle",
    );
    let while_loop = while_plan
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::Loop)
        .expect("while loop scope")
        .id;
    let evaluation_scopes: Vec<_> = while_plan
        .scopes
        .iter()
        .filter(|scope| {
            scope.parent == Some(while_loop) && scope.kind == CleanupScopeKind::Temporary
        })
        .map(|scope| scope.id)
        .collect();
    assert_eq!(evaluation_scopes.len(), 2);
    assert!(evaluation_scopes.iter().all(|evaluation_scope| {
        while_plan
            .blocks
            .iter()
            .any(|block| match &block.terminator {
                Some(CleanupTerminator::Goto(edge)) => {
                    edge.exited_scopes.contains(evaluation_scope)
                }
                Some(CleanupTerminator::Branch {
                    then_edge,
                    else_edge,
                    ..
                }) => {
                    then_edge.exited_scopes.contains(evaluation_scope)
                        || else_edge.exited_scopes.contains(evaluation_scope)
                }
                _ => false,
            })
    }));

    let loop_plan = cleanup_plan_text(
        r#"
let cycle(): never = { loop { 1; () } }
"#,
        "cycle",
    );
    let loop_scope = loop_plan
        .scopes
        .iter()
        .find(|scope| scope.kind == CleanupScopeKind::Loop)
        .expect("loop scope")
        .id;
    let body_scope = loop_plan
        .scopes
        .iter()
        .find(|scope| scope.parent == Some(loop_scope) && scope.kind == CleanupScopeKind::Temporary)
        .expect("loop body evaluation scope")
        .id;
    assert!(loop_plan.blocks.iter().any(|block| {
        matches!(
            &block.terminator,
            Some(CleanupTerminator::Goto(edge))
                if loop_plan.blocks[edge.target.index()].scope == body_scope
                    && edge.exited_scopes.contains(&body_scope)
        )
    }));
}

#[test]
fn cleanup_plan_keeps_condition_break_values_reachable() {
    let plan = cleanup_plan_text(
        r#"
let bind(): () = {
  let done = while { loop { break(false) } } {}
  done
}
"#,
        "bind",
    );
    let done = plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("while binding");
    let done_path = plan
        .move_paths
        .iter()
        .find(|path| path.place.local == done.id && path.place.projections.is_empty())
        .expect("while binding path");
    assert!(plan
        .blocks
        .iter()
        .any(|block| block.operations.contains(&CleanupOp::Init(done_path.id))));
    assert!(plan
        .blocks
        .iter()
        .any(|block| matches!(block.terminator, Some(CleanupTerminator::Return { .. }))));

    let do_plan = cleanup_plan_text(
        r#"
let bind(): i32 = {
  let done: i32 = do {
return(40)
0
  }
  done + 2
}
"#,
        "bind",
    );
    let done = do_plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("do result binding");
    assert!(do_plan.blocks.iter().any(|block| {
        block.operations.iter().any(|operation| match operation {
            CleanupOp::Init(path) => do_plan.move_paths[path.index()].place.local == done.id,
            _ => false,
        })
    }));
}

#[test]
fn cleanup_plan_does_not_initialize_results_when_loop_inputs_diverge() {
    let condition_plan = cleanup_plan_text(
        r#"
let stop(): never = { loop {} }
let bind(): () = {
  let done = while { stop() } {}
  done
}
"#,
        "bind",
    );
    let done = condition_plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("while binding");
    assert!(condition_plan.blocks.iter().all(|block| {
        block.operations.iter().all(|operation| match operation {
            CleanupOp::Init(path) => condition_plan.move_paths[path.index()].place.local != done.id,
            _ => true,
        })
    }));
    assert!(condition_plan
        .blocks
        .iter()
        .any(|block| { matches!(block.terminator, Some(CleanupTerminator::Unreachable)) }));

    let no_break_plan = cleanup_plan_text(
        r#"
let bind(): () = {
  let done = loop {}
  done
}
"#,
        "bind",
    );
    let done = no_break_plan
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("done"))
        .expect("loop binding");
    assert!(no_break_plan.blocks.iter().all(|block| {
        block.operations.iter().all(|operation| match operation {
            CleanupOp::Init(path) => no_break_plan.move_paths[path.index()].place.local != done.id,
            _ => true,
        })
    }));
}

#[test]
fn cleanup_plan_records_assignment_kinds_moves_and_maybe_overwrite_state() {
    let plan = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
let consume(move boxed: boxed): () = { () }
let classify(flag: bool): i32 = {
  let mut boxed = boxed { value: 0 }
  boxed = boxed { value: 1 }
  consume(boxed)
  boxed = boxed { value: 2 }
  if flag { consume(boxed) }
  boxed = boxed { value: 42 }
  boxed.value
}
"#,
        "classify",
    );
    let operations: Vec<_> = plan
        .blocks
        .iter()
        .flat_map(|block| block.operations.iter())
        .collect();
    assert!(operations
        .iter()
        .any(|operation| matches!(operation, CleanupOp::MoveOut(_))));
    assert!(operations
        .iter()
        .any(|operation| matches!(operation, CleanupOp::Init(_))));
    assert!(operations.iter().any(|operation| matches!(
        operation,
        CleanupOp::Transfer {
            kind: TransferKind::Overwrite,
            ..
        }
    )));
    assert!(operations.iter().any(|operation| matches!(
        operation,
        CleanupOp::Transfer {
            kind: TransferKind::MaybeOverwrite,
            ..
        }
    )));
}

#[test]
fn cleanup_plan_classifies_drop_paths_and_conditional_flags_from_types() {
    let conditional = cleanup_plan_text(
        r#"
let boxed = struct { value: i32 }
extend(boxed, droppable) {
  let drop(self: borrow(mut)(self))(): () = { () }}
let consume(move value: boxed): () = { () }
let finish(flag: bool): () = {
  let boxed = boxed { value: 42 };
  if flag { consume(boxed) };
  ()
}
"#,
        "finish",
    );
    let boxed = conditional
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("boxed"))
        .expect("boxed local");
    let root = conditional
        .move_paths
        .iter()
        .find(|path| path.place.local == boxed.id && path.parent.is_none())
        .expect("boxed root path");
    assert!(root.needs_drop);
    assert!(conditional.drop_flags.flag_for_path(root.id).is_some());
    assert!(conditional
        .drop_flags
        .sites
        .iter()
        .any(|site| site.local == boxed.id));

    let copy = cleanup_plan_text(
        r#"
let plain = struct { value: i32 }
extend(plain, copyable) {}
let finish(): () = { let value = plain { value: 42 }; () }
"#,
        "finish",
    );
    let value = copy
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("value"))
        .expect("copyable local");
    let root = copy
        .move_paths
        .iter()
        .find(|path| path.place.local == value.id && path.parent.is_none())
        .expect("copyable root path");
    assert!(!root.needs_drop);
    assert!(copy.drop_flags.flag_for_path(root.id).is_none());
    assert!(copy
        .drop_flags
        .sites
        .iter()
        .all(|site| site.local != value.id));
}

#[test]
fn erased_effect_callable_uses_cps_entry_and_owned_environment() {
    let mut program = resolve_text(
        r#"
let continuation = core.effect.continuation

let main(): i32 = {
  let continuation: (i32): i32 = { (value: i32) -> value + 2 }
  let action: (i32, continuation(i32, i32)): i32 = {
(input: i32, move next: continuation(i32, i32)) -> invoke_continuation(next, input)
  }
  let erased_continuation = erase_continuation(continuation)
  let erased_action = erase_effect_callable(action)
  invoke_effect_callable(erased_action, 40, erased_continuation)
}
"#,
    );
    let replacements = HashMap::from([
        (
            "invoke_continuation".to_owned(),
            "$handler$invoke$continuation".to_owned(),
        ),
        (
            "erase_continuation".to_owned(),
            "$handler$erase$continuation".to_owned(),
        ),
        (
            "erase_effect_callable".to_owned(),
            "$handler$erase$effect$callable".to_owned(),
        ),
        (
            "invoke_effect_callable".to_owned(),
            "$handler$invoke$effect$callable".to_owned(),
        ),
    ]);
    let Item::Function(main) = &mut program.items[0] else {
        panic!("expected main function");
    };
    rewrite_static_function_values(main.body.as_mut().expect("main has a body"), &replacements);

    let llvm = compile(&program).expect("erased cps action must lower");
    assert!(llvm.contains("%salicin.effect_callable = type { ptr, ptr, ptr, ptr }"));
    assert!(llvm.contains("effect.callable.invoke"));
    assert!(llvm.contains("erased effect-callable environment"));
    assert!(!llvm.contains("$effect$callable$adapter$"));
    assert!(llvm.contains("246566666563742463616c6c61626c65246164617074657224"));
}

#[test]
fn reusable_handler_capturing_action_materializes_direct_literals() {
    let llvm = compile_text(
        r#"
let ask = effect { let value(): i32 }
let run()(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(10) } action { action() }
}
let main(): i32 = {
  let mut base = 31
  run() { () ->
base = base + 1
ask.value() + base
  }
}
"#,
    )
    .expect("direct trailing-closure actions must materialize before specialization");
    assert!(llvm.contains("24636170747572696e672468616e646c657224"));
}

#[test]
fn reusable_handler_materializes_arguments_before_direct_action() {
    let llvm = compile_text(
        r#"
let ask = effect { let value(): i32 }
let run(seed: i32)(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(20) } action { action() + seed }
}
let prepare(order: borrow(mut)(i32)): i32 = {
  order = order + 1
  20
}
let main(): i32 = {
  let mut order = 0
  run(prepare(order)) { () ->
order = order * 2
ask.value() + order
  }
}
"#,
    )
    .expect("arguments before a direct action must be materialized in source order");
    assert!(llvm.contains("24636170747572696e672468616e646c657224"));
}

#[test]
fn reusable_handler_stages_borrowed_arguments_before_direct_action() {
    compile_text(
        r#"
let ask = effect { let value(): i32 }
let run(left: borrow(i32), right: borrow(mut)(i32))(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(2) } action {
    right = right + action()
    left + right
  }
}

let main(): i32 = {
  let left = 10
  let mut right = 20
  let mut order = 1
  let result = run(left, right) { () ->
    order = order * 2
    ask.value() + order
  }
  result + left + right + order - 28
}
"#,
    )
    .expect("borrowed arguments before a direct action must retain their source places");
}

#[test]
fn array_literals_dispatch_through_user_trait_implementations() {
    compile_text(
        r#"
let total = struct { value: i32 }

extend(total, array_literal(i32)) {
  let output = total

  let from_array_literal(comptime length: usize)
    (move values: array(i32)(length)): output = {
    total { value: 42 }
  }
}

let main(): i32 = {
  let value: total = [10, 20, 12]
  value.value
}
"#,
    )
    .expect("array literals should call a user-defined source-backed constructor");
}

#[test]
fn string_literals_dispatch_to_byte_arrays_and_user_types() {
    compile_text(
        r#"
let tag = struct { first: u8 }

extend(tag, string_literal) {
  let output = tag

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output = {
    tag { first: 83 }
  }
}

let byte(): u8 = {
  let text: array(u8)(3) = "abc"
  text[1]
}

let main(): i32 = {
  let value: tag = "Salicin"
  if value.first == 83 && byte() == 98 { 0 } else { 1 }
}
"#,
    )
    .expect("string literals should expose UTF-8 backing through the literal trait");
}

#[test]
fn array_literals_unsize_to_borrowed_slices() {
    compile_text(
        r#"
let first(values: borrow(slice(i32))): i32 = { values[0] }
let slice = core.memory.slice
let main(): i32 = { first([42, 43]) }
"#,
    )
    .expect("array literal temporaries should back slice borrows");
}
