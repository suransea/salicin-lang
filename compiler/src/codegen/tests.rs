use super::*;
use crate::ast::Param;
use crate::cleanup::{
    CleanupEdge, CleanupOp, CleanupPlan, LocalKind as CleanupLocalKind,
    LocalOwnership as CleanupLocalOwnership, MovePathId as CleanupMovePathId,
    Projection as CleanupProjection, ScopeKind as CleanupScopeKind,
    Terminator as CleanupTerminator, TransferKind,
};

fn resolve_text(source: &str) -> Program {
    crate::modules::resolve_sources(&[crate::modules::SourceUnit {
        path: "<test>".to_owned(),
        module_path: Vec::new(),
        source: source.to_owned(),
        is_root: true,
    }])
    .unwrap_or_else(|diagnostics| panic!("test source must resolve: {diagnostics:?}"))
}

fn compile_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = crate::parser::parse(source).expect("test source must parse");
    compile(&program)
}

fn compile_resolved_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile(&program)
}

fn compile_library_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = crate::parser::parse(source).expect("test source must parse");
    compile_library(&program)
}

fn compile_resolved_library_text(source: &str) -> Result<String, Vec<Diagnostic>> {
    let program = resolve_text(source);
    compile_library(&program)
}

fn cleanup_plan_text(source: &str, function_name: &str) -> CleanupPlan {
    let program = resolve_text(source);
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
        .expect("requested HIR function");
    HirCleanupPlanner::build(&hir, function).expect("cleanup plan must build and verify")
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
    }
}

fn function(name: &str, groups: Vec<Vec<Param>>, result: Type, body: Expr) -> Item {
    Item::Function(Function {
        name: name.to_owned(),
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
        passing: None,
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
        "let identity(T: type)(move value: T): T = { value }\n\
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
    assert_eq!(analyzer.function_instances.len(), 1);
    let instance = analyzer
        .function_instances
        .values()
        .next()
        .expect("identity instance");
    assert_eq!(instance.key.arguments, vec![Ty::I32]);
    assert!(instance.canonical.starts_with("$mono$fn$"));
}

#[test]
fn inferred_and_explicit_type_arguments_share_instance_cache_keys() {
    let program = crate::parser::parse(
        "let identity(T: type)(move value: T): T = { value }\n\
         let Cell(T: type) = struct { value: T }\n\
         let main(): i32 = {\n\
           let explicit = Cell(i32) { value: identity(i32)(20) }\n\
           let inferred_value = identity(22)\n\
           let inferred = Cell { value: inferred_value }\n\
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
    assert!(hir.is_some(), "mixed generic program HIR");

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
        .filter(|instance| instance.key.template == "Cell")
        .collect();
    assert_eq!(nominal_instances.len(), 1);
    assert_eq!(nominal_instances[0].key.arguments, vec![Ty::I32]);
}

#[test]
fn generic_inherent_extensions_materialize_members_per_nominal_instance() {
    let program = crate::parser::parse(
        "let Cell(T: type) = struct { value: T }\n\
         extend(T: type) Cell(T) {\n\
           let new(move value: T): Cell(T) = { Cell { value: value } }\n\
           let take(move self)(): T = { self.value }\n\
         }\n\
         let main(): i32 = { let cell = Cell.new(42); cell.take() }\n",
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
        template: "Cell".into(),
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
        .contains_key(&("Cell".to_owned(), "new".to_owned())));
}

#[test]
fn inference_reifies_and_decomposes_generic_nominal_types() {
    let program = crate::parser::parse(
        "let Cell(T: type) = struct { value: T }\n\
         let unwrap(T: type)(move value: Cell(T)): T = { value.value }\n\
         let main(): i32 = {\n\
           let inner = Cell(i32) { value: 42 }\n\
           let outer = Cell { value: inner }\n\
           let nested = unwrap(outer.value)\n\
           let direct = unwrap(Cell(i32) { value: 0 })\n\
           nested + direct\n\
         }\n",
    )
    .expect("nested inferred nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("nested inferred nominal HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );

    let inner_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
        arguments: vec![Ty::I32],
    };
    let inner = analyzer.nominal_instance_names[&inner_key].clone();
    let outer_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
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
        "let same(T: type)(left: T, right: T): i32 = { 14 }\n\
         let accept(T: type)(value: T): i32 = { 14 }\n\
         let main(): i32 = {\n\
           let wide: i64 = 7\n\
           same(0, wide) + same(wide, 0) + accept(0 + wide)\n\
         }\n",
    )
    .expect("ordered inference source must parse");
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("ordered inference HIR");
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
    let source = "let Maybe(T: type) = enum { Some(T), None }\n\
                  let identity(T: type)(move value: T): T = { value }\n\
                  let main(): i32 = {\n\
                    let some = identity(Maybe(i32).Some(42))\n\
                    let none: Maybe(i32) = identity(Maybe(i32).None)\n\
                    some match { Some(value) => value, None => 0 }\n\
                  }\n";
    compile_text(source).expect("outer inference over enum constructors must compile");
}

#[test]
fn inference_conflicts_do_not_materialize_instances() {
    let program = crate::parser::parse(
        "let identity(T: type)(move value: T): T = { value }\n\
         let Cell(T: type) = struct { value: T }\n\
         let main(): bool = { identity(Cell(i32) { value: 42 }) }\n",
    )
    .expect("conflicting inference source must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(analyzer.analyze().is_none());
    assert!(analyzer.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("conflicting inference for type parameter `T`")
    }));
    assert!(analyzer.function_instances.is_empty());
    assert!(analyzer.function_instance_names.is_empty());
    let baseline_nominals = [
        analyzer.lang_item_name(LangItemKind::Never),
        analyzer.lang_item_name(LangItemKind::PartialOrdering),
    ];
    assert!(analyzer
        .nominal_instances
        .values()
        .all(|instance| baseline_nominals.contains(&instance.key.template.as_str())));
    assert!(analyzer
        .nominal_instance_names
        .keys()
        .all(|key| baseline_nominals.contains(&key.template.as_str())));
}

#[test]
fn template_validation_rolls_back_temporary_instances_and_emits_closed_ir() {
    let program = crate::parser::parse(
        "let identity(T: type)(move value: T): T = { value }\n\
         let wrap(T: type)(move value: T): T = { identity(T)(value) }\n\
         let main(): i32 = { wrap(i32)(42) }\n",
    )
    .expect("generic composition must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(analyzer.function_instances.is_empty());
    assert!(analyzer.function_instance_names.is_empty());
    assert!(analyzer.function_type_substitutions.is_empty());
    assert!(analyzer
        .function_order
        .iter()
        .all(|name| !name.contains("$generic$")));

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    let hir = analyzer.analyze().expect("closed program HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(analyzer.function_instances.len(), 2);
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
        "let identity(U: type)(move value: U): U = { value }\n\
         let wrap(T: type)(move value: T): T = { identity(value) }\n\
         let main(): i32 = { wrap(i32)(42) }\n",
    )
    .expect("inferred generic composition must parse");
    let mut analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected validation diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert!(analyzer.function_instances.is_empty());
    assert!(analyzer.function_instance_names.is_empty());
    assert!(analyzer.function_type_substitutions.is_empty());

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    let hir = analyzer.analyze().expect("closed inferred generic HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(analyzer.function_instances.len(), 2);
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
        "let Plain = struct { value: i32 }\n\
         let Cell(T: type) = struct { value: T }\n\
         let main(): i32 = { Cell(i32) { value: Plain { value: 40 }.value }.value + Cell(i32) { value: 2 }.value }\n",
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
        .get("Plain")
        .expect("plain nominal metadata");
    assert_eq!(plain.canonical, "Plain");
    assert_eq!(plain.key.kind, NominalKind::Struct);
    assert_eq!(plain.key.template, "Plain");
    assert!(plain.key.arguments.is_empty());
    assert!(analyzer
        .nominal_instances
        .values()
        .all(|instance| instance.key.arguments.is_empty()));

    let hir = analyzer.analyze().expect("generic nominal HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
        arguments: vec![Ty::I32],
    };
    let canonical = analyzer
        .nominal_instance_names
        .get(&key)
        .expect("Cell(i32) canonical name");
    let instances: Vec<_> = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "Cell")
        .collect();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].key, key);
    assert_eq!(instances[0].canonical, *canonical);
    assert!(canonical.starts_with("$mono$type$"));
    assert!(hir.structs.iter().any(|layout| layout.name == *canonical));
}

#[test]
fn allows_same_named_struct_and_constructor_function() {
    let ir = compile_text(
        r#"
let Pair = struct { left: i32, right: i32 }
let Pair(left: i32, right: i32): Pair = { Pair { left: left, right: right } }
let main(): i32 = {
  let pair = Pair(40, 2)
  pair.left + pair.right
}
"#,
    )
    .expect("same-named constructor function should compile");

    let constructor = function_symbol("Pair");
    assert!(ir.contains(&format!(
        "define internal %sali.type.50616972 @{constructor}(i32 %arg.0, i32 %arg.1)"
    )));
    assert!(ir.contains(&format!("call %sali.type.50616972 @{constructor}")));
}

#[test]
fn materializes_nested_generic_struct_layouts_in_dependency_order() {
    let program = crate::parser::parse(
        "let Cell(T: type) = struct { value: T }\n\
         let main(): i32 = { Cell(Cell(i32)) { value: Cell(i32) { value: 42 } }.value.value }\n",
    )
    .expect("nested generic nominal source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze();
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    let hir = hir.expect("nested generic nominal HIR");

    let inner_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
        arguments: vec![Ty::I32],
    };
    let inner = analyzer.nominal_instance_names[&inner_key].clone();
    let outer_key = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
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
    let ir = compile_text(
        "let Maybe(T: type) = enum {\n\
           Some(T),\n\
           None,\n\
         }\n\
         let choose(flag: bool): Maybe(i32) = { if flag {\n\
           Maybe(i32).Some(42)\n\
         } else {\n\
           Maybe(i32).None\n\
         } }\n\
         let unwrap(move value: Maybe(i32)): i32 = { value match {\n\
           Some(item) => item,\n\
           None => 0,\n\
         } }\n\
         let main(): i32 = { unwrap(choose(false)) }\n",
    )
    .expect("generic enum program must compile");
    let key = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "Maybe".into(),
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
    assert_eq!(
        &analyzer.enum_template_order[..2],
        &["core::Option".to_owned(), "core::Result".to_owned()]
    );

    let option = &analyzer.enum_templates["core::Option"];
    assert_eq!(option.compile_groups.len(), 1);
    assert_eq!(option.compile_groups[0].len(), 1);
    assert_eq!(option.compile_groups[0][0].name, "T");
    assert_eq!(option.variants.len(), 2);
    assert_eq!(option.variants[0].name, "Some");
    assert_eq!(
        option.variants[0].fields,
        VariantFields::Positional(vec![Type::Named("T".into(), Vec::new())])
    );
    assert_eq!(option.variants[1].name, "None");
    assert_eq!(option.variants[1].fields, VariantFields::Unit);

    let result = &analyzer.enum_templates["core::Result"];
    assert_eq!(result.compile_groups.len(), 2);
    assert_eq!(result.compile_groups[0].len(), 1);
    assert_eq!(result.compile_groups[0][0].name, "E");
    assert_eq!(result.compile_groups[1].len(), 1);
    assert_eq!(result.compile_groups[1][0].name, "T");
    assert_eq!(
        result
            .compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        vec!["E", "T"]
    );
    assert_eq!(result.variants.len(), 2);
    assert_eq!(result.variants[0].name, "Ok");
    assert_eq!(result.variants[1].name, "Err");

    let never = &analyzer.enum_defs["Never"];
    assert!(never.compile_groups.is_empty());
    assert!(never.variants.is_empty());
    assert!(analyzer.enum_layouts["Never"].variants.is_empty());
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
            name: "Error".to_owned(),
            kind: CompileParamKind::Type,
            default: None,
        }]]
    );
    assert_eq!(
        throw.return_type,
        Some(Type::Named("Never".to_owned(), Vec::new()))
    );
    assert_eq!(
        throw.effects.custom,
        vec![Type::Named(
            analyzer
                .lang_item_name(LangItemKind::ThrowsEffect)
                .to_owned(),
            vec![Type::Named("Error".to_owned(), Vec::new())],
        )]
    );
    let unsafe_name = analyzer.lang_item_name(LangItemKind::Unsafe);
    assert!(
        analyzer.function_templates[unsafe_name].body.is_some(),
        "core.control.unsafe must remain source-backed"
    );
    assert_eq!(analyzer.nominal_instances.len(), 2);
    assert_eq!(analyzer.nominal_instance_names.len(), 2);
    let partial_ordering = analyzer.lang_item_name(LangItemKind::PartialOrdering);
    assert!(analyzer
        .nominal_instances
        .values()
        .any(|instance| instance.key.kind == NominalKind::Enum
            && instance.key.template == partial_ordering
            && instance.key.arguments.is_empty()));
    assert!(analyzer.functions.is_empty());
    let boxed = |name: &str| format!("alloc::boxed::{name}");
    let vec = |name: &str| format!("alloc::vec::{name}");
    assert!(analyzer.function_templates.contains_key(&boxed("box_new")));
    assert!(analyzer.function_templates.contains_key(&boxed("box_ptr")));
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
            if access.name == "A" && access.kind == CompileParamKind::Access
                && ty.name == "T" && ty.kind == CompileParamKind::Type)
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
            if access.name == "A" && access.kind == CompileParamKind::Access
                && ty.name == "T" && ty.kind == CompileParamKind::Type)
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
        .contains_key(&("alloc::boxed::Box".to_owned(), "new".to_owned())));
    assert!(analyzer
        .generic_inherent_functions
        .contains_key(&("alloc::vec::Vec".to_owned(), "new".to_owned())));
    assert!(analyzer.generic_inherent_extensions["alloc::vec::Vec"]
        .iter()
        .flat_map(|extension| &extension.members)
        .any(
            |member| matches!(member, ExtendMember::Function(function) if function.name == "push")
        ));
    assert!(analyzer.function_order.is_empty());
}

#[test]
fn constructs_infers_and_matches_core_option_and_result() {
    let ir = compile_resolved_text(
        r#"
use core.Option
use core.Result

let unwrap_option(move value: Option(i32)): i32 = { value match {
  Some(item) => item,
  None => 0,
} }
let unwrap_result(move value: Result(bool)(i32)): i32 = { value match {
  Ok(item) => item,
  Err(_) => 0,
} }
let main(): i32 = {
  let some = Option.Some(19)
  let none: Option(i32) = Option.None
  let ok: Result(bool)(i32) = Result.Ok(23)
  let err: Result(bool)(i32) = Result.Err(false)
  unwrap_option(some) + unwrap_option(none) + unwrap_result(ok) + unwrap_result(err)
}
"#,
    )
    .expect("core Option and Result program must compile");
    let option = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::Option".into(),
        arguments: vec![Ty::I32],
    };
    let result = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::Result".into(),
        arguments: vec![Ty::Bool, Ty::I32],
    };
    assert!(ir.contains(&hex_name(&nominal_instance_name(&option))));
    assert!(ir.contains(&hex_name(&nominal_instance_name(&result))));
}

#[test]
fn empty_enums_are_uninhabited_and_never_supports_empty_match() {
    let ir = compile_library_text(
        r#"
let Empty = enum {}
let from_never(move value: Never): i32 = { value match {} }
let from_empty(move value: Empty): bool = { value match {} }
let stop(): Never = { loop {} }
let choose(flag: bool): i32 = { if flag { 42 } else { stop() } }
"#,
    )
    .expect("empty enums and empty matches must compile");

    assert!(ir.contains(&type_symbol("Never")));
    assert!(ir.contains(&type_symbol("Empty")));
    assert!(ir.contains("unreachable"));
    assert!(!ir.contains("define i32 @main()"));
}

#[test]
fn coalesce_lowers_option_and_result_through_lazy_match_control_flow() {
    let ir = compile_resolved_text(
        r#"
use core.Option
use core.Result

let make(count: borrow(mut)(i32)): Option(i32) = {
  count = count + 1
  Option(i32).Some(20)
}
let fallback(count: borrow(mut)(i32)): i32 = {
  count = count + 10
  22
}
let main(): i32 = {
  let mut count = 0
  let option = make(count) ?? fallback(count)
  let result = Result(bool)(i32).Err(false) ?? option
  if count == 11 { result } else { 0 }
}
"#,
    )
    .expect("Option and Result coalescing must compile");

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
fn throw_returns_the_enclosing_throws_error_variant() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let answer(fail: bool): i32 with(Throws(bool)) = {
  if fail { throw(true) }
  42
}
let main(): i32 = {
  let result: Result(bool)(i32) = try { answer(true) }
  result ?? 42
}
"#,
    )
    .expect("throw(in) a throws function must compile");
    assert!(ir.contains("switch i32"));
    assert!(ir.contains("ret %sali.type."));
}

#[test]
fn throws_calls_propagate_automatically_and_try_handles_them() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let read(fail: bool): i32 with(Throws(bool)) = {
  if fail { throw(true) }
  40
}
let forward(fail: bool): i32 with(Throws(bool)) = { read(fail) + 2 }
let invoke(action: (bool): i32 with(Throws(bool)))(fail: bool): i32 with(Throws(bool)) = {
  action(fail) }
let main(): i32 = {
  let result: Result(bool)(i32) = try { invoke(forward)(false) }
  result match {
Ok(value) => value,
Err(_) => 0
  }
}
"#,
    )
    .expect("throws calls should branch automatically and try should produce Result");
    assert!(ir.contains("switch"));

    let unhandled = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let read(): i32 with(Throws(bool)) = { throw(true) }
let main(): i32 = { read() }
"#,
    )
    .expect_err("a pure caller must handle a throws effect");
    assert!(unhandled
        .iter()
        .any(|error| error.message.contains("handle it with `try { ... }`")));
}

#[test]
fn try_infers_a_unique_escaping_throws_source_without_context() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let fail(flag: bool): i32 with(Throws(bool)) = { if flag { throw(true) } else { 41 } }
let main(): i32 = {
  let action = fail
  let direct = try { fail(false) }
  let indirect = try { action(false) }
  (direct ?? 0) + (indirect ?? 0) - 40
}
"#,
    )
    .expect("try should infer Result from a unique direct or indirect throws source");
    assert!(ir.contains("switch"));

    compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let Failure = struct { code: i32 }
extend Failure: Copy {}
extend Failure {
  let raise(self: borrow(Self))(): i32 with(Throws(bool)) = { throw(true) }
}
let main(): i32 = {
  let failure = Failure { code: 1 }
  let result: Result(bool)(i32) = try { failure.raise() }
  result ?? 42
}
"#,
    )
    .expect("contextual try should handle a complete throwing method call");

    let ambiguous = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let left(): i32 with(Throws(bool)) = { throw(true) }
let right(): i32 with(Throws(i64)) = { throw(1) }
let main(): i32 = {
  let result = try { if true { left() } else { right() } }
  result ?? 0
}
"#,
    )
    .unwrap_err();
    assert!(ambiguous.iter().any(|error| {
        error
            .message
            .contains("multiple escaping error types: `bool`, `i64`")
    }));

    let handled = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let fail(): i32 with(Throws(bool)) = { throw(true) }
let main(): i32 = {
  let inner = try { fail() }
  let outer = try { inner }
  outer match { Ok(_) => 0, Err(_) => 1 }
}
"#,
    )
    .unwrap_err();
    assert!(handled
        .iter()
        .any(|error| { error.message.contains("body has no escaping throws source") }));
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

    let positional = compile_text(
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

    let duplicate = compile_text(
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

    let partial = compile_text(
        r#"
let choose(value: i32)(left: i32): i32 = { value + left }
let choose(value: i32)(right: i32): i32 = { value + right }
let main(): i32 = { choose(value: 40)(right: 2) }
"#,
    )
    .expect("later named parameter groups should disambiguate curried overloads");
    assert!(partial.contains(&function_symbol("choose$overload$1$value$$1$right")));

    compile_text(
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
use core.Result
use core.effects.Throws

let choose(fail: bool): i32 with(Throws(bool)) = { if fail { throw(true) } else { 42 } }
let choose(value: i32): i32 = { value }
let main(): i32 = {
  let result = try { choose(fail: false) }
  result ?? 0
}
"#,
    )
    .expect("a selected overload should preserve throws inference and lowering");

    compile_text(
        r#"
let choose(T: type)(left: T): T = { left }
let choose(T: type)(right: T): T = { right }
let main(): i32 = { choose(left: 20) + choose(i32)(right: 22) }
"#,
    )
    .expect("generic overload selection should precede inferred or explicit type arguments");

    compile_text(
        r#"
let choose(left: i32): i32 = { left }
let choose(T: type)(right: T): T = { right }
let main(): i32 = { choose(left: 20) + choose(right: 22) }
"#,
    )
    .expect("concrete and generic functions may share one label-directed overload set");
}

#[test]
fn named_arguments_select_inherent_member_overloads() {
    let ir = compile_text(
        r#"
let Counter = struct { value: i32 }
extend Counter: Copy {}
extend Counter {
  let add(self: borrow(Self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(Self))(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): Counter = { Counter { value: left } }
  let make(right: i32): Counter = { Counter { value: right + 1 } }
}
let main(): i32 = {
  let counter = Counter.make(right: 19)
  counter.add(right: 21)
}
"#,
    )
    .expect("named parameters should select method and associated-function overloads");
    assert!(ir.contains(&function_symbol("Counter::function::make$overload$1$right")));
    assert!(ir.contains(&function_symbol(
        "Counter::method::add$overload$1$self$$1$right"
    )));

    let positional = compile_text(
        r#"
let Counter = struct { value: i32 }
extend Counter {
  let add(self: borrow(Self))(left: i32): i32 = { self.value + left }
  let add(self: borrow(Self))(right: i32): i32 = { self.value + right }
}
let main(): i32 = { Counter { value: 40 }.add(2) }
"#,
    )
    .unwrap_err();
    assert!(positional
        .iter()
        .any(|error| error.message.contains("requires named arguments")));

    compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let Counter = struct { value: i32 }
extend Counter: Copy {}
extend Counter {
  let read(self: borrow(Self))(fail: bool): i32 with(Throws(bool)) = {
if fail { throw(true) } else { self.value }
  }
  let read(self: borrow(Self))(fallback: i32): i32 = { fallback }
}
let main(): i32 = {
  let counter = Counter { value: 42 }
  let result: Result(bool)(i32) = try { counter.read(fail: false) }
  result ?? 0
}
"#,
    )
    .expect("a selected method overload should preserve throws inference");

    compile_text(
        r#"
let Counter = struct { value: i32 }
extend Counter {
  let choose(T: type)(left: T): T = { left }
  let choose(T: type)(right: T): T = { right }
  let add(T: type)(self: borrow(Self))(left: T): T = { left }
  let add(T: type)(self: borrow(Self))(right: T): T = { right }
}
let main(): i32 = {
  Counter.choose(left: 20) + Counter { value: 0 }.add(i32)(right: 22)
}
"#,
    )
    .expect("generic inherent overloads should infer or explicitly consume compile arguments");

    compile_text(
        r#"
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T) {
  let choose(left: T): T = { left }
  let choose(right: T): T = { right }
  let add(self: borrow(Self))(left: T): T = { left }
  let add(self: borrow(Self))(right: T): T = { right }
}
let main(): i32 = {
  Cell.choose(left: 20) + Cell(i32) { value: 0 }.add(right: 22)
}
"#,
    )
    .expect("blanket generic inherent extensions should preserve overload sets per instance");
}

#[test]
fn named_arguments_select_trait_member_overloads() {
    compile_text(
        r#"
let Select = trait {
  let pick(self: borrow(Self))(left: i32): i32
  let pick(self: borrow(Self))(right: i32): i32
  let make(left: i32): i32
  let make(right: i32): i32
}
let Counter = struct { value: i32 }
extend Counter: Select {
  let pick(self: borrow(Self))(left: i32): i32 = { self.value + left }
  let pick(self: borrow(Self))(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): i32 = { left }
  let make(right: i32): i32 = { right + 1 }
}
let main(): i32 = { Counter { value: 0 }.pick(right: 20) + Counter.make(right: 20) }
"#,
    )
    .expect("named parameters should select trait method and associated-function overloads");

    let positional = compile_text(
        r#"
let Select = trait {
  let pick(self: borrow(Self))(left: i32): i32
  let pick(self: borrow(Self))(right: i32): i32
}
let Counter = struct { value: i32 }
extend Counter: Select {
  let pick(self: borrow(Self))(left: i32): i32 = { self.value + left }
  let pick(self: borrow(Self))(right: i32): i32 = { self.value + right }
}
let main(): i32 = { Counter { value: 40 }.pick(2) }
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
let Select = trait {
  let pick(self: borrow(Self))(left: i32): i32 = { left }
  let pick(self: borrow(Self))(right: i32): i32 = { right + 1 }
}
let Counter = struct { value: i32 }
extend Counter: Select {}
let select(T: type)(value: borrow(T)): i32 where T: Select = {
  value.pick(right: 41)
}
let main(): i32 = { select(Counter { value: 0 }) }
"#,
    )
    .expect("default overloads should dispatch through an assumed where-bound implementation");

    compile_text(
        r#"
let Select = trait {
  let pick(self: borrow(Self))(left: i32): i32
  let pick(self: borrow(Self))(right: i32): i32
}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Select {
  let pick(self: borrow(Self))(left: i32): i32 = { left }
  let pick(self: borrow(Self))(right: i32): i32 = { right + 1 }
}
let main(): i32 = { Cell(i32) { value: 0 }.pick(right: 41) }
"#,
    )
    .expect("blanket trait implementations should preserve overload identities");

    compile_text(
        r#"
let Left = trait { let pick(self: borrow(Self))(left: i32): i32 }
let Right = trait { let pick(self: borrow(Self))(right: i32): i32 }
let Counter = struct { value: i32 }
extend Counter: Left {
  let pick(self: borrow(Self))(left: i32): i32 = { self.value + left }
}
extend Counter: Right {
  let pick(self: borrow(Self))(right: i32): i32 = { self.value + right + 1 }
}
let main(): i32 = { Counter { value: 20 }.pick(right: 21) }
"#,
    )
    .expect("named arguments should disambiguate same-named methods from distinct traits");

    let duplicate = compile_text(
        r#"
let Select = trait {
  let pick(self: borrow(Self))(value: i32): i32
  let pick(self: borrow(Self))(value: bool): i32
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
fn effect_parameters_infer_forward_and_explicitly_select_throws_rows() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let invoke(E: effect)(action: (): i32 with(E))(): i32 with(E) = { action() }
let fail(): i32 with(Throws(bool)) = { throw(true) }
let forward(): i32 with(Throws(bool)) = { invoke(fail)() }
let explicit(): i32 with(Throws(bool)) = { invoke(Throws(bool))(fail)() }
let main(): i32 = {
  let inferred: Result(bool)(i32) = try { forward() }
  let selected: Result(bool)(i32) = try { explicit() }
  (inferred ?? 21) + (selected ?? 21)
}
"#,
    )
    .expect("effect parameters should carry throws rows through inference and forwarding");
    assert!(ir.contains("switch"));
}

#[test]
fn throw_requires_an_exact_active_throws_boundary() {
    for (source, expected) in [
        (
            "let fail(): i32 with(Throws(bool)) = { throw(0) }\nlet main(): i32 = { 0 }\n",
            "requires `core::effects::Throws(i32)`",
        ),
        (
            "let fail(): Option(i32) = { throw(false) }\nlet main(): i32 = { 0 }\n",
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
        let source = format!("use core.Option\nuse core.Result\nuse core.effects.Throws\n{source}");
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
use core.Option

let fallback(): i32 = { 42 }
let main(): i32 = { Option(i32).Some(20) ?? fallback() }
"#,
    );
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("coalesce HIR must lower");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );
    let main = hir
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main HIR");
    let HirExprKind::Block(_, Some(tail)) = &main.body.kind else {
        panic!("function body must remain a block");
    };
    let HirExprKind::Match { arms, .. } = &tail.kind else {
        panic!("coalesce must lower to match HIR");
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
let Apply = trait {
  let apply(T: type)(move self)(move value: T): T
}
let Boxed = struct { value: i32 }
extend Boxed: Apply {
  let apply(T: type)(move self)(move value: T): T = {
value
  }
}
let main(): i32 = { Boxed { value: 40 }.apply(i32)(2) }
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
        self_ty: Ty::Struct("Boxed".into()),
        trait_ref: TraitRefKey {
            name: "Apply".into(),
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
fn coalesce_operator_dispatches_through_core_trait_for_user_types() {
    let program = resolve_text(
        r#"
use core.ops.Coalesce

let Choice = enum { Present(i32), Missing }

extend Choice: Coalesce {
  let Item = i32
  let coalesce(E: effect)
    (move self)
    (move fallback: (): i32 with(E)): i32 with(E) = {
self match {
  Present(value) => value,
  Missing => fallback(),
}
  }}

let main(): i32 = {
  let present = Choice.Present(10) ?? 1
  let missing = Choice.Missing ?? 2
  present + missing
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum("Choice".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Coalesce".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "coalesce");
    let mut analyzer = Analyzer::new(&program);
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected custom Coalesce diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("custom Coalesce method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("custom Coalesce implementation must drive `??`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("call i32 @{symbol}(")));
}

#[test]
fn generic_associated_type_constructor_rebinds_chain_result() {
    let program = resolve_text(
        r#"
use core.ops.Chain

let Boxed = struct { value: i32 }
let Maybe(T: type) = enum { Some(T), None }

extend Maybe(Boxed): Chain {
  let Item = Boxed
  let Rebind = Maybe
  let chain(E: effect, U: type)
    (move self)
    (move transform: (Boxed): U with(E)): Maybe(U) with(E) = {
self match {
  Some(value) => Maybe(U).Some(transform(value)),
  None => Maybe(U).None,
}
  }}

let main(): i32 = {
  let value = Maybe(Boxed).Some(Boxed { value: 40 }).chain(pure, i32)({ (item: Boxed) -> item.value + 2 })
  value match { Some(answer) => answer, None => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Maybe".into(),
            arguments: vec![Ty::Struct("Boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::ops::Chain".into(),
            arguments: Vec::new(),
        },
    };
    let template = trait_method_name(&key, "chain");
    let mut analyzer = Analyzer::new(&program);
    let implementation = analyzer
        .trait_impls
        .get(&key)
        .expect("Maybe(Boxed) must implement Chain");
    assert_eq!(
        implementation.associated_type_sources["Rebind"],
        Type::Named("Maybe".into(), Vec::new())
    );
    let lowered = analyzer.analyze();
    assert!(
        lowered.is_some() && analyzer.diagnostics.is_empty(),
        "unexpected custom Chain diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("custom Chain method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("direct custom Chain call must compile");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn chain_operator_dispatches_through_core_trait_for_user_types() {
    let program = resolve_text(
        r#"
use core.ops.Chain

let Boxed = struct { value: i32 }
let Maybe(T: type) = enum { Some(T), None }

extend Maybe(Boxed): Chain {
  let Item = Boxed
  let Rebind = Maybe
  let chain(E: effect, U: type)
    (move self)
    (move transform: (Boxed): U with(E)): Maybe(U) with(E) = {
self match {
  Some(value) => Maybe(U).Some(transform(value)),
  None => Maybe(U).None,
}
  }}

let main(): i32 = {
  let value = Maybe(Boxed).Some(Boxed { value: 42 })?.value
  value match { Some(answer) => answer, None => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Maybe".into(),
            arguments: vec![Ty::Struct("Boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::ops::Chain".into(),
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
        .expect("custom Chain operator method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("custom Chain implementation must drive `?.`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn generic_trait_implementation_can_rebind_chain_constructor() {
    let program = resolve_text(
        r#"
use core.ops.Chain

let Boxed = struct { value: i32 }
let Maybe(T: type) = enum { Some(T), None }

extend(T: type) Maybe(T): Chain {
  let Item = T
  let Rebind = Maybe
  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): Maybe(U) with(E) = {
self match {
  Some(value) => Maybe(U).Some(transform(value)),
  None => Maybe(U).None,
}
  }}

let main(): i32 = {
  let value = Maybe(Boxed).Some(Boxed { value: 42 })?.value
  value match { Some(answer) => answer, None => 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Enum(nominal_instance_name(&NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Maybe".into(),
            arguments: vec![Ty::Struct("Boxed".into())],
        })),
        trait_ref: TraitRefKey {
            name: "core::ops::Chain".into(),
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
        .expect("generic Maybe(Boxed) instance must implement Chain");
    assert_eq!(
        implementation.associated_type_sources["Rebind"],
        Type::Named("Maybe".into(), Vec::new())
    );
    let instance = analyzer
        .function_instances
        .values()
        .find(|instance| instance.key.template == template)
        .expect("generic custom Chain method template must instantiate")
        .canonical
        .clone();
    let ir = compile(&program).expect("generic custom Chain implementation must drive `?.`");
    let symbol = function_symbol(&instance);
    assert!(ir.contains(&format!("@{symbol}(")));
}

#[test]
fn coalesce_probe_participates_in_outer_inference_and_nests_right_associatively() {
    compile_resolved_text(
        r#"
use core.Option
use core.Result

let identity(T: type)(move value: T): T = { value }
let Boxed(T: type) = struct { value: T }
let main(): i32 = {
  let first = Option(i32).None
  let second = Option(i32).Some(42)
  let scalar = identity(Option.None ?? 42)
  let boxed = identity(Option.None ?? Boxed(i32) { value: 42 })
  let wide = identity(Result(T: i64).Err(false) ?? 42)
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
use core.Option
use core.Result

let main(): i32 = {
  let inferred_option = Option.None ?? 40
  let inferred_result = Result(E: bool).Err(false) ?? 2
  let option = Option.None ?? 40
  let result = Result(E: bool).Err(false) ?? 1
  let fully_inferred_result = Result.Err(false) ?? 1
  let wide = Result(T: i64).Err(false) ?? 42
  let nested = Option(i32).None ?? Option.None ?? 1
  inferred_option + inferred_result + option + result + fully_inferred_result + nested - 43
}
"#,
    )
    .expect("coalesce payload evidence must resolve empty variants");
}

#[test]
fn coalesce_does_not_guess_an_unconstrained_result_error_type() {
    let errors =
        compile_resolved_text("use core.Result\nlet main(): i32 = { Result.Ok(40) ?? 2 }\n")
            .unwrap_err();
    assert!(errors.iter().any(|diagnostic| {
        diagnostic.message.contains("cannot infer type argument")
            && diagnostic.message.contains("`E`")
            && diagnostic.message.contains("`core::Result`")
    }));
}

#[test]
fn coalesce_reports_non_containers_mismatched_fallbacks_and_moves() {
    let non_container = compile_text("let main(): i32 = { 40 ?? 2 }\n").unwrap_err();
    assert!(non_container.iter().any(|diagnostic| diagnostic.message
        == "operator `??` requires `Option(T)` or `Result(E)(T)` on the left, found `i32`"));

    let mismatch =
        compile_resolved_text("use core.Option\nlet main(): i32 = { Option(i32).None ?? true }\n")
            .unwrap_err();
    assert!(mismatch
        .iter()
        .any(|diagnostic| diagnostic.message.contains("type mismatch")));

    let moved = compile_resolved_text(
        r#"
use core.Result

let main(): i32 = {
  let value = Result(bool)(i32).Ok(42)
  let answer = value ?? 0
  value match { Ok(item) => item, Err(_) => answer }
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
use core.Option

let Boxed = struct { value: i32 }
let consume(move value: Boxed): i32 = { value.value }
let main(): i32 = {
  let spare = Boxed { value: 41 }
  let choice = Option(i32).Some(1)
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
use core.Option
use core.Result

let main(): i32 = {
  let number = Option.Some(42)
  let flag = Option.Some(true)
  let first: Result(bool)(i32) = Result.Ok(42)
  let second: Result(i32)(bool) = Result.Err(0)
  let value = number match { Some(item) => item, None => 0 }
  let enabled = flag match { Some(item) => item, None => false }
  let left = first match { Ok(item) => item, Err(_) => 0 }
  let right = second match { Ok(_) => 0, Err(item) => item }
  if enabled { value + left - right - 42 } else { 0 }
}
"#,
    );
    let mut analyzer = Analyzer::new(&program);
    analyzer.analyze().expect("multiple prelude instances HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        analyzer.diagnostics
    );

    let option_i32 = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::Option".into(),
        arguments: vec![Ty::I32],
    };
    let option_bool = NominalInstanceKey {
        kind: NominalKind::Enum,
        template: "core::Option".into(),
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
        .filter(|instance| instance.key.template == "core::Result")
        .map(|instance| instance.key.arguments.clone())
        .collect::<HashSet<_>>();
    assert_eq!(
        result_keys,
        HashSet::from([vec![Ty::I32, Ty::Bool], vec![Ty::Bool, Ty::I32]])
    );
}

#[test]
fn allows_user_redefinitions_of_unimported_core_nominal_names() {
    for (source, name) in [
        (
            "let Option = struct { value: i32 }\nlet main(): i32 = { 42 }\n",
            "Option",
        ),
        (
            "let Result(T: type) = enum { Value(value: T) }\nlet main(): i32 = { 42 }\n",
            "Result",
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
        crate::parser::parse("let Never = struct { value: i32 }\nlet main(): i32 = { 42 }\n")
            .expect("reserved Never source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(analyzer
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.message == "duplicate top-level name `Never`" }));
    assert!(analyzer.enum_defs["Never"].variants.is_empty());

    let program =
        crate::parser::parse("let Add = struct { value: i32 }\nlet main(): i32 = { 42 }\n")
            .expect("unimported Add source must parse");
    let analyzer = Analyzer::new(&program);
    assert!(!analyzer
        .diagnostics
        .iter()
        .any(|diagnostic| { diagnostic.message == "duplicate top-level name `Add`" }));
    assert!(analyzer.struct_defs.contains_key("Add"));
    assert!(analyzer.traits["core::ops::Add"].valid);

    let errors = compile_text("let invalid(value: void): () = { () }\nlet main(): i32 = { 42 }\n")
        .expect_err("`void` must not resolve as a unit alias");
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown type `void`")));
}

#[test]
fn reserves_compiler_provided_control_contracts_for_core() {
    let errors = compile_text(
        "let do(T: type)(move action: (): T): T = { action() }\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic
        .message
        .contains("control lang-item name `do` is reserved")));

    let errors = compile_text(
        "let throw(Error: type)(move error: Error): Never = { loop { continue } }\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors.iter().any(|diagnostic| diagnostic
        .message
        .contains("control lang-item name `throw` is reserved")));

    let errors = compile_text(
        "let external(value: i32): i32\n\
         let main(): i32 = { 0 }\n",
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|diagnostic| diagnostic.message.contains("has no body")));
}

#[test]
fn custom_domain_declarations_are_allowed() {
    compile_text(
        "let Local = domain { one two }\n\
         let main(): i32 = { 0 }\n",
    )
    .expect("ordinary source can declare domains");
}

#[test]
fn type_parameters_shadow_core_lang_item_names_in_type_heads() {
    for (name, source) in [
        (
            "Option",
            "let choose(Option: type)(): i32 = { Option.None ?? 42 }\n\
             let main(): i32 = { choose(i32)() }\n",
        ),
        (
            "Result",
            "let choose(Result: type)(): i32 = { Result.Ok(1) ?? 42 }\n\
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
        "let Cell(T: type) = struct { value: T }\n\
         let wrap(T: type)(move value: T): Cell(T) = { Cell(T) { value: value } }\n\
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
        .all(|instance| baseline_nominals.contains(&instance.key.template.as_str())));
    assert!(analyzer
        .nominal_instance_names
        .keys()
        .all(|key| baseline_nominals.contains(&key.template.as_str())));
    assert!(analyzer.struct_layouts.is_empty());
    assert!(analyzer.struct_order.is_empty());

    let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
    analyzer.analyze().expect("closed generic nominal HIR");
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected lowering diagnostics: {:?}",
        analyzer.diagnostics
    );
    let instances: Vec<_> = analyzer
        .nominal_instances
        .values()
        .filter(|instance| instance.key.template == "Cell")
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
        "let Measure = trait { let measure(self: borrow(Self))(): i32 }\n\
         let Value = struct { value: i32 }\n\
         extend Value: Measure {\n\
           let measure(self: borrow(Self))(): i32 = { self.value }\n\
         }\n\
         let read(T: type)(value: borrow(T)): i32\n\
         where T: Measure = { value.measure() }\n\
         let main(): i32 = { let value = Value { value: 42 }; read(value) }\n",
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
            "let Invalid(T: type) = struct { next: Invalid(T) }\n\
             let main(): i32 = { 42 }\n",
            "recursive generic value layout has infinite size",
        ),
        (
            "let Wrap(T: type) = struct { value: T }\n\
             let Grow(T: type) = struct { next: Wrap(Grow(Wrap(T))) }\n\
             let main(): i32 = { 42 }\n",
            "recursive generic value layout has infinite size",
        ),
        (
            "let Invalid(T: type) = struct { value: Missing }\n\
             let main(): i32 = { 42 }\n",
            "unknown type `Missing`",
        ),
        (
            "let Cell(T: type) = struct { value: T }\n\
             let main(): i32 = { Cell(U: i32) { value: Cell(i32) { value: 42 } }.value.value }\n",
            "expects exactly one argument group",
        ),
        (
            "let Cell(T: type) = struct { value: T }\n\
             extend Cell { let answer = 42 }\n\
             let main(): i32 = { 42 }\n",
            "generic extend target `Cell` is not supported",
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
    let source = "let identity(T: type)(move value: T): T = { value }\n\
                  let helper(value: i32) = { identity(i32)(value) }\n\
                  let preserve(T: type)(move value: T): T = { helper(0); value }\n\
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
fn emits_global_if_mutation_and_short_circuit() {
    let global = Item::Global(Binding {
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
let Pair = struct { left: i32, right: i32 }
let Choice = enum {
  Pair(left: Pair),
  Empty,
}
let global: Pair = Pair { left: 40, right: 2 }
let read(choice: Choice): i32 = { choice match {
  Choice.Pair(left: pair) => pair.left + pair.right,
  Choice.Empty => 0,
} }
let main(): i32 = { read(Choice.Pair(left: global)) }
"#,
    )
    .unwrap();
    assert!(ir.contains("%sali.type.50616972 = type { i32, i32 }"));
    assert!(ir.contains("%sali.type.43686f696365 = type { i32, %sali.type.50616972 }"));
    assert!(ir.contains("switch i32"));
    assert!(ir.contains("@sali.global.676c6f62616c = internal unnamed_addr constant"));
}

#[test]
fn rejects_private_fields_across_module_boundaries_for_read_construct_and_pattern() {
    let errors = compile_with_origins(
        r#"
pub let Record = struct { value: i32 }
pub let make_record(): Record = { Record { value: 40 } }
pub let Event = enum {
  Value(value: i32),
  Empty,
}
pub let make_event(): Event = { Event.Value(value: 2) }
let read(): i32 = { make_record().value }
let build(): Record = { Record { value: 0 } }
let unpack(): i32 = { make_event() match {
  Event.Value(value: item) => item,
  Event.Empty => 0,
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
        .any(|error| error.message.contains("field `Record.value` is private")));
    assert!(errors.iter().any(|error| error
        .message
        .contains("field `Event.Value.value` is private")));
}

#[test]
fn permits_package_fields_in_sibling_modules_and_public_fields_across_packages() {
    compile_with_origins(
        r#"
pub let PackageRecord = struct { pub(package) value: i32 }
pub let make_package(): PackageRecord = { PackageRecord { value: 40 } }
pub let PublicRecord = struct { pub value: i32 }
pub let make_public(): PublicRecord = { PublicRecord { value: 2 } }
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
pub let PackageRecord = struct { pub(package) value: i32 }
pub let make_package(): PackageRecord = { PackageRecord { value: 40 } }
pub let Event = enum { Value(value: i32), Empty }
pub let make_event(): Event = { Event.Value(value: 2) }
let forbidden(): i32 = { make_package().value }
let allowed(): i32 = { make_event() match {
  Event.Value(value: value) => value,
  Event.Empty => 0,
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
        .contains("field `PackageRecord.value` is pub(package)")));
    assert!(!errors
        .iter()
        .any(|error| error.message.contains("Event.Value.0")));
}

#[test]
fn rejects_inferred_public_function_and_global_types_that_leak_private_nominals() {
    let errors = compile_resolved_with_origins(
        r#"
use core.Option

let Hidden = struct {}
pub let expose() = { Hidden {} }
pub let wrapped() = { Option(Hidden).Some(Hidden {}) }
pub let shared = Hidden {}
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
            && error.message.contains("private type `Hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("function `wrapped` return type")
            && error.message.contains("private type `Hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("global `shared` type")
            && error.message.contains("private type `Hidden`")
    }));
}

#[test]
fn permits_package_apis_to_infer_root_private_types() {
    compile_with_origins(
        r#"
let RootSecret = struct {}
pub(package) let make() = { RootSecret {} }
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
use core.Option

pub let Cell(T: type) = struct { value: T }
pub let make(): Option(Cell(i32)) = { Option(Cell(i32)).Some(Cell(i32) { value: 42 }) }
let infer(): Cell(i32) = { Cell { value: 0 } }
let chain(): Option(i32) = { make()?.value }
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
            .filter(|error| error.message.contains("field `Cell.value` is private"))
            .count()
            >= 2,
        "{errors:?}"
    );
}

#[test]
fn short_unit_variants_do_not_bypass_private_enum_visibility() {
    let errors = compile_with_origins(
        r#"
let Secret = enum { Only }
let forbidden(): i32 = { Only; 0 }
let main(): i32 = { forbidden() }
"#,
        vec![origin(1, &["hidden"]), origin(1, &[]), origin(1, &[])],
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("unknown name `Only`")),
        "{errors:?}"
    );
}

#[test]
fn inherent_members_inherit_the_target_api_boundary_for_leak_checks() {
    let errors = compile_text(
        r#"
let Hidden = struct {}
pub let Public = struct {}
extend Public {
  let reveal(self: borrow(Self))() = { Hidden {} }
  let secret = Hidden {}
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("return type") && error.message.contains("private type `Hidden`")
    }));
    assert!(errors.iter().any(|error| {
        error.message.contains("global") && error.message.contains("private type `Hidden`")
    }));
}

#[test]
fn generic_inherent_member_boundaries_include_concrete_type_arguments() {
    compile_text(
        r#"
let Hidden = struct { value: i32 }
pub let Cell(T: type) = struct { pub value: T }
extend(T: type) Cell(T) {
  let new(move value: T): Cell(T) = { Cell { value: value } }
  let take(move self)(): T = { self.value }
}
let use_hidden(): i32 = {
  let cell = Cell.new(Hidden { value: 42 })
  cell.take().value
}
let main(): i32 = { use_hidden() }
"#,
    )
    .expect("private type arguments must narrow concrete generic member APIs");
}

#[test]
fn trait_impl_associated_types_cannot_widen_beyond_trait_and_target_access() {
    let errors = compile_text(
        r#"
let Hidden = struct {}
pub let Public = struct {}
pub let Convert = trait {
  let Output: type
  let convert(self: borrow(Self))(): Output
}
extend Public: Convert {
  let Output = Hidden
  let convert(self: borrow(Self))(): Hidden = { Hidden {} }}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("trait implementation")
            && error.message.contains("associated type `Output`")
            && error.message.contains("private type `Hidden`")
    }));

    compile_text(
        r#"
let Hidden = struct {}
let Private = struct {}
pub let Convert = trait {
  let Output: type
  let convert(self: borrow(Self))(): Output
}
extend Private: Convert {
  let Output = Hidden
  let convert(self: borrow(Self))(): Hidden = { Hidden {} }}
let main(): i32 = { 0 }
"#,
    )
    .unwrap();
}

#[test]
fn generic_trait_extensions_materialize_conditionally_with_associated_types() {
    compile_text(
        r#"
let Read = trait {
  let read(self: borrow(Self))(): i32
}
let Leaf = struct { value: i32 }
extend Leaf: Read {
  let read(self: borrow(Self))(): i32 = { self.value }
}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read
where T: Read {
  let read(self: borrow(Self))(): i32 = { self.value.read() }
}

let read_cell(T: type)(cell: borrow(Cell(T))): i32
where T: Read = { cell.read() }

let Value = trait {
  let Item: type
  let take(move self)(): Item
}
extend(T: type) Cell(T): Value {
  let Item = T
  let take(move self)(): T = { self.value }
}

let main(): i32 = {
  let cell = Cell { value: Leaf { value: 42 } }
  let read = read_cell(cell)
  let leaf = cell.take()
  let wrapped = Cell { value: leaf }
  wrapped.read() + read - 42
}
"#,
    )
    .expect("generic trait extensions should instantiate for matching nominal arguments");

    let errors = compile_text(
        r#"
let Read = trait {
  let read(self: borrow(Self))(): i32
}
let Leaf = struct { value: i32 }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read
where T: Read {
  let read(self: borrow(Self))(): i32 = { self.value.read() }
}
let main(): i32 = {
  let cell = Cell { value: Leaf { value: 42 } }
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
let Read = trait {
  let read(self: borrow(Self))(): i32
}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i32 = { 1 }
}
extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i32 = { 2 }
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
let Convert(To: type) = trait {
  let convert(self: borrow(Self))(): To
}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { 1 }
}
extend(T: type) Cell(T): Convert(i64) {
  let convert(self: borrow(Self))(): i64 = { 2 }
}
let main(): i32 = {
  let cell = Cell { value: true }
  42
}
"#,
    )
    .expect("blanket implementations with disjoint trait arguments should coexist");

    compile_text(
        r#"
let Convert(To: type) = trait { let convert(self: borrow(Self))(): To }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Convert(T)
where T: Copy {
  let convert(self: borrow(Self))(): T = { self.value }}
extend Cell(i32): Convert(i64) {
  let convert(self: borrow(Self))(): i64 = { 42 }
}
let main(): i32 = { 42 }
"#,
    )
    .expect("a concrete implementation disjoint after target substitution should coexist");

    for source in [
        r#"
let Convert(To: type) = trait { let convert(self: borrow(Self))(): To }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { 1 }
}
extend Cell(i32): Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { 2 }
}
let main(): i32 = { 42 }
"#,
        r#"
let Convert(To: type) = trait { let convert(self: borrow(Self))(): To }
let Cell(T: type) = struct { value: T }
extend Cell(i32): Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { 2 }
}
extend(T: type) Cell(T): Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { 1 }
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
let Read = trait { let read(self: borrow(Self))(): i32 }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read
where T: Read {
  let read(self: borrow(Self))(): i32 = { self.value.read() }
}
let main(): i32 = { 42 }
"#,
    )
    .expect("an uninstantiated blanket body should validate from its where proofs");

    let mismatch = compile_text(
        r#"
let Read = trait { let read(self: borrow(Self))(): i32 }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i64 = { 0 }
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
let Read = trait { let read(self: borrow(Self))(): i32 }
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i32 = { missing }
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
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Copy
where T: Copy {}
let sum(copy cell: Cell(i32)): i32 = { cell.value }
let main(): i32 = {
  let cell = Cell { value: 42 }
  let first = cell
  sum(cell) + first.value - 42
}
"#,
    )
    .expect("a structurally valid blanket Copy instance should be copyable");

    compile_text(
        r#"
let Maybe(T: type) = enum {
  Some(T),
  None,
}
extend(T: type) Maybe(T): Copy
where T: Copy {}
let read(copy value: Maybe(i32)): i32 = { value match {
  Some(number) => number,
  None => 0,
} }
let main(): i32 = {
  let value: Maybe(i32) = Maybe.Some(42)
  let duplicate = value
  read(value) + read(duplicate) - 42
}
"#,
    )
    .expect("blanket Copy should validate and materialize for generic enums");

    compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { self.value = 0 }}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Copy
where T: Copy {}
let main(): i32 = {
  let cell = Cell { value: Resource { value: 42 } }
  let moved = cell
  moved.value.value
}
"#,
    )
    .expect("an unsatisfied blanket Copy predicate should leave the instance movable");

    let invalid = compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { self.value = 0 }}
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Copy {}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an unconditional blanket Copy must still be structurally valid");
    assert!(invalid
        .iter()
        .any(|error| error.message.contains("not structurally valid")));

    let conflict = compile_text(
        r#"
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Copy
where T: Copy {}
extend(T: type) Cell(T): Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = {
  let cell = Cell { value: 42 }
  42
}
"#,
    )
    .expect_err("a concrete instance cannot acquire both blanket Copy and Drop");
    assert!(conflict
        .iter()
        .any(|error| error.message.contains("both `Copy` and `Drop`")));

    let foreign_copy = compile_with_origins(
        r#"
pub let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Copy
where T: Copy {}
let main(): i32 = { 42 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("blanket Copy must be owned by the target package");
    assert!(foreign_copy
        .iter()
        .any(|error| error.message.contains("package that defines the type")));

    let foreign_drop = compile_with_origins(
        r#"
pub let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { 42 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("blanket Drop must be owned by the target package");
    assert!(foreign_drop
        .iter()
        .any(|error| error.message.contains("package that defines the type")));

    let missing_drop = compile_text(
        r#"
let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Drop {}
let main(): i32 = { 42 }
"#,
    )
    .expect_err("an unused blanket Drop implementation must still be complete");
    assert!(missing_drop
        .iter()
        .any(|error| error.message.contains("missing trait method `Drop.drop`")));
}

#[test]
fn rejects_recursive_value_layouts() {
    let errors = compile_text(
        r#"
let First = struct { next: Second }
let Second = enum {
  Again(First),
  End,
}
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error.message.contains("recursive value layout")
            && error.message.contains("First -> Second -> First")
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
    assert!(!ir.contains("phi i1"));
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
    let ir = compile(&Program::new(vec![minimum_i64, main])).unwrap();
    assert!(ir.contains("sub i64 0, 9223372036854775808"));
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
            passing: None,
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
                    mutable: false,
                    name: "value".into(),
                    annotation: None,
                    value: Expr::Integer(7),
                }),
                Stmt::Let(Binding {
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
let Pair = struct { left: i32, right: i32 }
extend Pair: Copy {}
let inferred(value: Pair): i32 = { value.left + value.right }
let explicit(copy value: Pair): i32 = { value.left + value.right }

let main(): i32 = {
  let pair = Pair { left: 19, right: 23 }
  let duplicate = pair
  let first = inferred(pair)
  let second = explicit(pair)
  if first == 42 && second == 42 && duplicate.left + pair.right == 42 { 42 } else { 0 }
}
"#,
    )
    .expect("a validated core Copy implementation must preserve its source place");
}

#[test]
fn source_backed_drop_is_exclusive_local_and_not_directly_callable() {
    let conflict = compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Copy {}
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("Copy and Drop must be mutually exclusive");
    assert!(conflict
        .iter()
        .any(|error| error.message.contains("both `Copy` and `Drop`")));

    let orphan = compile_with_origins(
        r#"
pub let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { 0 }
"#,
        vec![
            origin(1, &["owner"]),
            origin(2, &["consumer"]),
            origin(2, &["consumer"]),
        ],
    )
    .expect_err("Drop must obey the nominal ownership rule");
    assert!(orphan.iter().any(|error| error
        .message
        .contains("must be implemented in the package that defines the type")));

    let direct = compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = {
  let mut value = Resource { value: 0 }
  value.drop()
  0
}
"#,
    )
    .expect_err("Drop.drop must remain compiler-only");
    assert!(direct
        .iter()
        .any(|error| error.message.contains("cannot be called directly")));
}

#[test]
fn ordinary_trait_implementations_obey_the_package_orphan_rule() {
    let concrete = compile_with_origins(
        r#"
pub let Read = trait {
  let read(self: borrow(Self))(): i32
}
pub let Foreign = struct { value: i32 }
extend Foreign: Read {
  let read(self: borrow(Self))(): i32 = { self.value }
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
pub let Read = trait {
  let read(self: borrow(Self))(): i32
}
pub let Cell(T: type) = struct { value: T }
extend(T: type) Cell(T): Read {
  let read(self: borrow(Self))(): i32 = { 0 }
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
let Resource = struct { value: i32 }
let Wrapper = struct { resource: Resource, plain: i32 }
let Choice = enum { Some(Wrapper), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { self.value = 0 }}
let main(): i32 = {
  let value = Choice.Some(Wrapper { resource: Resource { value: 42 }, plain: 1 })
  0
}
"#;
    let program = crate::parser::parse(source).expect("drop source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("drop source must lower");
    assert!(analyzer.diagnostics.is_empty());
    let resource = Ty::Struct("Resource".to_owned());
    let wrapper = Ty::Struct("Wrapper".to_owned());
    let choice = Ty::Enum("Choice".to_owned());
    assert!(hir.needs_drop(&resource));
    assert!(hir.needs_drop(&wrapper));
    assert!(hir.needs_drop(&choice));
    let drop_method = hir.drop_methods[&resource].clone();

    let ir = compile(&program).expect("recursive drop glue must emit");
    assert_eq!(ir.matches("define internal void @sali.drop.").count(), 3);
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
let Resource = struct { value: i32 }
let Wrapper = struct { resource: Resource, plain: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = {
  let wrapper = Wrapper { resource: Resource { value: 1 }, plain: 0 }
  let resource = wrapper.resource
  0
}
"#,
    )
    .expect("a field with Drop may move out of a containing structural wrapper");

    let custom_drop_field = compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let consume_i32(move value: i32): () = { () }
let main(): i32 = {
  let resource = Resource { value: 1 }
  consume_i32(resource.value)
  0
}
"#,
    )
    .expect_err("custom Drop storage must remain complete");
    assert!(custom_drop_field
        .iter()
        .any(|error| error.message.contains("type with custom Drop")));

    compile_text(
        r#"
let Resource = struct { value: i32 }
let Choice = enum { Some(Resource), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { Choice.Some(Resource { value: 1 }) match {
  Some(resource) => 1,
  None => 0
} }
"#,
    )
    .expect("a direct enum payload with Drop may move into an unguarded binding");

    let custom_enum_move = compile_text(
        r#"
let Resource = struct { value: i32 }
let Choice = enum { Some(Resource), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
extend Choice: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { Choice.Some(Resource { value: 1 }) match {
  Some(resource) => 1,
  None => 0
} }
"#,
    )
    .expect_err("custom Drop enum storage must remain complete");
    assert!(custom_enum_move
        .iter()
        .any(|error| error.message.contains("implements `Drop`")));

    compile_text(
        r#"
let Choice = enum { Some(i32), None }
extend Choice: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { Choice.Some(1) match {
  whole if true => 1,
  _ => 0
} }
"#,
    )
    .expect("a guarded whole-value binding may preserve custom Drop ownership");

    compile_text(
        r#"
let Resource = struct { value: i32 }
let Choice = enum { Some(Resource), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { Choice.Some(Resource { value: 1 }) match {
  Some(resource) if false => 1,
  Some(_) => 2,
  None => 0
} }
"#,
    )
    .expect("a guarded payload move remains speculative until its guard succeeds");

    let nested_custom_drop = compile_text(
        r#"
let Resource = struct { value: i32 }
let Wrapper = struct { resource: Resource }
let Choice = enum { Some(Wrapper), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
extend Wrapper: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let main(): i32 = { Choice.Some(Wrapper { resource: Resource { value: 1 } }) match {
  Some(Wrapper(resource: resource)) => 1,
  None => 0
} }
"#,
    )
    .expect_err("a nested custom Drop aggregate must remain complete");
    assert!(nested_custom_drop.iter().any(|error| {
        error.message.contains("nested pattern binding")
            && error.message.contains("implements `Drop`")
    }));
}

#[test]
fn explicit_move_still_consumes_a_copy_nominal() {
    let errors = compile_text(
        r#"
let Pair = struct { left: i32, right: i32 }
extend Pair: Copy {}
let consume(move value: Pair): i32 = { value.left + value.right }
let main(): i32 = {
  let pair = Pair { left: 19, right: 23 }
  let answer = consume(pair)
  answer + pair.left
}
"#,
    )
    .expect_err("an explicit move must override nominal Copy semantics");
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn reinitializes_a_moved_root_and_preserves_rhs_ordering() {
    compile_text(
        r#"
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = Boxed { value: 21 }
  let first = consume(boxed)
  boxed = Boxed { value: 21 }
  first + consume(boxed)
}
"#,
    )
    .expect("a mutable owned root may be initialized after a move");

    let errors = compile_text(
        r#"
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = Boxed { value: 42 }
  consume(boxed)
  boxed = boxed
  boxed.value
}
"#,
    )
    .expect_err("the assignment RHS must observe the moved state");
    assert!(errors.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn rebuilds_a_moved_struct_field_by_field() {
    compile_text(
        r#"
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let consume(move pair: Pair): i32 = { pair.left.value + pair.right.value }
let main(): i32 = {
  let mut pair = Pair { left: Payload { value: 1 }, right: Payload { value: 2 } }
  let old = consume(pair)
  pair.left = Payload { value: 19 }
  let restored = pair.left.value
  pair.right = Payload { value: 23 }
  old + restored + consume(pair) - 3 - 19
}
"#,
    )
    .expect("restoring every normalized leaf must restore its ancestors");

    let errors = compile_text(
        r#"
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let consume(move pair: Pair): () = { () }
let main(): i32 = {
  let mut pair = Pair { left: Payload { value: 1 }, right: Payload { value: 2 } }
  consume(pair)
  pair.left = Payload { value: 42 }
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): () = { () }
let choose(flag: bool): i32 = {
  let mut boxed = Boxed { value: 0 }
  consume(boxed)
  if flag { boxed = Boxed { value: 19 } } else { boxed = Boxed { value: 23 } }
  boxed.value
}
let main(): i32 = { choose(true) + choose(false) }
"#,
    )
    .expect("reinitialization on every reachable branch restores the value");

    let errors = compile_text(
        r#"
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): () = { () }
let choose(flag: bool): i32 = {
  let mut boxed = Boxed { value: 0 }
  consume(boxed)
  if flag { boxed = Boxed { value: 42 } }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let consume_payload(move payload: Payload): () = { () }
let consume_pair(move pair: Pair): () = { () }
let choose(flag: bool): () = {
  let pair = Pair { left: Payload { value: 19 }, right: Payload { value: 23 } }
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): () = { () }
let classify(flag: bool): i32 = {
  let mut boxed = Boxed { value: 0 }
  boxed = Boxed { value: 1 }
  consume(boxed)
  boxed = Boxed { value: 2 }
  if flag { consume(boxed) }
  boxed = Boxed { value: 42 }
  boxed.value
}
let main(): i32 = { classify(false) }
"#,
    )
    .expect("assignment-kind source must parse");
    let mut analyzer = Analyzer::new(&program);
    let hir = analyzer.analyze().expect("assignment-kind HIR must lower");
    let function = hir
        .functions
        .iter()
        .find(|function| function.name == "classify")
        .expect("classify HIR");
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let main(): i32 = {
  let mut boxed = Boxed { value: 0 }
  let mut iteration = 0
  while iteration < 2 {
let previous = consume(boxed)
boxed = Boxed { value: previous + 21 }
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
let Payload = struct { value: i32 }
let Event = enum { Value(value: Payload), Empty }
let accept(move payload: Payload): bool = { payload.value == 42 }
let classify(event: Event): i32 = { event match {
  Event.Value(value: payload) if accept(payload) => 42,
  Event.Value(value: _) => 0,
  Event.Empty => 0,
} }
let main(): i32 = { classify(Event.Value(value: Payload { value: 42 })) }
"#,
    )
    .expect_err("a false guard must not consume a reusable non-Copy payload");
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("guard") && error.message.contains("move") }));

    compile_text(
        r#"
let Event = enum { Value(value: i32), Empty }
let accept(move value: i32): bool = { value == 42 }
let classify(event: Event): i32 = { event match {
  Event.Value(value: value) if accept(value) => 42,
  Event.Value(value: value) => value,
  Event.Empty => 0,
} }
let main(): i32 = { classify(Event.Value(value: 42)) }
"#,
    )
    .expect("Copy payload bindings may be consumed by a guard attempt");
}

#[test]
fn copy_validation_is_structural_transitive_and_source_order_independent() {
    compile_text(
        r#"
let Inner = struct { value: i32 }
let Outer = struct { inner: Inner }
let Choice = enum { Empty, Value(value: Outer), Named(value: Inner) }
let Holder = struct { values: Array(Outer, 2) }

extend Holder: Copy {}
extend Choice: Copy {}
extend Outer: Copy {}
extend Inner: Copy {}
let main(): i32 = {
  let values = [Outer { inner: Inner { value: 19 } }, Outer { inner: Inner { value: 23 } }]
  let holder = Holder { values: values }
  let duplicate = holder
  let choice = Choice.Value(value: holder.values[1])
  let copied = choice
  let left = holder.values[0].inner.value
  let right = duplicate.values[1].inner.value
  copied match {
Value(value: _) => left + right,
_ => 0,
  }
}
"#,
    )
    .expect("Copy dependencies, enums, and arrays must validate independently of source order");
}

#[test]
fn rejects_non_structural_copy_and_does_not_generalize_concrete_instances() {
    let structural = compile_text(
        r#"
let Token = struct { value: i32 }
let Invalid = struct { token: Token }
extend Invalid: Copy {}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("Copy must inspect every private representation field");
    assert!(structural.iter().any(|error| {
        error.message.contains("cannot implement `Copy`") && error.message.contains("Invalid.token")
    }));

    let concrete = compile_text(
        r#"
let Cell(T: type) = struct { value: T }
extend Cell(i32): Copy {}
let consume(value: Cell(bool)): bool = { value.value }
let main(): i32 = {
  let cell = Cell(bool) { value: true }
  let answer = consume(cell)
  if answer && cell.value { 42 } else { 0 }
}
"#,
    )
    .expect_err("Cell(i32): Copy must not make Cell(bool) Copy");
    assert!(concrete.iter().any(|error| error.message.contains("moved")));
}

#[test]
fn copy_diagnostics_render_concrete_generic_source_types() {
    let parameter = compile_text(
        r#"
let Cell(T: type) = struct { value: T }
extend Cell(i32): Copy {}
let read(copy cell: Cell(i64)): i64 = { cell.value }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("a concrete instance without its own Copy impl must be rejected");
    assert!(parameter.iter().any(|error| {
        error
            .message
            .contains("nominal type `Cell(i64)` does not implement Copy")
    }));
    assert!(parameter
        .iter()
        .all(|error| !error.message.contains("$mono$type$")));

    let structural = compile_text(
        r#"
let Token = struct { value: i32 }
let Cell(T: type) = struct { value: T }
extend Cell(Token): Copy {}
let main(): i32 = { 0 }
"#,
    )
    .expect_err("a concrete Copy impl must still validate its instantiated fields");
    assert!(structural.iter().any(|error| {
        error
            .message
            .contains("`Cell(Token)` cannot implement `Copy`")
            && error.message.contains("field `Cell(Token).value`")
    }));
    assert!(structural
        .iter()
        .all(|error| !error.message.contains("$mono$type$")));
}

#[test]
fn reports_a_value_moved_on_only_one_if_path_as_possibly_moved() {
    let errors = compile_text(
        r#"
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = Boxed { value: 42 }
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = Boxed { value: 42 }
  if flag {
consume(boxed)
return 0
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = Boxed { value: 42 }
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): bool = { boxed.value == 42 }
let choose(flag: bool): i32 = {
  let boxed = Boxed { value: 42 }
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let choose(flag: bool): i32 = {
  let boxed = Boxed { value: 42 }
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
let Choice = enum {
  First,
  Second,
}
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): i32 = { boxed.value }
let choose(choice: Choice): i32 = {
  let boxed = Boxed { value: 42 }
  choice match {
Choice.First => consume(boxed),
Choice.Second => boxed.value,
  }
}
let main(): i32 = { choose(Choice.First) }
"#,
    )
    .unwrap();
}

#[test]
fn carries_guard_moves_into_later_match_candidates() {
    let errors = compile_text(
        r#"
let Choice = enum {
  Only,
}
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): bool = { boxed.value == 0 }
let choose(choice: Choice): i32 = {
  let boxed = Boxed { value: 42 }
  choice match {
Choice.Only if consume(boxed) => 0,
Choice.Only => boxed.value,
  }
}
let main(): i32 = { choose(Choice.Only) }
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
if outer == 40 { return outer }
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
  do { break 42 }
} }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("`break` cannot be used outside")));
}

#[test]
fn do_transparently_forwards_throws_unsafe_and_custom_effects() {
    compile_resolved_text(
        r#"
use core.effects.{Throws, Unsafe}

let UI = effect
let fail(flag: bool): i32 with(Throws(bool)) = {
  if flag { throw(true) }
  40
}
let render(value: i32): i32 with(UI) = { value }
let combined(pointer: Ptr(i32)): i32 with(Throws(bool), Unsafe, UI) = { do {
  let attempted = fail(false)
  let value = render(attempted)
  if value == 40 { return *pointer }
  0
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("do should forward the complete active effect row through its closure boundary");

    let errors = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let fail(): i32 with(Throws(i64)) = { throw(1) }
let outer(): i32 with(Throws(bool)) = { do { return fail() } }
let main(): i32 = { 0 }
"#,
    )
    .unwrap_err();
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("requires `core::effects::Throws(i64)`")
    }));
}

#[test]
fn algebraic_effect_operations_are_typed_and_require_their_instantiated_row() {
    compile_text(
        r#"
let State(S: type) = effect {
  let get(): S
  let put(move value: S): ()
}
let read(): i32 with(State(i32)) = { State(i32).get() }
let write(value: i32): () with(State(i32)) = { State(i32).put(value) }
let main(): i32 = { 0 }
"#,
    )
    .expect("typed operations may propagate their exact effect instance");

    let missing = compile_text(
        r#"
let State(S: type) = effect { let get(): S }
let read(): i32 = { State(i32).get() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("an operation cannot run in a row that omits its effect");
    assert!(missing.iter().any(|error| {
        error.message.contains("requires custom effect") && error.message.contains("State(i32)")
    }));

    let wrong_instance = compile_text(
        r#"
let State(S: type) = effect { let get(): S }
let read(): i32 with(State(i64)) = { State(i32).get() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("different effect applications have different identities");
    assert!(wrong_instance.iter().any(|error| {
        error.message.contains("requires custom effect") && error.message.contains("State(i32)")
    }));
}

#[test]
fn algebraic_handlers_preserve_operation_and_frame_residual_effects() {
    compile_text(
        r#"
let IO = effect
let Ask = effect { let value(): i32 with(IO) }
let run(): i32 with(IO) = { Ask.handle(value: { (resume) -> resume(42) }) {
  Ask.value()
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("handling Ask should leave an operation's IO requirement active");

    compile_text(
        r#"
let Supply = effect { let seed(): i32 }
let Ask = effect { let value(): i32 with(Supply) }
let main(): i32 = {
  Supply.handle(seed: { (resume) -> resume(0) }) {
Ask.handle(value: { (resume) -> resume(42) }) { Ask.value() }
  }
}
"#,
    )
    .expect("an outer algebraic handler should satisfy an inner operation's residual row");

    compile_text(
        r#"
let Supply = effect { let seed(): i32 }
let Ask = effect { let value(): i32 with(Supply) }
let request(): i32 with(Ask, Supply) = { Ask.value() }
let inner(): i32 with(Supply) = {
  Ask.handle(value: { (resume) -> resume(42) }) { request() }
}
let main(): i32 = {
  Supply.handle(seed: { (resume) -> resume(0) }) { inner() }
}
"#,
    )
    .expect("lexical handler capabilities should cross generated named CPS frames");

    compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let Supply = effect { let seed(): i32 }
let Ask = effect { let value(): i32 with(Supply, Throws(bool)) }
let request(): i32 with(Ask, Supply, Throws(bool)) = { Ask.value() }
let inner(): i32 with(Supply, Throws(bool)) = {
  Ask.handle(value: { (resume) -> resume(42) }) { request() }
}
let main(): i32 = {
  let result: Result(bool)(i32) = try {
Supply.handle(seed: { (resume) -> resume(0) }) { inner() }
  }
  result ?? 0
}
"#,
    )
    .expect("throws answers should compose across nested named handler frames");

    let missing_operation_effect = compile_text(
        r#"
let IO = effect
let Ask = effect { let value(): i32 with(IO) }
let run(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  Ask.value()
} }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling one effect must not erase an operation's other effects");
    assert!(missing_operation_effect
        .iter()
        .any(|error| error.message.contains("requires custom effect `IO`")));

    let missing_frame_effect = compile_text(
        r#"
let IO = effect
let Ask = effect { let value(): i32 }
let request(): i32 with(Ask, IO) = { Ask.value() }
let run(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  request()
} }
let main(): i32 = { run() }
"#,
    )
    .expect_err("a specialized resumable frame must retain effects other than Ask");
    assert!(missing_frame_effect
        .iter()
        .any(|error| error.message.contains("requires custom effect `IO`")));

    compile_resolved_text(
        r#"
use core.effects.{Throws, Unsafe}

let AskUnsafe = effect { let value(): i32 with(Unsafe) }
let unsafe_run(): i32 = { unsafe { AskUnsafe.handle(value: { (resume) -> resume(42) }) {
  AskUnsafe.value()
} } }
let AskThrows = effect { let value(): i32 with(Throws(bool)) }
let throwing_run(): i32 with(Throws(bool)) = {
  AskThrows.handle(value: { (resume) -> resume(42) }) { AskThrows.value() }
}
let AskFrame = effect { let value(): i32 }
let throwing_request(): i32 with(AskFrame, Throws(bool)) = { AskFrame.value() }
let throwing_frame(): i32 with(Throws(bool)) = {
  AskFrame.handle(value: { (resume) -> resume(42) }) { throwing_request() }
}
let main(): i32 = { 0 }
"#,
    )
    .expect("unsafe and throws requirements should survive operation handling");

    let missing_unsafe = compile_resolved_text(
        r#"
use core.Result
use core.effects.Unsafe

let Ask = effect { let value(): i32 with(Unsafe) }
let run(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) { Ask.value() } }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling an operation must not authorize its unsafe requirement");
    assert!(missing_unsafe
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));

    let missing_throws = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let Ask = effect { let value(): i32 with(Throws(bool)) }
let run(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) { Ask.value() } }
let main(): i32 = { run() }
"#,
    )
    .expect_err("handling an operation must not erase its throws requirement");
    assert!(missing_throws.iter().any(|error| {
        error
            .message
            .contains("requires `core::effects::Throws(bool)`")
            && error.message.contains("core::effects::Throws(bool)")
    }));
}

#[test]
fn algebraic_effect_operations_overload_only_by_argument_names() {
    compile_text(
        r#"
let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}
let choose(): i32 with(Ask) = { Ask.value(left: 19) + Ask.value(right: 23) }
let main(): i32 = { Ask.handle(
  value: { (left, resume) -> resume(left) },
  value: { (right, resume) -> resume(right) }
) { choose() } }
"#,
    )
    .expect("named arguments and clause parameters should select effect operation overloads");

    let positional = compile_text(
        r#"
let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}
let choose(): i32 with(Ask) = { Ask.value(42) }
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
let Ask = effect { let value(): i32 }
let ask(): i32 with(Ask) = { Ask.value() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let action = ask
  let forwarded = action
  forwarded()
} }
"#,
    )
    .expect("a chained local alias should retain its statically known effectful target");

    compile_text(
        r#"
let Ask = effect { let value(): i32 }
let ask(): i32 with(Ask) = { Ask.value() }
let consume(action: (): i32 with(Ask)): i32 with(Ask) = { action() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let action = ask
  consume(action)
} }
"#,
    )
    .expect("a known function argument should specialize a higher-order effectful frame");

    compile_text(
        r#"
let Ask = effect { let value(): i32 }
let ask_left(): i32 with(Ask) = { Ask.value() }
let ask_right(): i32 with(Ask) = { Ask.value() }
let consume(action: (): i32 with(Ask)): i32 with(Ask) = { action() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let action: (): i32 with(Ask) = if true { ask_left } else { ask_right }
  let forwarded = action
  consume(forwarded)
} }
"#,
    )
    .expect("an aliased dynamic target should specialize a higher-order resumable frame");

    compile_text(
        r#"
let Ask = effect { let value(): i32 }
let ask_left(): i32 with(Ask) = { Ask.value() }
let ask_right(): i32 with(Ask) = { Ask.value() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let action: (): i32 with(Ask) = if true { ask_left } else { ask_right }
  let escaped = action
  escaped()
} }
"#,
    )
    .expect("a dynamic selection tag may be copied into an immutable handler-local alias");

    compile_text(
        r#"
let Ask = effect { let value(): i32 }
let ask_left(): i32 with(Ask) = { Ask.value() }
let ask_right(): i32 with(Ask) = { Ask.value() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let action: (): i32 with(Ask) = if true { ask_left } else { ask_right }
  let other: (): i32 with(Ask) = if true { ask_right } else { ask_left }
  let mut changed = action
  changed = other
  changed()
} }
"#,
    )
    .expect("mutable dynamic aliases remap assignments between equal finite target sets");

    let incompatible_alias = compile_text(
        r#"
let Ask = effect { let value(): i32 }
let first(): i32 with(Ask) = { Ask.value() }
let second(): i32 with(Ask) = { Ask.value() }
let third(): i32 with(Ask) = { Ask.value() }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(42) }) {
  let left: (): i32 with(Ask) = if true { first } else { second }
  let right: (): i32 with(Ask) = if true { first } else { third }
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
let Ask = effect {
  let choose(): bool
  let value(): i32
}
let main(): i32 = { Ask.handle(
  choose: { (resume) -> resume(false) },
  value: { (resume) -> resume(40) },
) {
  let left_base = 1
  let right_base = 2
  let left: (): i32 with(Ask) = { () -> Ask.value() + left_base }
  let right: (): i32 with(Ask) = { () -> Ask.value() + right_base }
  let first: (): i32 with(Ask) = if true { left } else { right }
  let second: (): i32 with(Ask) = if false { right } else { left }
  let combined: (): i32 with(Ask) = if Ask.choose() { first } else { second }
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
let Ask = effect { let value(): i32 }
let Payload = struct { value: i32 }
let consume(move payload: Payload): i32 = { payload.value }
let main(): i32 = { Ask.handle(value: { (resume) -> resume(1) }) {
  let left_payload = Payload { value: 20 }
  let right_payload = Payload { value: 21 }
  let left: (): i32 with(Ask) = { () -> Ask.value() + consume(left_payload) }
  let right: (): i32 with(Ask) = { () -> Ask.value() + consume(right_payload) }
  let action: (): i32 with(Ask) = if true { left } else { right }
  let first = action()
  first + action()
} }
"#,
    )
    .expect_err("a selected FnOnce resumable closure cannot be invoked twice");
    assert!(errors.iter().any(|error| {
        error.message.contains("closure")
            && (error.message.contains("consumed") || error.message.contains("moved"))
    }));
}

#[test]
fn effectful_guards_inspect_noncopy_inputs_without_committing_payload_moves() {
    compile_text(
        r#"
let Ask = effect { let accept(): bool }
let Payload = struct { value: i32 }
let Event = enum { Value(value: Payload), Empty }
let main(): i32 = { Ask.handle(accept: { (resume) -> resume(false) }) {
  let event = Event.Value(value: Payload { value: 42 })
  event match {
Event.Value(value: _) if Ask.accept() => 0,
Event.Value(value: _) => 42,
Event.Empty => 0,
  }
} }
"#,
    )
    .expect("a binding-free effectful guard may inspect a non-Copy enum input");

    compile_text(
        r#"
let Ask = effect { let accept(): bool }
let Payload = struct { value: i32 }
let Event = enum { Value(value: Payload), Empty }
let consume(move payload: Payload): i32 = { payload.value }
let main(): i32 = { Ask.handle(accept: { (resume) -> resume(false) }) {
  let event = Event.Value(value: Payload { value: 42 })
  event match {
Event.Value(value: payload) if Ask.accept() => consume(payload),
Event.Value(value: payload) => consume(payload),
Event.Empty => 0,
  }
} }
"#,
    )
    .expect("a successful guard path can commit non-Copy bindings before entering its body");

    compile_text(
        r#"
let Ask = effect { let accept(): bool }
let Payload = struct { value: i32 }
let Event = enum { Value(value: Payload), Empty }
let main(): i32 = { Ask.handle(accept: { (resume) -> resume(false) }) {
  let event = Event.Value(value: Payload { value: 42 })
  event match {
Event.Value(value: payload) if Ask.accept() && payload.value > 0 => 1,
Event.Value(value: _) => 42,
Event.Empty => 0,
  }
} }
"#,
    )
    .expect("a suspended guard reconstructs projected non-Copy bindings from its owned input");

    let moving_guard_binding = compile_text(
        r#"
let Ask = effect { let accept(): bool }
let Payload = struct { value: i32 }
let Event = enum { Value(value: Payload), Empty }
let consume(move payload: Payload): bool = { payload.value > 0 }
let main(): i32 = { Ask.handle(accept: { (resume) -> resume(false) }) {
  let event = Event.Value(value: Payload { value: 42 })
  event match {
Event.Value(value: payload) if Ask.accept() && consume(payload) => 1,
Event.Value(value: _) => 42,
Event.Empty => 0,
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
let UI = effect
let render(): i32 with(UI) = { 42 }
let invoke(E: effect)(action: (): i32 with(E))(): i32 with(E) = { action() }
let screen(): i32 with(UI) = { invoke(render)() }
let main(): i32 = { 0 }
"#,
    )
    .expect("a function may forward a declared marker effect");

    let missing = compile_text(
        r#"
let UI = effect
let render(): i32 with(UI) = { 42 }
let screen(): i32 = { render() }
let main(): i32 = { screen() }
"#,
    )
    .expect_err("a pure caller cannot invoke a custom-effect function");
    assert!(missing
        .iter()
        .any(|error| error.message.contains("requires custom effect `UI`")));

    let unknown = compile_text(
        r#"
let render(): i32 with(UI) = { 42 }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("custom effects are nominal declarations");
    assert!(unknown
        .iter()
        .any(|error| error.message.contains("unknown custom effect `UI`")));

    let entry = compile_text(
        r#"
let UI = effect
let main(): i32 with(UI) = { 0 }
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
let UI = effect
let pure(): i32 = { 42 }
let render(): i32 with(UI) = { 42 }
let accept_ui(action: (): i32 with(UI))(): i32 with(UI) = { action() }
let screen(): i32 with(UI) = {
  let widened: (): i32 with(UI) = pure
  accept_ui(pure)() + widened()
}
let main(): i32 = { 0 }
"#,
    )
    .expect("a callable with fewer requirements may fill a wider effect slot");

    let narrowing = compile_text(
        r#"
let UI = effect
let render(): i32 with(UI) = { 42 }
let accept_pure(action: (): i32)(): i32 = { action() }
let screen(): i32 with(UI) = { accept_pure(render)() }
let main(): i32 = { 0 }
"#,
    )
    .expect_err("an effectful callable cannot fill a pure slot");
    assert!(narrowing.iter().any(|error| {
        error.message.contains("expected `(): i32`")
            && error.message.contains("found `(): i32 with(UI)`")
    }));

    compile_resolved_text(
        r#"
use core.effects.Unsafe

let pure(): i32 = { 42 }
let accept_unsafe(action: (): i32 with(Unsafe))(): i32 with(Unsafe) = { action() }
let main(): i32 = { unsafe { accept_unsafe(pure)() } }
"#,
    )
    .expect("pure named functions use the same pointer ABI in unsafe callable slots");
}

#[test]
fn unsafe_effects_are_declared_forwarded_and_handled_at_calls() {
    let ir = compile_resolved_text(
        r#"
use core.effects.Unsafe

let read(pointer: Ptr(i32)): i32 with(Unsafe) = { *pointer }
let forward(pointer: Ptr(i32)): i32 with(Unsafe) = { read(pointer) }
let main(): i32 = {
  let value = 42
  unsafe { forward(Ptr(borrow(value))) }
}
"#,
    )
    .expect("an unsafe handler should discharge the declared effect");
    assert!(ir.contains(&format!("call i32 @{}(", function_symbol("forward"))));

    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let read(pointer: Ptr(i32)): i32 with(Unsafe) = { *pointer }
let main(): i32 = {
  let value = 42
  read(Ptr(borrow(value)))
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
fn unsafe_effect_checks_survive_aliasing_and_partial_application() {
    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let read(pointer: Ptr(i32))(offset: i32): i32 with(Unsafe) = { *pointer + offset }
let main(): i32 = {
  let value = 40
  let named = read
  let pending = named(Ptr(borrow(value)))
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
use core.effects.Unsafe

let read(pointer: Ptr(i32))(offset: i32): i32 with(Unsafe) = { *pointer + offset }
let main(): i32 = {
  let value = 40
  let pending = read(Ptr(borrow(value)))
  unsafe { pending(2) }
}
"#,
    )
    .expect("the effect is required only when the final parameter group is applied");
}

#[test]
fn unsafe_effects_participate_in_method_and_trait_signatures() {
    compile_resolved_text(
        r#"
use core.effects.Unsafe

let Reader = struct { pointer: Ptr(i32) }
let Read = trait {
  let read(self: borrow(Self))(): i32 with(Unsafe)
}
extend Reader: Read {
  let read(self: borrow(Self))(): i32 with(Unsafe) = { *self.pointer }
}
let main(): i32 = {
  let value = 42
  let reader = Reader { pointer: Ptr(borrow(value)) }
  unsafe { reader.read() }
}
"#,
    )
    .expect("methods should carry their declared unsafe effect");

    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let Reader = struct { pointer: Ptr(i32) }
let Read = trait {
  let read(self: borrow(Self))(): i32 with(Unsafe)
}
extend Reader: Read {
  let read(self: borrow(Self))(): i32 = { unsafe { *self.pointer } }
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
fn entry_point_cannot_export_an_unsafe_effect() {
    let errors =
        compile_resolved_text("use core.effects.Unsafe\nlet main(): i32 with(Unsafe) = { 42 }\n")
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
use core.effects.Unsafe

let tagged(E: effect)(value: i32): i32 with(E) = { value }
let forward(E: effect)(value: i32): i32 with(E) = { tagged(E)(value) }
let main(): i32 = { forward(20) + forward(pure)(20) + unsafe { forward(E: Unsafe)(2) } }
"#,
    )
    .expect("effect arguments should specialize the function call requirement");

    compile_resolved_text(
        r#"
use core.effects.Unsafe

let identity(E: effect, T: type)(value: T): T with(E) = { value }
let main(): i32 = { identity(20) + unsafe { identity(E: Unsafe, T: i32)(22) } }
"#,
    )
    .expect("effect and type parameters should coexist in one inferred compile group");

    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let tagged(E: effect)(value: i32): i32 with(E) = { value }
let forward(E: effect)(value: i32): i32 with(E) = { tagged(E)(value) }
let main(): i32 = { forward(Unsafe)(42) }
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
use core.effects.Unsafe

let read(E: effect)(pointer: Ptr(i32)): i32 with(E) = { *pointer }
let main(): i32 = {
  let value = 42
  unsafe { read(Unsafe)(Ptr(borrow(value))) }
}
"#,
    )
    .expect("the selected unsafe row should authorize the generic body");

    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let read(E: effect)(pointer: Ptr(i32)): i32 with(E) = { *pointer }
let main(): i32 = {
  let value = 42
  read(Ptr(borrow(value)))
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
let tagged(E: effect)(value: i32): i32 with(E) = { value }
let main(): i32 = { tagged(E: copy)(42) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("invalid effect argument")));

    let errors = compile_resolved_text(
        r#"
use core.effects.Unsafe

let always(E: effect)(value: i32): i32 with(Unsafe, E) = { value }
let main(): i32 = { always(pure)(42) }
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));
}

#[test]
fn effect_parameters_specialize_inherent_methods() {
    compile_resolved_text(
        r#"
use core.effects.Unsafe

let Value = struct { value: i32 }
extend Value {
  let tagged(E: effect)(self: borrow(Self))(): i32 with(E) = { self.value }
}
let main(): i32 = {
  let value = Value { value: 42 }
  unsafe { value.tagged(Unsafe)() }
}
"#,
    )
    .expect("inherent methods should retain generic effect rows");
}

#[test]
fn throws_effects_lower_to_result_boundaries_and_propagate() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let fail(flag: bool): i32 with(Throws(bool)) = { if flag { throw(true) } else { 41 } }
let forward(flag: bool): i32 with(Throws(bool)) = { fail(flag) }
let main(): i32 = {
  let result: Result(bool)(i32) = try { forward(false) }
  result ?? 0
}
"#,
    )
    .expect("throws effects should use a Result ABI with automatic propagation");
    assert!(ir.contains("define internal %sali.type."));
}

#[test]
fn throws_and_unsafe_share_one_effect_row() {
    compile_resolved_library_text(
        r#"
use core.effects.{Throws, Unsafe}

let read(pointer: Ptr(i32), fail: bool): i32 with(Throws(bool), Unsafe) = {
  if fail { throw(true) }
  *pointer
}
let forward(pointer: Ptr(i32), fail: bool): i32 with(Throws(bool), Unsafe) = {
  read(pointer, fail) }
"#,
    )
    .expect("standard Throws should propagate while Unsafe remains a separate call requirement");

    let errors = compile_resolved_text(
        r#"
use core.effects.{Throws, Unsafe}

let read(pointer: Ptr(i32)): i32 with(Throws(bool), Unsafe) = { *pointer }
let main(): i32 = {
  let value = 42
  read(Ptr(borrow(value)))
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("requires an `unsafe` handler")));
}

#[test]
fn try_handles_throws_while_forwarding_unsafe_and_custom_effects() {
    compile_resolved_text(
        r#"
use core.Result
use core.effects.Unsafe

let UI = effect
let render(value: i32): i32 with(UI) = { value }
let read(pointer: Ptr(i32)): i32 with(Unsafe) = { *pointer }
let handle(pointer: Ptr(i32)): Result(bool)(i32) with(Unsafe, UI) = { try {
  let value = read(pointer)
  return render(value)
} }
let main(): i32 = { 0 }
"#,
    )
    .expect("try should remove only throws and forward the rest of the active effect row");
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
        .any(|error| { error.message.contains("FnMut") && error.message.contains("mutable") }));
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
let Payload = struct { value: i32 }
let take(move payload: Payload): i32 = { payload.value }
let main(): i32 = {
  let payload = Payload { value: 42 }
  let invoke = { take(payload) }
  invoke()
}
"#,
    )
    .unwrap();
    let symbol = function_symbol("__closure.0");
    assert!(ir.contains(&format!(
        "define internal i32 @{symbol}(%sali.type.5061796c6f6164 %arg.0)"
    )));
    assert!(ir.contains("; closure capture"));
    assert!(ir.contains(&format!("call i32 @{symbol}(%sali.type.5061796c6f6164")));
}

#[test]
fn rejects_calling_an_fn_once_closure_twice() {
    let errors = compile_text(
        r#"
let Payload = struct { value: i32 }
let take(move payload: Payload): i32 = { payload.value }
let main(): i32 = {
  let payload = Payload { value: 42 }
  let invoke = { take(payload) }
  invoke()
  invoke()
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("FnOnce") && error.message.contains("consumed") }));
}

#[test]
fn rejects_calling_a_resource_partial_application_twice() {
    let errors = compile_text(
        r#"
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let finish(move resource: Resource)(value: i32): i32 = { value }
let main(): i32 = {
  let pending = finish(Resource { value: 1 })
  pending(1)
  pending(2)
}
"#,
    )
    .expect_err("a partial with a move capture must be FnOnce");
    assert!(errors.iter().any(|error| {
        error.message.contains("FnOnce partial application") && error.message.contains("consumed")
    }));
}

#[test]
fn moves_an_fn_once_source_when_the_closure_is_created() {
    let errors = compile_text(
        r#"
let Payload = struct { value: i32 }
let take(move payload: Payload): i32 = { payload.value }
let main(): i32 = {
  let payload = Payload { value: 42 }
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
fn rejects_partial_application_of_a_curried_closure() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  let add_one = add(1)
  add_one(1)
}
"#,
    )
    .unwrap_err();
    assert!(errors
        .iter()
        .any(|error| error.message.contains("partial application")));
}

#[test]
fn emits_kind_discriminated_inherent_receiver_abis() {
    let ir = compile_text(
        r#"
let Counter = struct { value: i32 }
extend Counter {
  let read(self: borrow(Self))(): i32 = { self.value }
  let reset(self: borrow(mut)(Self))(): () = { self.value = 0 }
  let take(move self)(): i32 = { self.value }
  let answer = 1
}
let main(): i32 = {
  let mut counter = Counter { value: 42 }
  let value = counter.read()
  counter.reset()
  value + counter.take() + Counter.answer
}
"#,
    )
    .unwrap();
    let read = function_symbol("Counter::method::read");
    let reset = function_symbol("Counter::method::reset");
    let take = function_symbol("Counter::method::take");
    let answer = global_symbol("Counter::constant::answer");
    assert!(ir.contains(&format!("define internal i32 @{read}(ptr %arg.0)")));
    assert!(ir.contains(&format!("define internal void @{reset}(ptr %arg.0)")));
    assert!(ir.contains(&format!(
        "define internal i32 @{take}(%sali.type.436f756e746572 %arg.0)"
    )));
    assert!(ir.contains(&format!("call i32 @{read}(ptr")));
    assert!(ir.contains(&format!("call void @{reset}(ptr")));
    assert!(ir.contains(&format!("call i32 @{take}(%sali.type.436f756e746572")));
    assert!(ir.contains(&format!("@{answer} = internal unnamed_addr constant i32 1")));
}

#[test]
fn registers_generic_trait_metadata_and_emits_static_method_dispatch() {
    let program = crate::parser::parse(
        r#"
let Convert(Rhs: type) = trait {
  let Output: type
  let convert(self: borrow(Self))(move rhs: Rhs): Output
}
let Number = struct { value: i32 }
extend Number: Convert(i32) {
  let Output = i32
  let convert(self: borrow(Self))(move rhs: i32): i32 = { self.value + rhs }
}
let main(): i32 = {
  let number = Number { value: 40 }
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
    assert_eq!(analyzer.trait_impls.len(), 1);
    let (key, implementation) = analyzer.trait_impls.iter().next().unwrap();
    assert_eq!(key.self_ty, Ty::Struct("Number".into()));
    assert_eq!(key.trait_ref.name, "Convert");
    assert_eq!(key.trait_ref.arguments, vec![Ty::I32]);
    assert_eq!(implementation.associated_types["Output"], Ty::I32);
    let canonical = trait_method_name(key, "convert");
    assert_eq!(implementation.methods["convert"], canonical);

    analyzer.analyze().expect("concrete trait program HIR");
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
		let Functor = trait(Self: (Value: type): type) {
		  let map(E: effect, A: type, B: type)(
		    move self: Self(A),
		  )(
		    move transform: (A): B with(E),
		  ): Self(B) with(E)
		}
	let Chain = trait {
	  let Item: type
	  let Rebind(Value: type): type
	  let chain(E: effect, U: type)(
	    move self
	  )(
	    move transform: (Item): U with(E)
	  ): Rebind(U) with(E)
	}
	let main(): i32 = { 0 }
	"#,
    )
    .expect("higher-kinded trait source must parse");

    let analyzer = Analyzer::new(&program);
    assert!(
        analyzer.diagnostics.is_empty(),
        "unexpected HKT trait diagnostics: {:?}",
        analyzer.diagnostics
    );
    assert_eq!(
        analyzer.traits["Functor"].self_parameter.kind,
        CompileParamKind::TypeConstructor { parameter_count: 1 }
    );

    compile(&program).expect("higher-kinded trait declaration must compile");
}

#[test]
fn higher_kinded_trait_inheritance_requires_constructor_supertraits() {
    let source = r#"
	let Functor = trait(Self: (Value: type): type) {
	  let map(E: effect, A: type, B: type)(
	    move self: Self(A),
	  )(
	    move transform: (A): B with(E),
	  ): Self(B) with(E)
	}
let Applicative = trait(Self: (Value: type): type)
where Self: Functor {
  let pure(A: type)(move value: A): Self(A)}
let Carrier(T: type) = struct { value: T }
extend Carrier: Applicative {
  let pure(A: type)(move value: A): Carrier(A) = {
Carrier(A) { value: value }
  }}
let main(): i32 = { 0 }
"#;
    let errors = compile_text(source)
        .expect_err("Applicative without its inherited Functor implementation must fail");
    assert!(errors
        .iter()
        .any(|error| { error.message.contains("requires `Functor` for `Carrier`") }));

    compile_text(
        r#"
	let Functor = trait(Self: (Value: type): type) {
	  let map(E: effect, A: type, B: type)(
	    move self: Self(A),
	  )(
	    move transform: (A): B with(E),
	  ): Self(B) with(E)
	}
let Applicative = trait(Self: (Value: type): type)
where Self: Functor {
  let pure(A: type)(move value: A): Self(A)}
let Carrier(T: type) = struct { value: T }
extend Carrier: Applicative {
  let pure(A: type)(move value: A): Carrier(A) = {
Carrier(A) { value: value }
  }}
	extend Carrier: Functor {
  let map(E: effect, A: type, B: type)(
	    move self: Carrier(A),
	  )(
	    move transform: (A): B with(E),
	  ): Carrier(B) with(E) = {
	    Carrier(B) { value: transform(self.value) }
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
let Monad = trait(Self: (Value: type): type) {}
let Carrier(T: type) = struct { value: T }
extend Carrier: Monad {}
let keep(M: (Value: type): type, A: type)(move value: M(A)): M(A)
where M: Monad = {
  value
}

let main(): i32 = {
  let kept = keep(M: Carrier)(Carrier(i32) { value: 42 })
  kept.value
}
"#,
    )
    .expect("generic function should accept explicit constructor arguments");
    let identity = function_instance_name(&FunctionInstanceKey {
        template: "keep".into(),
        arguments: vec![Ty::Struct(type_constructor_marker("Carrier")), Ty::I32],
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
let Bad = trait(Self: (Value: type): type) {
  let read(move value: Self): ()
}
let main(): i32 = { 0 }
"#,
            "expects 1 type arguments, found 0",
        ),
        (
            r#"
let Bad = trait {
  let read(E: effect)(move value: E): ()
}
let main(): i32 = { 0 }
"#,
            "cannot be used as a runtime type",
        ),
        (
            r#"
let Bad = trait(Self: type) {
  let read(E: (Error: type): effect)(): () with(E)
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
let Higher = trait(Self: (Value: type): type) {}
let Tagged(Tag: type) = trait(Self: (Value: type): type) {}
let Carrier(T: type) = struct { value: T }
extend Carrier: Higher {}
extend Carrier: Tagged(i32) {}
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
        .filter(|key| key.target.name == "Carrier")
        .collect::<Vec<_>>();
    assert_eq!(carrier_headers.len(), 2);
    assert!(analyzer.constructor_trait_impl_headers.iter().any(|key| {
        key.target.name == "Carrier"
            && key.target.parameter_count == 1
            && key.trait_ref.name == "Higher"
            && key.trait_ref.arguments.is_empty()
    }));
    assert!(analyzer.constructor_trait_impl_headers.iter().any(|key| {
        key.target.name == "Carrier"
            && key.trait_ref.name == "Tagged"
            && key.trait_ref.arguments == vec![Ty::I32]
    }));

    compile(&program).expect("marker constructor trait implementations must compile");
}

#[test]
fn constructor_trait_implementation_methods_register_generic_templates() {
    let program = crate::parser::parse(
        r#"
	let Functor = trait(Self: (Value: type): type) {
	  let map(E: effect, A: type, B: type)(
	    move self: Self(A),
	  )(
	    move transform: (A): B with(E),
	  ): Self(B) with(E)
	}
let Carrier(T: type) = struct { value: T }
	extend Carrier: Functor {
  let map(E: effect, A: type, B: type)(
	    move self: Carrier(A),
	  )(
	    move transform: (A): B with(E),
	  ): Carrier(B) with(E) = {
	    Carrier(B) { value: transform(self.value) }
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
        .filter(|key| key.target.name == "Carrier")
        .collect::<Vec<_>>();
    assert_eq!(carrier_headers.len(), 1);
    let key = analyzer
        .constructor_trait_impl_headers
        .iter()
        .find(|key| key.target.name == "Carrier")
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
            "Carrier".into(),
            vec![Type::Named("B".into(), Vec::new())]
        ))
    );

    compile(&program).expect("constructor trait method templates must validate");
}

#[test]
fn constructor_trait_receiver_methods_dispatch_from_instances() {
    let program = crate::parser::parse(
        r#"
	let Functor = trait(Self: (Value: type): type) {
	  let map(E: effect, A: type, B: type)(
	    move self: Self(A),
	  )(
	    move transform: (A): B with(E),
	  ): Self(B) with(E)
	}
let Carrier(T: type) = struct { value: T }
	extend Carrier: Functor {
  let map(E: effect, A: type, B: type)(
	    move self: Carrier(A),
	  )(
	    move transform: (A): B with(E),
	  ): Carrier(B) with(E) = {
	    Carrier(B) { value: transform(self.value) }
	  }}
let add_one(x: i32): i32 = { x + 1 }
let main(): i32 = {
	  let value = Carrier(i32) { value: 41 }.map(add_one)
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
        .find(|key| key.target.name == "Carrier")
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
        "expected call to constructor trait method instance in IR:\n{ir}"
    );
}

#[test]
fn core_option_and_result_implement_monad() {
    compile_resolved_text(
        r#"
use core.Option
use core.Result
use core.functional.Monad

let add_one(value: i32): i32 = {
  value + 1
}

let option_next(value: i32): Option(i32) = {
  Option(i32).Some(value + 1)
}

let result_next(value: i32): Result(bool)(i32) = {
  Result(bool)(i32).Ok(value + 2)
}

let read_option(value: Option(i32)): i32 = {
  value match {
Some(number) => number,
None => 0,
  }
}

let read_result(value: Result(bool)(i32)): i32 = {
  value match {
Ok(number) => number,
Err(_) => 0,
  }
}

let main(): i32 = {
  let option = Option(i32).Some(39).flat_map(option_next)
  let result = Result(bool)(i32).Ok(1).flat_map(result_next)
  let mapped_option = Option(i32).Some(1).map(add_one)
  let mapped_result = Result(bool)(i32).Ok(2).map(add_one)
  let pure_option: Option(i32) = Option.pure(3)
  let pure_result: Result(bool)(i32) = Result.pure(4)
  let option_transform: Option((i32): i32) = Option.Some(add_one)
  let result_transform: Result(bool)((i32): i32) = Result.Ok(add_one)
  let applied_option = option_transform.apply(Option(i32).Some(5))
  let applied_result = result_transform.apply(Result(bool)(i32).Ok(6))
  read_option(option) + read_result(result) + read_option(mapped_option) + read_result(mapped_result) + read_option(pure_option) + read_result(pure_result) + read_option(applied_option) + read_result(applied_result) - 70
}
"#,
    )
    .expect("core Option and Result should dispatch Monad.flat_map");
}

#[test]
fn constructor_trait_implementation_headers_report_current_limits() {
    for (source, expected) in [
        (
            r#"
let Higher = trait(Self: (Value: type): type) {}
let Carrier(T: type) = struct { value: T }
extend Carrier: Higher {}
extend Carrier: Higher {}
let main(): i32 = { 0 }
"#,
            "duplicate constructor trait implementation",
        ),
        (
            r#"
let Higher = trait(Self: (Left: type, Right: type): type) {}
let Carrier(T: type) = struct { value: T }
extend Carrier: Higher {}
let main(): i32 = { 0 }
"#,
            "expects a constructor with 2",
        ),
        (
            r#"
	let Functor = trait(Self: (Value: type): type) {
	  let map(E: effect, A: type, B: type)(
	    move self: Self(A),
	  )(
	    move transform: (A): B with(E),
	  ): Self(B) with(E)
	}
let Carrier(T: type) = struct { value: T }
extend Carrier: Functor {}
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
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(Number) {
  let Output = i32
  let add(move self)(move rhs: Number): i32 = { self.value + rhs.value }
}
let main(): i32 = { Number { value: 40 } + Number { value: 2 } }
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Add".into(),
            arguments: vec![Ty::Struct("Number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "add"));
    let ir = compile(&program).expect("core Add source must compile");
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
use core.ops.{Sub, Mul, Div, Rem}
let Number = struct { value: i32 }
extend Number: Sub(Number) {
  let Output = Number
  let sub(move self)(move rhs: Number): Number = { Number { value: self.value - rhs.value } }
}
extend Number: Mul(Number) {
  let Output = Number
  let mul(move self)(move rhs: Number): Number = { Number { value: self.value * rhs.value } }
}
extend Number: Div(Number) {
  let Output = Number
  let div(move self)(move rhs: Number): Number = { Number { value: self.value / rhs.value } }
}
extend Number: Rem(Number) {
  let Output = Number
  let rem(move self)(move rhs: Number): Number = { Number { value: self.value % rhs.value } }
}
let main(): i32 = {
  let a = Number { value: 18 }
  let b = Number { value: 5 }
  (((a - b) * Number { value: 2 }) / Number { value: 3 } % Number { value: 4 }).value
}
"#,
    );
    let ir = compile(&program).expect("arithmetic trait dispatch must compile");
    for (trait_name, method) in [
        ("core::ops::Sub", "sub"),
        ("core::ops::Mul", "mul"),
        ("core::ops::Div", "div"),
        ("core::ops::Rem", "rem"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("Number".into()),
            trait_ref: TraitRefKey {
                name: trait_name.into(),
                arguments: vec![Ty::Struct("Number".into())],
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
use core.ops.Eq
let Number = struct { value: i32 }
extend Number: Eq(Number) {
  let eq(self: borrow(Self))(rhs: borrow(Number)): bool = { self.value == rhs.value }
}
let main(): i32 = {
  let left = Number { value: 21 }
  let right = Number { value: 21 }
  if left == right && !(left != right) { 42 } else { 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Eq".into(),
            arguments: vec![Ty::Struct("Number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "eq"));
    let ir = compile(&program).expect("core Eq source must compile");
    assert_eq!(ir.matches(&format!("call i1 @{symbol}(")).count(), 2);
    assert!(ir.contains("xor i1"), "`!=` must negate the Eq result");
}

#[test]
fn lowers_partial_ord_operators_through_four_state_results() {
    let program = resolve_text(
        r#"
use core.ops.{PartialOrd, PartialOrdering}
let Number = struct { value: i32, unordered: bool }
extend Number: PartialOrd(Number) {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Number)): PartialOrdering = {
if self.unordered || rhs.unordered { Unordered }
else if self.value < rhs.value { Less }
else if self.value > rhs.value { Greater }
else { Equal } }
}
let main(): i32 = {
  let low = Number { value: 1, unordered: false }
  let high = Number { value: 2, unordered: false }
  let none = Number { value: 0, unordered: true }
  if low < high && low <= high && high > low && high >= low &&
!(none < low) && !(none <= low) && !(none > low) && !(none >= low) {
42
  } else { 0 }
}
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::PartialOrd".into(),
            arguments: vec![Ty::Struct("Number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "partial_cmp"));
    let ir = compile(&program).expect("core PartialOrd source must compile");
    assert_eq!(
        ir.lines()
            .filter(|line| line.contains(" call ") && line.contains(&format!("@{symbol}(")))
            .count(),
        8
    );
    assert!(ir.contains("switch i32"));
}

#[test]
fn lowers_unary_operator_traits_to_consuming_static_calls() {
    let program = resolve_text(
        r#"
use core.ops.{Neg, Not}
let Number = struct { value: i32 }
let Flag = struct { value: bool }
extend Number: Neg {
  let Output = i32
  let neg(move self)(): i32 = { -self.value }}
extend Flag: Not {
  let Output = i32
  let not(move self)(): i32 = { if self.value { 0 } else { 42 } }
}
let negate(T: type)(move value: T): T where T: Neg(Output = T) = { -value }
let invert(T: type)(move value: T): T where T: Not(Output = T) = { !value }
let main(): i32 = { if invert(false) {
  !Flag { value: false } + -Number { value: 0 } + negate(0)
} else { 0 } }
"#,
    );
    let ir = compile(&program).expect("core unary operator source must compile");
    for (ty, trait_name, method) in [("Number", "Neg", "neg"), ("Flag", "Not", "not")] {
        let key = TraitImplKey {
            self_ty: Ty::Struct(ty.into()),
            trait_ref: TraitRefKey {
                name: format!("core::ops::{trait_name}"),
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
use core.ops.{BitAnd, BitOr, BitXor, Shl, Shr}
let Bits = struct { value: i32 }
extend Bits: BitAnd(Bits) {
  let Output = Bits
  let bit_and(move self)(move rhs: Bits): Bits = { Bits { value: self.value & rhs.value } }
}
extend Bits: BitOr(Bits) {
  let Output = Bits
  let bit_or(move self)(move rhs: Bits): Bits = { Bits { value: self.value | rhs.value } }
}
extend Bits: BitXor(Bits) {
  let Output = Bits
  let bit_xor(move self)(move rhs: Bits): Bits = { Bits { value: self.value ^ rhs.value } }
}
extend Bits: Shl(Bits) {
  let Output = Bits
  let shl(move self)(move rhs: Bits): Bits = { Bits { value: self.value << rhs.value } }
}
extend Bits: Shr(Bits) {
  let Output = Bits
  let shr(move self)(move rhs: Bits): Bits = { Bits { value: self.value >> rhs.value } }
}
let mask(T: type)(move left: T)(move right: T): T
where T: BitAnd(T, Output = T) = { left & right }
let unsigned_shift(value: u32): u32 = { value >> 2 }
let main(): i32 = {
  let value = ((((mask(Bits { value: 6 })(Bits { value: 3 }) | Bits { value: 8 }) ^ Bits { value: 3 }) << Bits { value: 1 }) >> Bits { value: 1 }).value
  let builtins = (6 & 3) == 2 && (2 | 8) == 10 && (10 ^ 3) == 9 &&
(9 << 1) == 18 && (-8 >> 2) == -2 && unsigned_shift(8) == 2
  if value == 9 && builtins { 42 } else { 0 }
}
"#,
    );
    let ir = compile(&program).expect("bitwise operator source must compile");
    for (trait_name, method) in [
        ("BitAnd", "bit_and"),
        ("BitOr", "bit_or"),
        ("BitXor", "bit_xor"),
        ("Shl", "shl"),
        ("Shr", "shr"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("Bits".into()),
            trait_ref: TraitRefKey {
                name: format!("core::ops::{trait_name}"),
                arguments: vec![Ty::Struct("Bits".into())],
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
fn unary_operator_traits_report_missing_output_and_move_errors() {
    let missing = compile_resolved_text(
        "let Number = struct { value: i32 }\nlet main(): i32 = { (-Number { value: 1 }).value }\n",
    )
    .unwrap_err();
    assert!(missing.iter().any(|error| {
        error
            .message
            .contains("no matching `Neg` implementation for unary `-`")
    }));

    let mismatch = compile_resolved_text(
        r#"
use core.ops.Neg
let Number = struct { value: i32 }
extend Number: Neg {
  let Output = i32
  let neg(move self)(): i32 = { -self.value }}
let main(): bool = { -Number { value: 1 } }
"#,
    )
    .unwrap_err();
    assert!(mismatch.iter().any(|error| {
        error
            .message
            .contains("no matching `Neg` implementation for unary `-`")
    }));

    let moved = compile_resolved_text(
        r#"
use core.ops.Neg
let Resource = struct { value: i32 }
extend Resource: Neg {
  let Output = Resource
  let neg(move self)(): Resource = { self }}
let main(): i32 = {
  let value = Resource { value: 42 }
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

    assert_eq!(ir.matches("call void @llvm.trap()").count(), 4);
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
    .expect_err("signed MIN % -1 must not reach LLVM srem undefined behavior");

    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("constant arithmetic overflows `i32`")
    }));
}

#[test]
fn a_unique_operator_candidate_must_match_the_expected_output() {
    let errors = compile_resolved_text(
        r#"
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(i32) {
  let Output = bool
  let add(move self)(move rhs: i32): bool = { self.value == rhs }
}
let main(): i32 = { Number { value: 42 } + 42 }
"#,
    )
    .expect_err("a unique Add implementation cannot ignore its expected Output");

    assert!(errors.iter().any(|error| {
        error.message.contains("no matching `Add` implementation")
            && error.message.contains("producing `i32`")
    }));
}

#[test]
fn uninhabited_operator_output_coerces_when_no_exact_output_exists() {
    compile_resolved_text(
        r#"
use core.ops.Sub
let Number = struct { value: i32 }
extend Number: Sub(i32) {
  let Output = Never
  let sub(move self)(move rhs: i32): Never = { loop {} }
}
let main(): i32 = { Number { value: 42 } - 1 }
"#,
    )
    .expect("an uninhabited operator Output must coerce to the expected type");
}

#[test]
fn exact_operator_output_takes_precedence_over_uninhabited_output() {
    let program = resolve_text(
        r#"
use core.ops.Sub
let Number = struct { value: i32 }
extend Number: Sub(i32) {
  let Output = Never
  let sub(move self)(move rhs: i32): Never = { loop {} }
}
extend Number: Sub(i64) {
  let Output = i32
  let sub(move self)(move rhs: i64): i32 = { 42 }
}
let main(): i32 = { Number { value: 42 } - 1 }
"#,
    );
    let exact = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Sub".into(),
            arguments: vec![Ty::I64],
        },
    };
    let uninhabited = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Sub".into(),
            arguments: vec![Ty::I32],
        },
    };
    let ir = compile(&program).expect("exact Output must resolve the operator candidate");
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
use core.ops.Sub
let Number = struct { value: i32 }
extend Number: Sub(i32) {
  let Output = i32
  let sub(move self)(move rhs: i32): i32 = { self.value - rhs }
}
extend Number: Sub(bool) {
  let Output = i32
  let sub(move self)(move rhs: bool): i32 = { if rhs { 42 } else { 0 } }
}
let main(): i32 = { Number { value: 1 } - do {
  let flag = true
  flag
} }
"#,
    );
    let boolean = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Sub".into(),
            arguments: vec![Ty::Bool],
        },
    };
    let integer = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Sub".into(),
            arguments: vec![Ty::I32],
        },
    };
    let ir = compile(&program).expect("the RHS block type must select Sub(bool)");
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
use core.ops.Sub
let Number = struct { value: i32 }
extend Number: Sub(i32) {
  let Output = i64
  let sub(move self)(move rhs: i32): i64 = { 42 }
}
let identity(T: type)(move value: T): T = { value }
let main(): i32 = {
  let answer = identity(Number { value: 44 } - 2)
  if answer == 42 { 42 } else { 0 }
}
"#,
    )
    .expect("Sub Output must be visible to generic inference");
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
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(i32) {
  let Output = i32
  let add(move self)(move rhs: i32): i32 = { self.value + rhs }
}
let identity(T: type)(move value: T): T = { value }
let main(): i32 = { identity(Number { value: 40 } + 2) }
"#,
    )
    .expect("Add Output must be visible to generic inference");
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
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(i32) {
  let Output = i64
  let add(move self)(move rhs: i32): i64 = { 0 }
}
extend Number: Add(i64) {
  let Output = i64
  let add(move self)(move rhs: i64): i64 = { rhs }
}
let main(): i32 = {
  let answer: i64 = Number { value: 0 } + do { 2147483648 }
  if answer == 2147483648 { 42 } else { 0 }
}
"#,
    );
    let i32_key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Add".into(),
            arguments: vec![Ty::I32],
        },
    };
    let i64_key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Add".into(),
            arguments: vec![Ty::I64],
        },
    };
    let i32_symbol = function_symbol(&trait_method_name(&i32_key, "add"));
    let i64_symbol = function_symbol(&trait_method_name(&i64_key, "add"));
    let ir = compile(&program).expect("large literal must select Add { value: i64 }");
    assert!(ir.contains(&format!("call i64 @{i64_symbol}(")));
    assert!(!ir.contains(&format!("call i64 @{i32_symbol}(")));
}

#[test]
fn add_lowering_is_independent_of_inferred_producer_declaration_order() {
    let program = resolve_text(
        r#"
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = { Number { value: self.value + rhs.value } }
}
let main(): i32 = {
  let answer = make() + Number { value: 2 }
  answer.value
}
let make() = { Number { value: 40 } }
"#,
    );
    let key = TraitImplKey {
        self_ty: Ty::Struct("Number".into()),
        trait_ref: TraitRefKey {
            name: "core::ops::Add".into(),
            arguments: vec![Ty::Struct("Number".into())],
        },
    };
    let symbol = function_symbol(&trait_method_name(&key, "add"));
    let ir = compile(&program).expect("later inferred producer must support overloaded Add");
    assert!(ir.contains(&format!("call %sali.type.4e756d626572 @{symbol}(")));
}

#[test]
fn builtin_add_is_independent_of_inferred_producer_declaration_order() {
    let ir = compile_text(
        r#"
let main(): i32 = { make() + 2 }
let make() = { 40 }
"#,
    )
    .expect("later inferred integer producer must support built-in Add");
    assert!(ir.contains("add i32"));
}

#[test]
fn add_reports_when_no_ambiguous_candidate_has_the_expected_output() {
    let errors = compile_resolved_text(
        r#"
use core.ops.Add
let Number = struct { value: i32 }
extend Number: Add(i32) {
  let Output = bool
  let add(move self)(move rhs: i32): bool = { false }
}
extend Number: Add(i64) {
  let Output = bool
  let add(move self)(move rhs: i64): bool = { true }
}
let main(): i32 = { Number { value: 40 } + 2 }
"#,
    )
    .expect_err("expected output must reject every Add candidate");
    assert!(errors.iter().any(|error| {
        error.message.contains("no matching `Add` implementation")
            && error.message.contains("producing `i32`")
    }));
}

#[test]
fn trait_method_bodies_resolve_concrete_trait_type_substitutions() {
    let ir = compile_text(
        r#"
let Cell(T: type) = struct { value: T }
let Factory(T: type) = trait {
  let Output: type
  let make(self: borrow(Self))(move value: T): Output
}
let Maker = struct { seed: i32 }
extend Maker: Factory(i32) {
  let Output = Cell(i32)
  let make(self: borrow(Self))(move value: i32): Cell(i32) = { Cell(T) { value: value + self.seed } }
}
let main(): i32 = {
  let maker = Maker { seed: 0 }
  maker.make(42).value
}
"#,
    )
    .expect("trait method compile-time substitutions must resolve");
    let instance = NominalInstanceKey {
        kind: NominalKind::Struct,
        template: "Cell".into(),
        arguments: vec![Ty::I32],
    };
    assert!(ir.contains(&hex_name(&nominal_instance_name(&instance))));
}

#[test]
fn trait_associated_functions_dispatch_from_the_implementing_type() {
    let ir = compile_text(
        r#"
let Construct(T: type) = trait {
  let construct(move value: T): Self
}
let Number = struct { value: i32 }
extend Number: Construct(i32) {
  let construct(move value: i32): Number = { Number { value: value } }
}
let main(): i32 = { Number.construct(42).value }
"#,
    )
    .expect("receiver-free trait function must dispatch through its implementing type");

    assert!(ir.contains("ret i32 42") || ir.contains("i32 42"));
}

#[test]
fn nominal_error_types_propagate_through_throws() {
    let ir = compile_resolved_text(
        r#"
use core.Result
use core.effects.Throws

let Failure = struct { code: i32 }
let read(fail: bool): i32 with(Throws(Failure)) = {
  if fail { throw(Failure { code: 1 }) } else { 40 } }
let run(fail: bool): i32 with(Throws(Failure)) = { read(fail) + 2 }
let main(): i32 = {
  let result: Result(Failure)(i32) = try { run(false) }
  result match { Ok(value) => value, Err(_) => 0 }
}
"#,
    )
    .expect("nominal errors should use the same automatic throws propagation");

    assert!(ir.contains("add i32"));
    assert!(ir.contains("switch i32"));
}

#[test]
fn inherent_methods_take_precedence_over_trait_candidates() {
    let program = crate::parser::parse(
        r#"
let Answer = trait {
  let answer(self: borrow(Self))(): i32
}
let Number = struct { value: i32 }
extend Number: Answer {
  let answer(self: borrow(Self))(): i32 = { 1 }
}
extend Number {
  let answer(self: borrow(Self))(): i32 = { self.value }
}
let main(): i32 = {
  let number = Number { value: 42 }
  number.answer()
}
"#,
    )
    .expect("method precedence source must parse");
    let analyzer = Analyzer::new(&program);
    let trait_key = analyzer.trait_impls.keys().next().unwrap();
    let trait_symbol = function_symbol(&trait_method_name(trait_key, "answer"));
    let inherent_symbol = function_symbol(&inherent_method_name("Number", "answer"));
    let ir = compile(&program).expect("method precedence source must compile");
    assert!(ir.contains(&format!("call i32 @{inherent_symbol}(ptr")));
    assert!(!ir.contains(&format!("call i32 @{trait_symbol}(ptr")));
}

#[test]
fn rejects_unsupported_gats_and_associated_cycles() {
    let cases = [
        (
            r#"
	let Generic = trait {
	  let Item(T: type): type
	}
	let Node = struct { value: i32 }
	extend Node: Generic {
	  let Item = i32
		}
		let main(): i32 = { 0 }
		"#,
            "must name a generic type constructor",
        ),
        (
            r#"
let Cycle = trait {
  let A: type
  let B: type
}
let Node = struct { value: i32 }
extend Node: Cycle {
  let A = B
  let B = A
}
let main(): i32 = { 0 }
"#,
            "associated type cycle",
        ),
        (
            r#"
let Broken = trait {
  let read(self: borrow(Self))(): Missing
}
let main(): i32 = { 0 }
"#,
            "unknown type `Missing`",
        ),
        (
            r#"
let Conflict(T: type) = trait {
  let T: type
}
let main(): i32 = { 0 }
"#,
            "conflicts with a trait type parameter",
        ),
        (
            r#"
let Read = trait {
  let read(self: borrow(Self))(): i32
}
let Number = struct { value: i32 }
extend Number: Read {
  let read(value: borrow(Number))(): i32 = { value.value }
}
let main(): i32 = { 0 }
"#,
            "signature mismatch",
        ),
        (
            r#"
let Boxed = struct { value: i32 }
let InvalidCopy = trait {
  let consume(self: borrow(Self))(copy value: Boxed): i32
}
let main(): i32 = { 0 }
"#,
            "requires `Copy`",
        ),
        (
            r#"
let Read = trait {
  let read(self: borrow(Self))(): i32
}
let Number = struct { value: i32 }
extend Number: Read {}
extend Number: Read {
  let read(self: borrow(Self))(): i32 = { self.value }
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
let Broken = trait {
  let value(self: borrow(Self))(): i32 = { missing }
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
let Cell(T: type) = struct { value: T }
let Reader = trait {
  let read(self: borrow(Self))(copy value: Cell(i32)): i32
}

let Host = struct { value: i32 }
extend Host: Reader {
  let read(self: borrow(Self))(copy value: Cell(i32)): i32 = { self.value + value.value }
}
extend Cell(i32): Copy {}
let main(): i32 = {
  let host = Host { value: 19 }
  let cell = Cell(i32) { value: 23 }
  host.read(cell) + cell.value - 23
}
"#,
    )
    .expect("trait schemas must see concrete Copy implementations collected later in source");
}

#[test]
fn emits_resource_array_drop_glue_for_unconstructed_layout_fields() {
    let ir = compile_text(
        r#"
let Payload = struct { value: i32 }
extend Payload: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let Holder = struct { values: Array(Payload, 1) }
let main(): i32 = { 42 }
"#,
    )
    .expect("resource arrays are valid even when their containing layout is not constructed");
    let payload = Ty::Struct("Payload".to_owned());
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
let Boxed = struct { value: i32 }
extend Boxed {
  let identity(value: Self): Self = { value }
}
let main(): i32 = { Boxed.identity(Boxed { value: 42 }).value }
"#,
    )
    .unwrap();
}

#[test]
fn keeps_same_named_method_and_associated_function_symbols_distinct() {
    let ir = compile_text(
        r#"
let Number = struct { raw: i32 }
extend Number {
  let value(self: borrow(Self))(): i32 = { self.raw }
  let value(): i32 = { 2 }
}
let main(): i32 = {
  let number = Number { raw: 40 }
  number.value() + Number.value()
}
"#,
    )
    .unwrap();
    let method = function_symbol("Number::method::value");
    let function = function_symbol("Number::function::value");
    assert_ne!(method, function);
    assert!(ir.contains(&format!("@{method}")));
    assert!(ir.contains(&format!("@{function}")));
}

#[test]
fn emits_dynamic_array_bounds_check_before_inbounds_gep_and_hoists_allocas() {
    let ir = compile_text(
        r#"
let read(values: Array(i32, 2), index: i32): i32 = { values[index] }
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
}

#[test]
fn rejects_array_lengths_beyond_the_first_version_limit() {
    let errors = compile_text(
        r#"
let main(): i32 = {
  let values: Array(i32, 2147483648) = [42]
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
  while true {
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
let Boxed = struct { value: i32 }
let consume(move value: Boxed): i32 = { value.value }
let main(): i32 = {
  let boxed = Boxed { value: 42 }
  loop {
break consume(boxed)
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
  break
}

answer = answer + 2
break answer
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
        "use core.Option\n\
         use core.iter.{Iterator, IntoIterator}\n\
         let Counter = struct { current: i32, end: i32 }\n\
         extend Counter {\n\
           let into_iter(self: borrow(Self))(): i32 = { self.current }\n\
           let next(self: borrow(Self))(): bool = { false }\n\
         }\n\
         extend Counter: Iterator {\n\
           let Item = i32\n\
           let next(self: borrow(mut)(Self))(): Option(i32) = {\n\
             if self.current < self.end {\n\
               let value = self.current\n\
               self.current = self.current + 1\n\
               Some(value)\n\
             } else { None }\n\
           }\n}\n\
         extend Counter: IntoIterator {\n\
           let IntoIter = Counter\n\
           let into_iter(move self)(): Counter = { self }\n}\n\
         let main(): i32 = {\n\
           let mut sum = 0\n\
           for value in Counter { current: 0, end: 4 } {\n\
             sum = sum + value\n\
           }\n\
           sum\n\
         }\n",
    )
    .expect("for loop must lower through IntoIterator and Iterator");
    assert!(ir.contains("define internal i32 @sali.fn.6d61696e"));
    for (trait_name, method) in [
        ("core::iter::Iterator", "next"),
        ("core::iter::IntoIterator", "into_iter"),
    ] {
        let key = TraitImplKey {
            self_ty: Ty::Struct("Counter".to_owned()),
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
use core.Option
use core.Result

let Payload = struct { value: i32, nested: Option(i32) }
extend Payload {
  let add(self: borrow(Self))(amount: i32): i32 = { self.value + amount }
}
let read(value: Option(Payload)): Option(i32) = { value?.value }
let nested(value: Option(Payload)): Option(Option(i32)) = { value?.nested }
let call(value: Result(bool)(Payload)): Result(bool)(i32) = { value?.add(2) }
let main(): i32 = {
  let boxed = Payload { value: 40, nested: Option(i32).Some(42) }
  let left = read(Option(Payload).Some(boxed)) ?? 0
  let right = call(Result(bool)(Payload).Ok(Payload { value: 0, nested: Option(i32).None })) ?? 0
  left + right
}
"#,
    )
    .unwrap();
    assert!(ir.contains("switch i32"));
    assert!(ir.contains(&function_symbol("Payload::method::add")));
}

#[test]
fn optional_chain_expected_result_only_constrains_the_base_error_type() {
    compile_resolved_text(
        r#"
use core.Result

let Boxed = struct { value: i32 }
let read(): Result(bool)(i32) = { Result.Ok(Boxed { value: 42 })?.value }
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
let Payload = struct { value: i32 }
let read(value: borrow(Option(Payload))): Option(i32) = { value?.value }
let main(): i32 = { 0 }
"#,
            "owned",
        ),
        (
            r#"
let Payload = struct { value: i32 }
extend Payload { let reset(self: borrow(mut)(Self))(): i32 = { self.value } }
let read(value: Option(Payload)): Option(i32) = { value?.reset() }
let main(): i32 = { 0 }
"#,
            "mutable-borrow receiver",
        ),
        (
            r#"
let Payload = struct { value: i32 }
extend Payload { let add(self: borrow(Self))(x: i32)(y: i32): i32 = { self.value + x + y } }
let read(value: Option(Payload)): Option(i32) = { value?.add(1) }
let main(): i32 = { 0 }
"#,
            "fully applied",
        ),
        (
            r#"
let Payload = struct { value: i32 }
let read(value: Payload): Option(i32) = { value?.value }
let main(): i32 = { 0 }
"#,
            "requires an owned `Option(T)`, `Result(E)(T)`, or `Chain` value",
        ),
    ];
    for (source, expected) in cases {
        let source = format!("use core.Option\n{source}");
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
if flag { return outer + inner }
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
  if flag { break 7 }
  break 9
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
let Boxed = struct { value: i32 }
let spin(): i32 = { loop {} }
let make(): Boxed = { Boxed { value: spin() } }
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
let Boxed = struct { value: i32 }
let make(flag: bool): Boxed = { loop {
  if flag { break Boxed { value: 41 } }
  break Boxed { value: 42 }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let make(): Pair = { loop {
  break Pair { left: Payload { value: 1 }, right: break Pair { left: Payload { value: 2 }, right: Payload { value: 3 } } }
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
let Payload = struct { value: i32 }
let discard(move payload: Payload): () = {
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
let Payload = struct { value: i32 }
let payload = Payload { value: 42 }
let get(): Payload = { payload }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload }
let replace(target: borrow(mut)(Pair), move replacement: Payload): () = {
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
use core.Result
use core.effects.Throws

let read(fail: bool): i32 with(Throws(bool)) = { if fail { throw(true) } else { 42 } }
let propagate(fail: bool): i32 with(Throws(bool)) = {
  let item = read(fail)
  if item == 0 { throw(true) }
  item
}
let main(): i32 = {
  let result: Result(bool)(i32) = try { propagate(false) }
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
let Resource = struct { value: i32 }
let Bundle = struct { left: Resource, right: Resource }
let Choice = enum { Some(Bundle), None }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let consume(move value: Resource): () = { () }
let inspect(move choice: Choice): i32 = { choice match {
  Some(Bundle(left: left, right: _)) if left.value == 1 => do {
consume(left)
1
  },
  Some(_) => 2,
  None => 0
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
    let (_, source) = pattern_transfers[0];
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
let Boxed = struct { value: i32 }
let read(): i32 = {
  let value = Boxed { value: 42 }
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
let Boxed = struct { value: i32 }
let inspect(): () = {
  let value = Boxed { value: 42 }
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
let Payload = struct { value: i32 }
let Choice = enum { Value(value: Payload), Empty }
let make(): Choice = { Choice.Value(value: Payload { value: 7 }) }
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
let Payload = struct { value: i32 }
extend Payload: Copy {}
let make(): Array(Payload, 2) = { [Payload { value: 1 }, Payload { value: 2 }] }
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
let Empty = struct {}
extend Empty: Copy {}
let Pair = struct { left: i32, right: Empty }
extend Pair: Copy {}
let Choice = enum { First(next: Pair), Second(i32), Unit }
let inspect(move empty: Empty, move pair: Pair, move choice: Choice, move values: Array(Pair, 3), alias: borrow(Pair)): () = { () }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let take(): Payload = { Pair { left: Payload { value: 1 }, right: Payload { value: 2 } }.left }
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
let Payload = struct { value: i32 }
extend Payload: Copy {}
let take(): Payload = { [Payload { value: 1 }, Payload { value: 2 }][1] }
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
        "let take(values: Array(i32, 3), index: i32): i32 = { values[index] }\n",
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
let Payload = struct { value: i32 }
let take(): Payload = { [Payload { value: 1 }, Payload { value: 2 }][1] }
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
        compile_library_text("let too_wide(move values: Array(i32, 65536)): () = { () }\n")
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
let Payload = struct { value: i32 }
let stop(): Never = { loop {} }
let choose(move value: Payload, code: i32): Payload = { value }
let make(): Payload = { choose(Payload { value: 1 }, stop()) }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let update(): Pair = {
  let mut old = Pair { left: Payload { value: 0 }, right: Payload { value: 1 } }
  old = Pair { left: Payload { value: 2 }, right: return Pair { left: Payload { value: 3 }, right: Payload { value: 4 } } }
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
let Payload = struct { value: i32 }
let Pair = struct { left: Payload, right: Payload }
let make(): Pair = { Pair { left: Payload { value: 1 }, right: return Pair { left: Payload { value: 2 }, right: Payload { value: 3 } } } }
"#,
        "make",
    );
    assert_partial_child_without_root(&struct_plan, CleanupProjection::Field(0));

    let enum_plan = cleanup_plan_text(
        r#"
let Payload = struct { value: i32 }
let Choice = enum { Pair(Payload, Payload) }
let make(): Choice = { Choice.Pair(Payload { value: 1 }, return Choice.Pair(Payload { value: 2 }, Payload { value: 3 })) }
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
let Payload = struct { value: i32 }
extend Payload: Copy {}
let make(): Array(Payload, 2) = { [Payload { value: 1 }, return [Payload { value: 2 }, Payload { value: 3 }]] }
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
        "let Payload = struct { value: i32 }\nlet make(): Payload = { Payload { value: 1 } }\n",
        "make",
    );
    let explicit = cleanup_plan_text(
        "let Payload = struct { value: i32 }\nlet make(): Payload = { return Payload { value: 1 } }\n",
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
let Payload = struct { value: i32 }
let choose(flag: bool): Payload = {
  if flag { Payload { value: 1 } } else { Payload { value: 2 } }
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
let Choice = enum { First, Second }
let Payload = struct { value: i32 }
let choose(choice: Choice): Payload = { choice match {
  Choice.First => Payload { value: 1 },
  Choice.Second => Payload { value: 2 },
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
let Payload = struct { value: i32 }
let choose(flag: bool): Payload = { loop {
  let inner = loop {
if flag { break Payload { value: 1 } }
break Payload { value: 2 }
  }
  break inner
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
let Payload = struct { value: i32 }
let make(): Payload = { Payload { value: 1 } }
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
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let consume(move value: Resource): () = { () }
let run(): () = {
  let resource = Resource { value: 1 }
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
let Resource = struct { value: i32 }
extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let finish(move resource: Resource)(value: i32): i32 = { value }
let main(): i32 = {
  let pending = finish(Resource { value: 1 })
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
    let plan = cleanup_plan_text("let absurd(move value: Never): i32 = { value }\n", "absurd");
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
let Empty = enum {}
let Holder = struct { value: Empty }
let absurd(move holder: Holder): i32 = { holder.value }
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
let Empty = enum {}
extend Empty: Copy {}
let identity(move values: Array(Empty, 1)): Array(Empty, 1) = { values }
let absurd(move values: Array(Empty, 1)): i32 = { identity(values)[0] }
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
let Boxed = struct { value: i32 }
let make(): Boxed = { Boxed { value: 42 } }
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
  let done = while false {}
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
  let done = loop { break }
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
  done = while false {}
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
let cycle(): () = { while false { 1; () } }
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
let cycle(): Never = { loop { 1; () } }
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
  let done = while loop { break false } {}
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
return 40
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
let stop(): Never = { loop {} }
let bind(): () = {
  let done = while stop() {}
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
let Boxed = struct { value: i32 }
let consume(move boxed: Boxed): () = { () }
let classify(flag: bool): i32 = {
  let mut boxed = Boxed { value: 0 }
  boxed = Boxed { value: 1 }
  consume(boxed)
  boxed = Boxed { value: 2 }
  if flag { consume(boxed) }
  boxed = Boxed { value: 42 }
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
let Boxed = struct { value: i32 }
extend Boxed: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }}
let consume(move value: Boxed): () = { () }
let finish(flag: bool): () = {
  let boxed = Boxed { value: 42 };
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
let Plain = struct { value: i32 }
extend Plain: Copy {}
let finish(): () = { let value = Plain { value: 42 }; () }
"#,
        "finish",
    );
    let value = copy
        .locals
        .iter()
        .find(|local| local.debug_name.as_deref() == Some("value"))
        .expect("Copy local");
    let root = copy
        .move_paths
        .iter()
        .find(|path| path.place.local == value.id && path.parent.is_none())
        .expect("Copy root path");
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
    let mut program = crate::parser::parse(
        r#"
let main(): i32 = {
  let continuation: (i32): i32 = { (value: i32) -> value + 2 }
  let action: (i32, Continuation(i32, i32)): i32 = {
(input: i32, move next: Continuation(i32, i32)) -> invoke_continuation(next, input)
  }
  let erased_continuation = erase_continuation(continuation)
  let erased_action = erase_effect_callable(action)
  invoke_effect_callable(erased_action, 40, erased_continuation)
}
"#,
    )
    .expect("internal effect-callable source must parse");
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

    let llvm = compile(&program).expect("erased CPS action must lower");
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
let Ask = effect { let value(): i32 }
let run()(move action: (): i32 with(Ask)): i32 = {
  Ask.handle(value: { (resume) -> resume(10) }) { action() }
}
let main(): i32 = {
  let mut base = 31
  run() { () ->
base = base + 1
Ask.value() + base
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
let Ask = effect { let value(): i32 }
let run(seed: i32)(move action: (): i32 with(Ask)): i32 = {
  Ask.handle(value: { (resume) -> resume(20) }) { action() + seed }
}
let prepare(order: borrow(mut)(i32)): i32 = {
  order = order + 1
  20
}
let main(): i32 = {
  let mut order = 0
  run(prepare(order)) { () ->
order = order * 2
Ask.value() + order
  }
}
"#,
    )
    .expect("arguments before a direct action must be materialized in source order");
    assert!(llvm.contains("24636170747572696e672468616e646c657224"));
}
