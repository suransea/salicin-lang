use std::collections::{HashMap, HashSet};

use crate::ast::{
    AssociatedKind, CompileParam, Expr, ExtendDef, ExtendMember, Function, FunctionEffects, Item,
    ItemOrigin, PassMode, Program, Sort, StructDef, TraitDef, TraitMember, Type, VariantFields,
    Visibility, WherePredicate,
};
use crate::core::{
    copy_trait_has_required_shape, drop_trait_has_required_shape, move_trait_has_required_shape,
    operator_trait_has_required_shape, unary_operator_trait_has_required_shape, LangItemKind,
};
use crate::modules::PackageId;

use super::compile_time::{
    closed_value_from_marker, describe_compile_sort, effect_row_from_source, effect_row_source,
    source_effect_identity,
};
use super::hir::{AccessBoundary, FunctionSig, ParamSig, Ty};
use super::names::{canonical_type_encoding, generic_parameter_marker};
use super::operators::{BINARY_OPERATOR_TRAITS, UNARY_OPERATOR_TRAITS};
use super::registry::{
    display_parameter_label_shape, function_parameter_labels, overloaded_function_name,
    schema_function_has_receiver, top_level_namespace, FunctionShape,
    GenericConstructorTraitExtensionTarget, GenericTraitExtension, NominalInstanceInfo,
    NominalInstanceKey, NominalInstanceState, NominalKind, ParameterLabelShape, TopLevelNamespace,
    TraitImplKey, TraitRefKey, TraitSchema, TypeConstructorImplTarget,
};
use super::source_rewrite::{
    expand_function_aliases, rewrite_abstract_self_qualified_methods, substitute_function_types,
    substitute_type_parameters,
};
use super::{compile_parameter_sort_label, primitive_scalar_type, remaining_sort_groups, Analyzer};

impl Analyzer {
    pub(super) fn collect_items(
        &mut self,
        core: &Program,
        alloc: &Program,
        std: &Program,
        program: &Program,
    ) {
        let mut names = HashMap::<String, HashSet<TopLevelNamespace>>::new();
        for (kind, namespaces) in [
            (LangItemKind::ArrayTypeForm, &[TopLevelNamespace::Type][..]),
            (LangItemKind::SliceTypeForm, &[TopLevelNamespace::Type][..]),
            (LangItemKind::StrTypeForm, &[TopLevelNamespace::Type][..]),
            (
                LangItemKind::PtrTypeForm,
                &[TopLevelNamespace::Type, TopLevelNamespace::Function][..],
            ),
            (LangItemKind::SizeOf, &[TopLevelNamespace::Function][..]),
            (LangItemKind::AlignOf, &[TopLevelNamespace::Function][..]),
        ] {
            names
                .entry(kind.source_name().to_owned())
                .or_default()
                .extend(namespaces.iter().cloned());
        }
        for reserved in [
            "raw_alloc",
            "raw_dealloc",
            "raw_init",
            "raw_take",
            "raw_offset",
            "raw_borrow",
            "raw_slice",
            "raw_slice_len",
            "raw_slice_at",
            "raw_subview",
            "raw_str",
            "raw_str_bytes",
            "raw_trap",
            "forget",
        ] {
            names
                .entry(reserved.to_owned())
                .or_default()
                .insert(TopLevelNamespace::Other);
        }
        let mut extensions = Vec::new();
        if program.items.len() != program.item_visibilities.len()
            || program.items.len() != program.item_origins.len()
        {
            self.error("program item metadata count does not match item count");
            return;
        }
        let prelude_items = core
            .items
            .iter()
            .zip(&core.item_visibilities)
            .zip(&core.item_origins)
            .map(|((item, visibility), origin)| (item, *visibility, origin.clone()));
        let source_items = program
            .items
            .iter()
            .zip(&program.item_visibilities)
            .zip(&program.item_origins)
            .map(|((item, visibility), origin)| (item, *visibility, origin.clone()));
        let alloc_items = alloc
            .items
            .iter()
            .zip(&alloc.item_visibilities)
            .zip(&alloc.item_origins)
            .map(|((item, visibility), origin)| (item, *visibility, origin.clone()));
        let std_items = std
            .items
            .iter()
            .zip(&std.item_visibilities)
            .zip(&std.item_origins)
            .map(|((item, visibility), origin)| (item, *visibility, origin.clone()));
        let all_items = prelude_items
            .chain(alloc_items)
            .chain(std_items)
            .chain(source_items)
            .collect::<Vec<_>>();
        let mut function_counts = HashMap::<String, usize>::new();
        for (item, _, _) in &all_items {
            if let Item::Function(function) = item {
                *function_counts.entry(function.name.clone()).or_default() += 1;
            }
            if let Item::Sort(definition) = item {
                if let Some(members) = &definition.members {
                    self.collection
                        .closed_type_values
                        .insert(definition.name.clone(), members.clone());
                    if self.is_lang_item_name(&definition.name, LangItemKind::AccessSort) {
                        self.collection
                            .closed_type_values
                            .insert("access".to_owned(), members.clone());
                    }
                    if self.is_lang_item_name(&definition.name, LangItemKind::AbiSort) {
                        self.collection
                            .closed_type_values
                            .insert("abi".to_owned(), members.clone());
                    }
                }
            }
            if let Item::TypeForm(definition) = item {
                if !definition.values.is_empty() {
                    self.collection
                        .closed_type_values
                        .insert(definition.name.clone(), definition.values.clone());
                    if self.is_lang_item_name(&definition.name, LangItemKind::Bool) {
                        self.collection
                            .closed_type_values
                            .insert("bool".to_owned(), definition.values.clone());
                    }
                }
            }
            if let Item::Enum(definition) = item {
                let all_unit = definition
                    .variants
                    .iter()
                    .all(|variant| matches!(variant.fields, VariantFields::Unit));
                if all_unit && !definition.variants.is_empty() {
                    let values = definition
                        .variants
                        .iter()
                        .map(|variant| variant.name.clone())
                        .collect::<Vec<_>>();
                    self.collection
                        .closed_type_values
                        .insert(definition.name.clone(), values.clone());
                    if self.is_lang_item_name(&definition.name, LangItemKind::Bool) {
                        self.collection
                            .closed_type_values
                            .insert("bool".to_owned(), values);
                    }
                }
            }
        }
        let mut overload_shapes = HashMap::<String, HashSet<ParameterLabelShape>>::new();
        let mut overload_visibilities = HashMap::<String, Visibility>::new();
        for (item, visibility, origin) in all_items {
            self.current_origin = Some(Box::new(origin.clone()));
            let name = match item {
                Item::Function(function) => &function.name,
                Item::Global(binding) => &binding.name,
                Item::Struct(definition) => &definition.name,
                Item::Enum(definition) => &definition.name,
                Item::Effect(definition) => &definition.name,
                Item::Sort(definition) => &definition.name,
                Item::TypeAlias(definition) => &definition.name,
                Item::TypeForm(definition) => &definition.name,
                Item::Trait(definition) => &definition.name,
                Item::Extend(extension) => {
                    if origin.package != PackageId::CORE.0
                        && extension.members.iter().any(|member| {
                            matches!(
                                member,
                                ExtendMember::Function(function) if function.builtin
                            )
                        })
                    {
                        self.error(
                            "`builtin()` is private to the core package and cannot define extension methods",
                        );
                        continue;
                    }
                    extensions.push((extension.clone(), origin));
                    continue;
                }
            };
            let namespace = top_level_namespace(item);
            let overloaded_function = matches!(item, Item::Function(_))
                && function_counts.get(name).cloned().unwrap_or_default() > 1;
            let occupied = names.get(name).cloned().unwrap_or_default();
            let duplicate = match namespace {
                TopLevelNamespace::Function => {
                    occupied.contains(&TopLevelNamespace::Other)
                        || (occupied.contains(&TopLevelNamespace::Function) && !overloaded_function)
                }
                TopLevelNamespace::Type => occupied.contains(&TopLevelNamespace::Type),
                TopLevelNamespace::Other => {
                    occupied.contains(&TopLevelNamespace::Function)
                        || occupied.contains(&TopLevelNamespace::Other)
                }
            };
            if duplicate {
                self.error(format!("duplicate top-level name `{name}`"));
                continue;
            }
            names.entry(name.clone()).or_default().insert(namespace);
            match item {
                Item::Function(function) => {
                    let mut function = function.clone();
                    let source_name = function.name.clone();
                    if function.builtin && origin.package != PackageId::CORE.0 {
                        self.error(format!(
                            "`builtin()` is private to the core package and cannot define `{source_name}`"
                        ));
                        continue;
                    }
                    if function.builtin
                        && origin.package == PackageId::CORE.0
                        && [
                            LangItemKind::Builtin,
                            LangItemKind::Foreign,
                            LangItemKind::Test,
                        ]
                        .into_iter()
                        .any(|kind| self.is_lang_item_name(&source_name, kind))
                    {
                        continue;
                    }
                    let transparent_modifier = function.compile_groups.len() == 1
                        && function.compile_groups[0].len() == 1
                        && function.compile_groups[0][0].kind == Sort::ParameterModifier
                        && function.groups.is_empty()
                        && function.return_type.is_none()
                        && function.effects == FunctionEffects::default()
                        && function.where_predicates.is_empty()
                        && matches!(
                            function.body.as_ref(),
                            Some(Expr::Name(name)) if name == &function.compile_groups[0][0].name
                        );
                    if transparent_modifier {
                        self.collection
                            .transparent_parameter_modifiers
                            .insert(source_name.clone());
                        continue;
                    }
                    let parameter_modifier_intrinsic = origin.package == PackageId::CORE.0
                        && [
                            LangItemKind::CopyParameters,
                            LangItemKind::MoveParameters,
                            LangItemKind::ComptimeParameters,
                        ]
                        .into_iter()
                        .any(|kind| self.is_lang_item_name(&source_name, kind))
                        && function.compile_groups.as_slice().iter().flatten().count() == 1
                        && function.compile_groups[0][0].kind == Sort::Parameters
                        && function.groups.is_empty()
                        && matches!(
                            function.return_type.as_ref(),
                            Some(Type::Named(name, arguments))
                                if name == "parameters" && arguments.is_empty()
                        )
                        && function.effects == FunctionEffects::default()
                        && function.where_predicates.is_empty()
                        && function.builtin
                        && function.body.is_none();
                    if parameter_modifier_intrinsic {
                        continue;
                    }
                    for parameter in function.compile_groups.iter().flatten() {
                        let Sort::Named(compile_type) = &parameter.kind else {
                            continue;
                        };
                        let Some(members) = self.collection.closed_type_values.get(compile_type)
                        else {
                            self.error(format!(
                                "compile-time parameter `{}` in `{source_name}` uses unknown closed type `{compile_type}`",
                                parameter.name
                            ));
                            continue;
                        };
                        if let Some(crate::ast::CompileParamDefault::Name(default)) =
                            &parameter.default
                        {
                            if !members.contains(default) {
                                self.error(format!(
                                    "default `{default}` for compile-time parameter `{}` in `{source_name}` is not a member of `{compile_type}`",
                                    parameter.name
                                ));
                            }
                        }
                    }
                    if origin.package != PackageId::CORE.0
                        && matches!(
                            source_name.rsplit("::").next(),
                            Some(
                                "do" | "try"
                                    | "throw"
                                    | "unsafe"
                                    | "loop"
                                    | "while"
                                    | "if"
                                    | "match"
                                    | "for"
                            )
                        )
                    {
                        self.error(format!(
                            "control lang-item name `{}` is reserved for `core.control`",
                            source_name.rsplit("::").next().unwrap()
                        ));
                        continue;
                    }
                    if overloaded_function {
                        if source_name == "main" {
                            self.error("entry point `main` cannot be overloaded");
                            continue;
                        }
                        if overload_visibilities
                            .get(&source_name)
                            .is_some_and(|previous| previous != &visibility)
                        {
                            self.error(format!(
                                "overloads of `{source_name}` must use the same visibility"
                            ));
                            continue;
                        }
                        overload_visibilities
                            .entry(source_name.clone())
                            .or_insert(visibility);
                        let shape = function_parameter_labels(&function);
                        if !overload_shapes
                            .entry(source_name.clone())
                            .or_default()
                            .insert(shape.clone())
                        {
                            self.error(format!(
                                "duplicate overload `{source_name}` with parameter labels {}",
                                display_parameter_label_shape(&shape)
                            ));
                            continue;
                        }
                        function.name = overloaded_function_name(&source_name, &shape);
                        self.collection
                            .function_overloads
                            .entry(source_name)
                            .or_default()
                            .push(function.name.clone());
                    }
                    self.collection.function_accesses.insert(
                        function.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                    if function.compile_groups.is_empty() {
                        for parameter in function.groups.iter().flatten() {
                            for modifier in &parameter.modifiers {
                                self.error(format!(
                                    "parameter modifier `{modifier}` on `{}.{}` does not normalize to a `parameters` schema",
                                    function.name, parameter.name
                                ));
                            }
                        }
                        self.collection.function_order.push(function.name.clone());
                        self.collection
                            .functions
                            .insert(function.name.clone(), function.clone());
                        self.collection
                            .function_origins
                            .insert(function.name.clone(), origin.clone());
                    } else {
                        self.collection
                            .function_template_order
                            .push(function.name.clone());
                        self.collection
                            .function_templates
                            .insert(function.name.clone(), function.clone());
                        self.collection
                            .function_template_origins
                            .insert(function.name.clone(), origin.clone());
                    }
                }
                Item::Global(binding) => {
                    if binding.mutable {
                        self.error(format!(
                            "mutable global `{}` is not supported yet",
                            binding.name
                        ));
                    }
                    self.collection.global_order.push(binding.name.clone());
                    self.collection
                        .globals
                        .insert(binding.name.clone(), binding.clone());
                    self.collection
                        .global_origins
                        .insert(binding.name.clone(), origin.clone());
                    self.collection.global_accesses.insert(
                        binding.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                }
                Item::Struct(definition) => {
                    self.collection.nominal_accesses.insert(
                        definition.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                    if definition.compile_groups.is_empty() {
                        let key = NominalInstanceKey {
                            kind: NominalKind::Struct,
                            template: definition.name.clone(),
                            arguments: Vec::new(),
                        };
                        self.collection
                            .nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.collection.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.collection
                            .nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.collection.struct_order.push(definition.name.clone());
                        self.collection
                            .struct_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        self.collection
                            .struct_template_order
                            .push(definition.name.clone());
                        self.collection
                            .struct_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                    for derive in &definition.derives {
                        match derive.as_str() {
                            "copyable" => {
                                if let Some(extension) = self.derived_copy_extension(definition) {
                                    extensions.push((extension, origin.clone()));
                                }
                            }
                            other => self.error(format!(
                                "unsupported derive `{other}` on struct `{}`",
                                definition.name
                            )),
                        }
                    }
                }
                Item::Enum(definition) => {
                    if self.is_lang_item_name(&definition.name, LangItemKind::Bool) {
                        continue;
                    }
                    self.collection.nominal_accesses.insert(
                        definition.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                    if definition.compile_groups.is_empty() {
                        let key = NominalInstanceKey {
                            kind: NominalKind::Enum,
                            template: definition.name.clone(),
                            arguments: Vec::new(),
                        };
                        self.collection
                            .nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.collection.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.collection
                            .nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.collection.enum_order.push(definition.name.clone());
                        self.collection
                            .enum_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        self.collection
                            .enum_template_order
                            .push(definition.name.clone());
                        self.collection
                            .enum_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                }
                Item::Effect(definition) => {
                    if definition.compile_groups.len() > 1
                        || definition
                            .compile_groups
                            .iter()
                            .flatten()
                            .any(|parameter| parameter.kind != Sort::Type)
                    {
                        self.error(format!(
                            "effect `{}` currently accepts one compile-time group containing only `type` parameters",
                            definition.name
                        ));
                    }
                    self.collection.effects.insert(definition.name.clone());
                    self.collection
                        .effect_defs
                        .insert(definition.name.clone(), definition.clone());
                }
                Item::Sort(definition) => {
                    if definition.members.is_none() && origin.package != PackageId::CORE.0 {
                        self.error(format!(
                            "abstract sort `{}` is compiler-owned; user sorts must declare a finite member set with `= sort(1) {{ ... }}`",
                            definition.name
                        ));
                    }
                }
                Item::TypeForm(definition) => {
                    if definition.builtin && origin.package != PackageId::CORE.0 {
                        self.error(format!(
                            "`builtin()` is private to the core package and cannot define type `{}`",
                            definition.name
                        ));
                    }
                }
                Item::TypeAlias(_) => {
                    unreachable!("type aliases are expanded before item collection")
                }
                Item::Trait(definition) => {
                    self.collect_trait_schema(definition.clone(), visibility, origin)
                }
                Item::Extend(_) => unreachable!("extensions were collected separately"),
            }
        }

        self.validate_program_effects(core);
        self.validate_program_effects(alloc);
        self.validate_program_effects(program);

        self.validate_generic_nominal_cycles();
        self.collect_nominal_layouts();
        for (extension, _) in &extensions {
            if extension.trait_ref.is_some() {
                continue;
            }
            let Type::Named(target, arguments) = &extension.target else {
                continue;
            };
            if extension.compile_groups.is_empty() && !arguments.is_empty() {
                continue;
            }
            for member in &extension.members {
                let ExtendMember::Function(function) = member else {
                    continue;
                };
                let is_method = schema_function_has_receiver(function);
                *self
                    .collection
                    .inherent_overload_counts
                    .entry((target.clone(), function.name.clone(), is_method))
                    .or_default() += 1;
            }
        }
        let mut remaining_extensions = Vec::new();
        for (extension, origin) in extensions {
            self.current_origin = Some(Box::new(origin.clone()));
            if self.is_core_copy_extension(&extension) {
                self.collect_extension(extension, origin);
            } else {
                remaining_extensions.push((extension, origin));
            }
        }
        self.validate_copy_implementations();
        self.activate_generic_copy_extensions();
        self.validate_copy_implementations();
        self.collection.copy_impls_finalized = true;
        self.validate_trait_schemas();
        for (extension, origin) in remaining_extensions {
            self.current_origin = Some(Box::new(origin.clone()));
            self.collect_extension(extension, origin);
        }
        self.current_origin = None;
        self.validate_trait_inheritance_implementations();

        let never = self.lang_item_name(LangItemKind::Never);
        if !self.collection.enum_defs.contains_key(never) {
            self.error("compiler core did not register its validated `Never` declaration");
        }

        for name in self.collection.function_order.clone() {
            let function = self.collection.functions[&name].clone();
            let groups = function
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|param| ParamSig {
                            name: param.name.clone(),
                            ty: self.lower_source_type(&param.ty),
                            mode: param.mode,
                        })
                        .collect()
                })
                .collect();
            let result = function
                .return_type
                .as_ref()
                .map(|ty| self.lower_source_type(ty));
            let failure_error = function
                .effects
                .failure
                .as_deref()
                .map(|error| self.lower_source_type(error));
            self.lowering.signatures.insert(
                name,
                FunctionSig {
                    groups,
                    unsafety: self.function_effects_unsafe(&function.effects),
                    failure_error,
                    custom_effects: self.function_effects_custom_identities(&function.effects),
                    result,
                },
            );
        }
        self.register_runtime_handler_actions();
        for name in self.collection.global_order.clone() {
            let binding = self.collection.globals[&name].clone();
            let annotation = binding
                .annotation
                .as_ref()
                .map(|ty| self.lower_source_type(ty));
            self.lowering.global_annotations.insert(name, annotation);
        }

        self.validate_nominal_templates();
        self.validate_function_templates();
    }

    pub(super) fn validate_program_effects(&mut self, program: &Program) {
        fn functions(item: &Item) -> Vec<&Function> {
            match item {
                Item::Function(function) => vec![function],
                Item::Trait(definition) => definition
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        TraitMember::Function(function) => Some(function),
                        TraitMember::AssociatedType { .. } => None,
                    })
                    .collect(),
                Item::Extend(extension) => extension
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        ExtendMember::Function(function) => Some(function),
                        ExtendMember::Const(_) => None,
                    })
                    .collect(),
                Item::Global(_)
                | Item::Struct(_)
                | Item::Enum(_)
                | Item::Sort(_)
                | Item::TypeForm(_) => Vec::new(),
                Item::Effect(definition) => definition.operations.iter().collect(),
                Item::TypeAlias(_) => Vec::new(),
            }
        }

        for function in program.items.iter().flat_map(functions) {
            for effect in &function.effects.custom {
                let Type::Named(name, arguments) = effect else {
                    self.error(format!(
                        "custom effect in function `{}` must be a nominal effect application",
                        function.name
                    ));
                    continue;
                };
                if !self.collection.effects.contains(name) {
                    self.error(format!(
                        "unknown custom effect `{}` in function `{}`",
                        source_effect_identity(effect),
                        function.name
                    ));
                } else if let Some(definition) = self.collection.effect_defs.get(name) {
                    let expected = definition.compile_groups.iter().flatten().count();
                    if arguments.len() != expected {
                        self.error(format!(
                            "effect argument count mismatch for `{name}` in function `{}`: expected {expected}, found {}",
                            function.name,
                            arguments.len()
                        ));
                    }
                }
            }
        }
    }

    pub(super) fn is_core_copy_extension(&self, extension: &ExtendDef) -> bool {
        matches!(
            extension.trait_ref.as_ref(),
            Some(Type::Named(name, arguments))
                if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty()
        )
    }

    pub(super) fn derived_copy_extension(&mut self, definition: &StructDef) -> Option<ExtendDef> {
        let parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        for parameter in &parameters {
            if parameter.kind != Sort::Type {
                self.error(format!(
                    "struct `{}` cannot derive `copyable` with non-type compile-time parameter `{}`",
                    definition.name, parameter.name
                ));
                return None;
            }
        }
        let arguments = parameters
            .iter()
            .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
            .collect::<Vec<_>>();
        let where_predicates = parameters
            .iter()
            .map(|parameter| WherePredicate {
                subject: Type::Named(parameter.name.clone(), Vec::new()),
                trait_ref: Type::Named(
                    self.lang_item_name(LangItemKind::Copy).to_owned(),
                    Vec::new(),
                ),
                associated_types: Vec::new(),
            })
            .collect();
        Some(ExtendDef {
            compile_groups: definition.compile_groups.clone(),
            target: Type::Named(definition.name.clone(), arguments),
            trait_ref: Some(Type::Named(
                self.lang_item_name(LangItemKind::Copy).to_owned(),
                Vec::new(),
            )),
            where_predicates,
            members: Vec::new(),
        })
    }

    pub(super) fn validate_copy_implementations(&mut self) {
        let copy_name = self.lang_item_name(LangItemKind::Copy);
        let mut candidates = self
            .collection
            .trait_impls
            .iter()
            .filter(|(key, _)| {
                key.trait_ref.name == copy_name && key.trait_ref.arguments.is_empty()
            })
            .map(|(key, _)| (key.self_ty.clone(), key.clone()))
            .collect::<Vec<_>>();
        candidates.sort_by(|(left, _), (right, _)| {
            canonical_type_encoding(left).cmp(&canonical_type_encoding(right))
        });

        let mut valid = HashSet::new();
        loop {
            let previous_len = valid.len();
            for (target, _) in &candidates {
                if self.copy_layout_is_valid(target, &valid) {
                    valid.insert(target.clone());
                }
            }
            if valid.len() == previous_len {
                break;
            }
        }

        for (target, key) in &candidates {
            if valid.contains(target) {
                continue;
            }
            let target_name = self.diagnostic_type_name(target);
            if let Some((member, ty)) = self.first_non_copy_member(target, &valid) {
                let ty = self.diagnostic_type_name(&ty);
                self.error(format!(
                    "`{target_name}` cannot implement `copyable`: {member} has type `{ty}`, which does not implement `copyable`"
                ));
            } else {
                self.error(format!(
                    "`{target_name}` cannot implement `copyable` because its value layout is not copyable"
                ));
            }
            self.collection.trait_impls.remove(key);
        }
        self.collection.copy_nominals = valid;
    }

    pub(super) fn validate_dynamic_copy_implementation(&mut self, key: &TraitImplKey) {
        let target = &key.self_ty;
        if self.copy_layout_is_valid(target, &self.collection.copy_nominals) {
            self.collection.copy_nominals.insert(target.clone());
            return;
        }
        let target_name = self.diagnostic_type_name(target);
        if let Some((member, ty)) =
            self.first_non_copy_member(target, &self.collection.copy_nominals)
        {
            let ty = self.diagnostic_type_name(&ty);
            self.error(format!(
                "`{target_name}` cannot implement `copyable`: {member} has type `{ty}`, which does not implement `copyable`"
            ));
        } else {
            self.error(format!(
                "`{target_name}` cannot implement `copyable` because its value layout is not copyable"
            ));
        }
        self.collection.trait_impls.remove(key);
        self.collection.trait_impl_headers.remove(key);
    }

    pub(super) fn activate_generic_copy_extensions(&mut self) {
        let copy_name = self.lang_item_name(LangItemKind::Copy).to_owned();
        let template_names = self
            .collection
            .generic_trait_extensions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for template_name in template_names {
            let extensions = self
                .collection
                .generic_trait_extensions
                .get(&template_name)
                .cloned()
                .unwrap_or_default();
            let mut retained = Vec::new();
            for extension in extensions {
                let is_copy = matches!(
                    &extension.trait_ref,
                    Type::Named(name, arguments) if name == &copy_name && arguments.is_empty()
                );
                if !is_copy || self.generic_copy_extension_is_structural(&template_name, &extension)
                {
                    retained.push(extension);
                }
            }
            self.collection
                .generic_trait_extensions
                .insert(template_name.clone(), retained.clone());

            let copy_extensions = retained
                .iter()
                .filter(|extension| {
                    matches!(
                        &extension.trait_ref,
                        Type::Named(name, arguments) if name == &copy_name && arguments.is_empty()
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            if copy_extensions.is_empty() {
                continue;
            }
            let existing = self
                .collection
                .nominal_instances
                .iter()
                .filter(|(_, instance)| instance.key.template == template_name)
                .map(|(canonical, instance)| (canonical.clone(), instance.key.arguments.clone()))
                .collect::<Vec<_>>();
            for extension in &copy_extensions {
                for (canonical, arguments) in &existing {
                    let Some(source_arguments) = arguments
                        .iter()
                        .map(|argument| self.source_type_for_ty(argument))
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    self.instantiate_generic_trait_extension(
                        &template_name,
                        canonical,
                        &source_arguments,
                        extension,
                    );
                }
            }
        }
    }

    pub(super) fn generic_copy_extension_is_structural(
        &mut self,
        template_name: &str,
        extension: &GenericTraitExtension,
    ) -> bool {
        let (kind, parameters) =
            if let Some(template) = self.collection.struct_templates.get(template_name) {
                (
                    NominalKind::Struct,
                    template
                        .compile_groups
                        .iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            } else if let Some(template) = self.collection.enum_templates.get(template_name) {
                (
                    NominalKind::Enum,
                    template
                        .compile_groups
                        .iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            } else {
                return false;
            };
        let owner = format!("generic-copy::{template_name}");
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            let marker = generic_parameter_marker(&owner, index, &parameter.name);
            self.collection
                .abstract_type_parameters
                .insert(marker.clone(), parameter.name.clone());
            source_arguments.push(Type::Named(marker.clone(), Vec::new()));
            arguments.push(Ty::Struct(marker));
        }
        let substitutions = extension
            .target_arguments
            .iter()
            .cloned()
            .zip(source_arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let nominals_before = self.snapshot_nominals();
        let copy_before = self.collection.copy_nominals.clone();
        for predicate in &extension.where_predicates {
            if !matches!(&predicate.trait_ref, Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty())
            {
                continue;
            }
            let mut subject = predicate.subject.clone();
            substitute_type_parameters(&mut subject, &substitutions);
            let subject = self.lower_source_type(&subject);
            if subject != Ty::Error {
                self.collection.copy_nominals.insert(subject);
            }
        }
        self.collection.suppress_generic_inherent_instantiation += 1;
        let instance =
            self.ensure_nominal_instance(kind, template_name, source_arguments, arguments);
        self.collection.suppress_generic_inherent_instantiation -= 1;
        let valid = instance.as_ref().is_some_and(|canonical| {
            let target = match kind {
                NominalKind::Struct => Ty::Struct(canonical.clone()),
                NominalKind::Enum => Ty::Enum(canonical.clone()),
            };
            self.copy_layout_is_valid(&target, &self.collection.copy_nominals)
        });
        if !valid {
            self.error(format!(
                "blanket `copyable` implementation for `{template_name}` is not structurally valid for every instance allowed by its where predicates"
            ));
        }
        self.restore_nominals(nominals_before);
        self.collection.copy_nominals = copy_before;
        valid
    }

    pub(super) fn collect_trait_schema(
        &mut self,
        definition: TraitDef,
        visibility: Visibility,
        origin: ItemOrigin,
    ) {
        let mut valid = true;
        if definition.name == self.lang_item_name(LangItemKind::Move)
            && !move_trait_has_required_shape(&definition)
        {
            self.error("`movable` language trait must have shape `let movable = trait {}`");
            valid = false;
        }
        if definition.name == self.lang_item_name(LangItemKind::Copy)
            && !copy_trait_has_required_shape(&definition)
        {
            self.error(
                "`copyable` language trait must have shape `let copyable = trait where self: movable {}`",
            );
            valid = false;
        }
        if definition.name == self.lang_item_name(LangItemKind::Drop)
            && !drop_trait_has_required_shape(&definition)
        {
            self.error(
                "`droppable` language trait must have shape `let droppable = trait { let drop(self: borrow(mut)(self))(): () }`",
            );
            valid = false;
        }
        let operator_trait = BINARY_OPERATOR_TRAITS
            .iter()
            .cloned()
            .find(|candidate| definition.name == self.lang_item_name(candidate.lang_item));
        if let Some(operator_trait) = operator_trait {
            if !operator_trait_has_required_shape(operator_trait.lang_item, &definition) {
                let trait_name = operator_trait.lang_item.source_name();
                let method = operator_trait.method();
                let shape = match operator_trait.lang_item {
                    LangItemKind::Eq => format!(
                        "let eq(comptime rhs: type) = trait {{ let {method}(self: borrow(self))(rhs: borrow(rhs)): bool }}"
                    ),
                    LangItemKind::PartialOrd => format!(
                        "let partial_ord(comptime rhs: type) = trait {{ let {method}(self: borrow(self))(rhs: borrow(rhs)): partial_ordering }}"
                    ),
                    _ => format!(
                        "let {trait_name}(comptime rhs: type) = trait {{ let output: type; let {method}(self)(rhs: rhs): output }}"
                    ),
                };
                self.error(format!(
                    "`{trait_name}` language trait must have shape `{shape}`"
                ));
                valid = false;
            }
        }
        let unary_operator = UNARY_OPERATOR_TRAITS
            .iter()
            .cloned()
            .find(|candidate| definition.name == self.lang_item_name(candidate.lang_item));
        if let Some(operator) = unary_operator {
            if !unary_operator_trait_has_required_shape(operator.lang_item, &definition) {
                let trait_name = operator.lang_item.source_name();
                let method = operator.method();
                self.error(format!(
                    "`{trait_name}` language trait must have shape `let {trait_name} = trait {{ let Output: type; let {method}(self)(): Output }}`"
                ));
                valid = false;
            }
        }
        if definition.compile_groups.len() > 1 {
            self.error(format!(
                "trait `{}` supports at most one compile-time parameter group",
                definition.name
            ));
            valid = false;
        }
        if definition.self_parameter.name != "self" {
            self.error(format!(
                "trait `{}` self sort parameter must be named `self`",
                definition.name
            ));
            valid = false;
        }
        if !matches!(
            definition.self_parameter.kind,
            Sort::Type | Sort::TypeConstructor { .. } | Sort::Effect
        ) {
            self.error(format!(
                "trait `{}` self sort must be `type`, a type-constructor sort, or `effect`, found {}",
                definition.name,
                describe_compile_sort(definition.self_parameter.kind.clone())
            ));
            valid = false;
        }
        let compile_parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let mut compile_parameter_names = HashSet::new();
        for parameter in &compile_parameters {
            if parameter.name == "self" {
                self.error(format!(
                    "trait `{}` cannot declare reserved type parameter `self`",
                    definition.name
                ));
                valid = false;
            }
            if !compile_parameter_names.insert(parameter.name.clone()) {
                self.error(format!(
                    "duplicate type parameter `{}` in trait `{}`",
                    parameter.name, definition.name
                ));
                valid = false;
            }
        }
        let function_counts = definition
            .members
            .iter()
            .filter_map(|member| match member {
                TraitMember::Function(function) => Some(function.name.clone()),
                TraitMember::AssociatedType { .. } => None,
            })
            .fold(HashMap::<_, usize>::new(), |mut counts, name| {
                *counts.entry(name).or_default() += 1;
                counts
            });
        let associated_names = definition
            .members
            .iter()
            .filter_map(|member| match member {
                TraitMember::AssociatedType { name, .. } => Some(name.clone()),
                TraitMember::Function(_) => None,
            })
            .collect::<HashSet<_>>();
        let mut member_names = HashSet::new();
        let mut associated_types = Vec::new();
        let mut associated_type_kinds = HashMap::new();
        let mut associated_type_parameter_groups = HashMap::new();
        let mut associated_type_parameters = HashMap::new();
        let mut associated_parameter_schemas = HashSet::new();
        let mut associated_parameter_counts = HashMap::new();
        let mut methods = HashMap::new();
        let mut method_overloads = HashMap::<String, Vec<String>>::new();
        let mut overload_shapes = HashMap::<String, HashSet<ParameterLabelShape>>::new();
        let mut method_order = Vec::new();
        for member in definition.members {
            match member {
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    kind: associated_kind,
                    default,
                } => {
                    if !member_names.insert(name.clone()) {
                        self.error(format!(
                            "duplicate trait member `{}.{name}`",
                            definition.name
                        ));
                        valid = false;
                        continue;
                    }
                    if name == "self" || compile_parameter_names.contains(&name) {
                        self.error(format!(
                            "associated type `{}.{name}` conflicts with a trait type parameter",
                            definition.name
                        ));
                        valid = false;
                    }
                    let kind = if associated_kind == AssociatedKind::Parameters {
                        Sort::Parameters
                    } else if compile_groups.is_empty() {
                        Sort::Type
                    } else {
                        for parameter in compile_groups.iter().flatten() {
                            if matches!(
                                parameter.kind,
                                Sort::Effect
                                    | Sort::Effects
                                    | Sort::Parameters
                                    | Sort::ParameterPack
                                    | Sort::ParameterModifier
                                    | Sort::TypeConstructor { .. }
                                    | Sort::EffectConstructor { .. }
                            ) {
                                self.error(format!(
                                    "generic associated type `{}.{name}` parameter `{}` has unsupported sort",
                                    definition.name, parameter.name
                                ));
                                valid = false;
                            }
                        }
                        associated_type_parameters.insert(
                            name.clone(),
                            compile_groups.iter().flatten().cloned().collect(),
                        );
                        associated_type_parameter_groups
                            .insert(name.clone(), compile_groups.clone());
                        Sort::TypeConstructor {
                            parameter_groups: compile_groups
                                .iter()
                                .map(|group| {
                                    group
                                        .iter()
                                        .map(|parameter| parameter.kind.clone())
                                        .collect()
                                })
                                .collect(),
                        }
                    };
                    if associated_kind == AssociatedKind::Parameters {
                        associated_parameter_schemas.insert(name.clone());
                        associated_parameter_counts
                            .insert(name.clone(), compile_groups.iter().flatten().count());
                    }
                    if default.is_some() {
                        self.error(format!(
                            "default associated type `{}.{name}` is not supported",
                            definition.name
                        ));
                        valid = false;
                    }
                    associated_type_kinds.insert(name.clone(), kind);
                    associated_types.push(name);
                }
                TraitMember::Function(function) => {
                    let name = function.name.clone();
                    if associated_names.contains(&name) {
                        self.error(format!(
                            "duplicate trait member `{}.{name}`",
                            definition.name
                        ));
                        valid = false;
                        continue;
                    }
                    let overloaded = function_counts[&name] > 1;
                    let method_id = if overloaded {
                        let shape = function_parameter_labels(&function);
                        if !overload_shapes
                            .entry(name.clone())
                            .or_default()
                            .insert(shape.clone())
                        {
                            self.error(format!(
                                "duplicate trait method overload `{}.{name}` with parameter labels {}",
                                definition.name,
                                display_parameter_label_shape(&shape)
                            ));
                            valid = false;
                            continue;
                        }
                        let id = overloaded_function_name(&name, &shape);
                        method_overloads
                            .entry(name.clone())
                            .or_default()
                            .push(id.clone());
                        id
                    } else {
                        name.clone()
                    };
                    if function.return_type.is_none() {
                        self.error(format!(
                            "trait method `{}.{name}` requires an explicit return type",
                            definition.name
                        ));
                        valid = false;
                    }
                    method_order.push(method_id.clone());
                    methods.insert(method_id, function);
                }
            }
        }
        self.collection.traits.insert(
            definition.name,
            TraitSchema {
                self_parameter: definition.self_parameter,
                compile_parameters,
                where_predicates: definition.where_predicates,
                associated_types,
                associated_type_kinds,
                associated_type_parameter_groups,
                associated_type_parameters,
                associated_parameter_schemas,
                associated_parameter_counts,
                methods,
                method_overloads,
                method_order,
                access: AccessBoundary { visibility, origin },
                valid,
            },
        );
    }

    pub(super) fn validate_trait_schemas(&mut self) {
        let mut trait_names = self.collection.traits.keys().cloned().collect::<Vec<_>>();
        trait_names.sort();
        for trait_name in trait_names {
            let schema = self.collection.traits[&trait_name].clone();
            let mut compile_parameter_sorts = schema
                .compile_parameters
                .iter()
                .map(|parameter| (parameter.name.clone(), parameter.kind.clone()))
                .collect::<HashMap<_, _>>();
            compile_parameter_sorts.insert("self".to_owned(), schema.self_parameter.kind.clone());
            compile_parameter_sorts.extend(
                schema
                    .associated_types
                    .iter()
                    .map(|name| (name.clone(), schema.associated_type_kinds[name].clone())),
            );
            let mut valid = schema.valid;
            valid &= self.validate_where_predicate_shapes(
                &format!("trait `{trait_name}`"),
                &schema.where_predicates,
                &compile_parameter_sorts,
            );
            for method_name in &schema.method_order {
                let method = &schema.methods[method_name];
                let mut method_compile_parameter_sorts = compile_parameter_sorts.clone();
                method_compile_parameter_sorts.extend(
                    method
                        .compile_groups
                        .iter()
                        .flatten()
                        .map(|parameter| (parameter.name.clone(), parameter.kind.clone())),
                );
                for parameter in method.groups.iter().flatten() {
                    if let Type::Named(wrapper, schemas) = &parameter.ty {
                        if matches!(
                            wrapper.as_str(),
                            "$parameters$expand" | "$parameter$groups$expand"
                        ) {
                            let schema_name = schemas.first().and_then(|schema| match schema {
                                Type::Named(name, _) => Some(name),
                                _ => None,
                            });
                            let compile_schema = schema_name.is_some_and(|name| {
                                schema.compile_parameters.iter().any(|parameter| {
                                    parameter.name == *name && parameter.kind == Sort::Parameters
                                })
                            });
                            if !schema_name.is_some_and(|name| {
                                schema.associated_parameter_schemas.contains(name)
                            }) && !compile_schema
                            {
                                self.error(format!(
                                    "trait method `{trait_name}.{method_name}` expands a type that is not declared as an associated `parameters` schema"
                                ));
                                valid = false;
                            } else if let Some(Type::Named(name, arguments)) = schemas.first() {
                                let expected = schema
                                    .associated_parameter_counts
                                    .get(name)
                                    .cloned()
                                    .unwrap_or(0);
                                if arguments.len() != expected {
                                    self.error(format!(
                                        "associated parameter schema `{trait_name}.{name}` expects {expected} type arguments, found {}",
                                        arguments.len()
                                    ));
                                    valid = false;
                                }
                            }
                        }
                    }
                    valid &= self.validate_trait_source_type(
                        &trait_name,
                        method_name,
                        &parameter.ty,
                        &method_compile_parameter_sorts,
                    );
                    if parameter.mode == PassMode::Copy
                        && !self.trait_source_type_is_definitely_copy(&parameter.ty)
                    {
                        self.error(format!(
                            "trait method `{}.{method_name}` parameter `{}` requires `copyable`, but its type is not provably copyable without a trait bound",
                            trait_name,
                            parameter.name
                        ));
                        valid = false;
                    }
                }
                if let Some(result) = &method.return_type {
                    valid &= self.validate_trait_source_type(
                        &trait_name,
                        method_name,
                        result,
                        &method_compile_parameter_sorts,
                    );
                }
                valid &= self.validate_trait_source_effects(
                    &trait_name,
                    method_name,
                    &method.effects,
                    &method_compile_parameter_sorts,
                );
            }
            self.collection
                .traits
                .get_mut(&trait_name)
                .expect("trait schema exists")
                .valid = valid;
            if valid {
                self.register_trait_default_validation_templates(&trait_name, &schema);
            }
        }
    }

    pub(super) fn register_trait_default_validation_templates(
        &mut self,
        trait_name: &str,
        schema: &TraitSchema,
    ) {
        let self_parameter = "$default$Self".to_owned();
        let mut compile_parameters = schema.compile_parameters.clone();
        compile_parameters.push(CompileParam {
            name: self_parameter.clone(),
            kind: schema.self_parameter.kind.clone(),
            default: None,
        });
        compile_parameters.extend(schema.associated_types.iter().map(|name| CompileParam {
            name: name.clone(),
            kind: schema.associated_type_kinds[name].clone(),
            default: None,
        }));
        let trait_arguments = schema
            .compile_parameters
            .iter()
            .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
            .collect();
        let associated_types = schema
            .associated_types
            .iter()
            .filter(|name| schema.associated_type_kinds[*name] == Sort::Type)
            .map(|name| crate::ast::AssociatedTypeBinding {
                name: name.clone(),
                compile_groups: Vec::new(),
                ty: Type::Named(name.clone(), Vec::new()),
            })
            .collect();
        let predicate = crate::ast::WherePredicate {
            subject: Type::Named(self_parameter.clone(), Vec::new()),
            trait_ref: Type::Named(trait_name.to_owned(), trait_arguments),
            associated_types,
        };
        let mut self_substitution = HashMap::new();
        self_substitution.insert("self".to_owned(), Type::Named(self_parameter, Vec::new()));
        for method_name in &schema.method_order {
            let method = &schema.methods[method_name];
            if method.body.is_none() {
                continue;
            }
            let canonical = format!(
                "$trait$default$validation${trait_name}${method_name}${}",
                self.collection.function_template_order.len()
            );
            let mut template = method.clone();
            template.name = canonical.clone();
            let method_compile_groups = std::mem::take(&mut template.compile_groups);
            template.compile_groups = vec![compile_parameters.clone()];
            template.compile_groups.extend(method_compile_groups);
            let method_predicates = std::mem::take(&mut template.where_predicates);
            template.where_predicates = vec![predicate.clone()];
            template.where_predicates.extend(method_predicates);
            substitute_function_types(&mut template, &self_substitution);
            if let Some(body) = &mut template.body {
                rewrite_abstract_self_qualified_methods(body);
            }
            self.collection
                .function_template_order
                .push(canonical.clone());
            self.collection
                .function_templates
                .insert(canonical.clone(), template);
            self.collection
                .function_template_origins
                .insert(canonical.clone(), schema.access.origin.clone());
            self.collection
                .function_accesses
                .insert(canonical, schema.access.clone());
        }
    }

    pub(super) fn trait_source_type_is_definitely_copy(&self, source: &Type) -> bool {
        self.probe_source_ty(source)
            .is_some_and(|ty| self.is_copy_type(&ty))
    }

    pub(super) fn validate_trait_source_type(
        &mut self,
        trait_name: &str,
        member_name: &str,
        source: &Type,
        compile_parameters: &HashMap<String, Sort>,
    ) -> bool {
        match source {
            Type::Tuple(fields) => {
                let mut valid = true;
                for field in fields {
                    valid &= self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        field,
                        compile_parameters,
                    );
                }
                valid
            }
            Type::Named(wrapper, schemas)
                if matches!(
                    wrapper.as_str(),
                    "$parameters$expand" | "$parameter$groups$expand"
                ) && schemas.len() == 1 =>
            {
                let Type::Named(name, arguments) = &schemas[0] else {
                    self.error(format!(
                        "parameter expansion in trait member `{trait_name}.{member_name}` requires an associated parameter schema"
                    ));
                    return false;
                };
                let Some(kind) = compile_parameters.get(name) else {
                    self.error(format!(
                        "`{name}` in trait member `{trait_name}.{member_name}` is not an associated parameter schema"
                    ));
                    return false;
                };
                let parameter_count = match kind {
                    Sort::Parameters => arguments.len(),
                    Sort::TypeConstructor { parameter_groups } => {
                        parameter_groups.iter().map(Vec::len).sum()
                    }
                    _ => {
                        self.error(format!(
                            "`{name}` in trait member `{trait_name}.{member_name}` is not an associated parameter schema"
                        ));
                        return false;
                    }
                };
                if arguments.len() != parameter_count {
                    self.error(format!(
                        "parameter schema `{name}` in `{trait_name}.{member_name}` expects {parameter_count} type arguments, found {}",
                        arguments.len()
                    ));
                    return false;
                }
                arguments.iter().all(|argument| {
                    self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        argument,
                        compile_parameters,
                    )
                })
            }
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::I128
            | Type::ISize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::USize
            | Type::Bool
            | Type::Unit => true,
            Type::CompileUSize(value) => {
                self.error(format!(
                    "compile-time `usize` value `{value}` cannot be used as a runtime type in trait member `{trait_name}.{member_name}`"
                ));
                false
            }
            Type::Borrow { pointee, .. } => self.validate_trait_source_type(
                trait_name,
                member_name,
                pointee,
                compile_parameters,
            ),
            Type::Array(element, length) => {
                let mut valid = true;
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} in trait member `{trait_name}.{member_name}` exceeds the first-version limit"
                    ));
                    valid = false;
                }
                valid &= self.validate_trait_source_type(
                    trait_name,
                    member_name,
                    element,
                    compile_parameters,
                );
                valid
            }
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                let mut valid = self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm);
                if !valid {
                    self.error(format!(
                        "array syntax in trait member `{trait_name}.{member_name}` resolved to non-standard constructor `{constructor}`"
                    ));
                }
                match length {
                    crate::ast::USizeConst::Literal(length) if *length > i32::MAX as u64 => {
                        self.error(format!(
                            "array length {length} in trait member `{trait_name}.{member_name}` exceeds the first-version limit"
                        ));
                        valid = false;
                    }
                    crate::ast::USizeConst::Parameter(name)
                        if compile_parameters.get(name) != Some(&Sort::USize) =>
                    {
                        self.error(format!(
                            "array length `{name}` in trait member `{trait_name}.{member_name}` is not a declared `usize` parameter"
                        ));
                        valid = false;
                    }
                    _ => {}
                }
                valid
                    & self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        element,
                        compile_parameters,
                    )
            }
            Type::Function {
                groups,
                effects,
                result,
            } => {
                groups.iter().flatten().all(|ty| {
                    self.validate_trait_source_type(trait_name, member_name, ty, compile_parameters)
                }) && self.validate_trait_source_type(
                    trait_name,
                    member_name,
                    result,
                    compile_parameters,
                ) && self.validate_trait_source_effects(
                    trait_name,
                    member_name,
                    effects,
                    compile_parameters,
                )
            }
            Type::Named(name, arguments) if compile_parameters.contains_key(name) => {
                let kind = compile_parameters
                    .get(name)
                    .cloned()
                    .expect("checked compile parameter exists");
                match kind {
                    Sort::Universe(_) => {
                        self.error(format!(
                            "universe parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    Sort::Type => {
                        if arguments.is_empty() {
                            true
                        } else {
                            self.error(format!(
                                "trait type parameter `{name}` in `{trait_name}.{member_name}` does not accept type arguments"
                            ));
                            false
                        }
                    }
                    Sort::USize => {
                        self.error(format!(
                            "`usize` parameter `{name}` in `{trait_name}.{member_name}` can only be used as a compile-time value"
                        ));
                        false
                    }
                    Sort::TypeConstructor { parameter_groups } => {
                        let parameter_count = parameter_groups.iter().map(Vec::len).sum::<usize>();
                        let mut valid = true;
                        if arguments.len() != parameter_count {
                            self.error(format!(
                                "type constructor parameter `{name}` in `{trait_name}.{member_name}` expects {parameter_count} type arguments, found {}",
                                arguments.len()
                            ));
                            valid = false;
                        }
                        let associated_parameters = self
                            .collection
                            .traits
                            .get(trait_name)
                            .and_then(|schema| schema.associated_type_parameters.get(name))
                            .cloned();
                        if let Some(parameters) = associated_parameters {
                            for (argument, parameter) in arguments.iter().zip(parameters) {
                                valid &= self.validate_associated_constructor_argument(
                                    trait_name,
                                    member_name,
                                    name,
                                    argument,
                                    &parameter,
                                    compile_parameters,
                                );
                            }
                        } else {
                            for argument in arguments {
                                valid &= self.validate_trait_source_type(
                                    trait_name,
                                    member_name,
                                    argument,
                                    compile_parameters,
                                );
                            }
                        }
                        valid
                    }
                    Sort::EffectConstructor { .. } => {
                        self.error(format!(
                            "effect constructor parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    Sort::Effect | Sort::Effects => {
                        self.error(format!(
                            "effect parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    Sort::Parameters => {
                        self.error(format!(
                            "parameter schema `{name}` in `{trait_name}.{member_name}` can only be used through a complete `...` parameter-group expansion"
                        ));
                        false
                    }
                    Sort::ParameterPack => {
                        self.error(format!(
                            "parameter-group pack `{name}` in `{trait_name}.{member_name}` can only be used through a complete repeated-group expansion"
                        ));
                        false
                    }
                    Sort::ParameterModifier => {
                        self.error(format!(
                            "parameter modifier `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    Sort::Region => {
                        self.error(format!(
                            "region parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    Sort::Named(compile_type) => {
                        self.error(format!(
                            "`{compile_type}` value parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.collection.struct_defs.contains_key(name)
                    || self.collection.enum_defs.contains_key(name)
                {
                    true
                } else if self.collection.struct_templates.contains_key(name)
                    || self.collection.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` in trait member `{trait_name}.{member_name}` requires type arguments"
                    ));
                    false
                } else {
                    self.error(format!(
                        "unknown type `{name}` in trait member `{trait_name}.{member_name}`"
                    ));
                    false
                }
            }
            Type::Named(name, arguments) => {
                let expected = self
                    .collection
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.collection
                            .enum_templates
                            .get(name)
                            .map(|template| template.compile_groups.iter().flatten().count())
                    });
                let Some(expected) = expected else {
                    if self.collection.struct_defs.contains_key(name)
                        || self.collection.enum_defs.contains_key(name)
                    {
                        self.error(format!(
                            "non-generic type `{name}` in trait member `{trait_name}.{member_name}` does not accept type arguments"
                        ));
                    } else {
                        self.error(format!(
                            "unknown generic type `{name}` in trait member `{trait_name}.{member_name}`"
                        ));
                    }
                    return false;
                };
                let mut valid = true;
                if arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}` in trait member `{trait_name}.{member_name}`: expected {expected}, found {}",
                        arguments.len()
                    ));
                    valid = false;
                }
                for argument in arguments {
                    valid &= self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        argument,
                        compile_parameters,
                    );
                }
                valid
            }
            Type::NamedArgs(name, _) => {
                self.error(format!(
                    "internal error: labeled type arguments for `{name}` were not normalized"
                ));
                false
            }
        }
    }

    pub(super) fn validate_associated_constructor_argument(
        &mut self,
        trait_name: &str,
        member_name: &str,
        associated: &str,
        argument: &Type,
        parameter: &CompileParam,
        compile_parameters: &HashMap<String, Sort>,
    ) -> bool {
        let parameter_reference_has_kind = |expected: &Sort| {
            matches!(argument, Type::Named(name, values)
                if values.is_empty() && compile_parameters.get(name) == Some(expected))
        };
        let valid = match &parameter.kind {
            Sort::Type => {
                return self.validate_trait_source_type(
                    trait_name,
                    member_name,
                    argument,
                    compile_parameters,
                );
            }
            Sort::Region => {
                parameter_reference_has_kind(&Sort::Region)
                    || matches!(argument, Type::Named(_, values) if values.is_empty())
            }
            Sort::USize => {
                matches!(argument, Type::CompileUSize(_))
                    || parameter_reference_has_kind(&Sort::USize)
            }
            Sort::Named(kind) => {
                parameter_reference_has_kind(&Sort::Named(kind.clone()))
                    || matches!(argument, Type::Named(name, values)
                    if values.is_empty()
                        && self.collection.closed_type_values.get(kind).is_some_and(|members| {
                            members.contains(name)
                                || closed_value_from_marker(name)
                                    .is_some_and(|(owner, value)| owner == kind && members.contains(&value.to_owned()))
                        }))
            }
            _ => false,
        };
        if !valid {
            self.error(format!(
                "generic associated type `{trait_name}.{associated}` argument for parameter `{}` in `{trait_name}.{member_name}` must have sort `{}`",
                parameter.name,
                compile_parameter_sort_label(&parameter.kind)
            ));
        }
        valid
    }

    pub(super) fn validate_trait_source_effects(
        &mut self,
        trait_name: &str,
        member_name: &str,
        effects: &FunctionEffects,
        compile_parameters: &HashMap<String, Sort>,
    ) -> bool {
        let mut valid = true;
        if let Some(error) = &effects.failure {
            valid &=
                self.validate_trait_source_type(trait_name, member_name, error, compile_parameters);
        }
        for parameter in &effects.parameters {
            match compile_parameters.get(parameter).cloned() {
                Some(Sort::Effect | Sort::Effects) => {}
                Some(kind) => {
                    self.error(format!(
                        "effect row `{parameter}` in trait member `{trait_name}.{member_name}` has incompatible compile-time sort {}",
                        describe_compile_sort(kind)
                    ));
                    valid = false;
                }
                None => {
                    self.error(format!(
                        "unknown effect row `{parameter}` in trait member `{trait_name}.{member_name}`"
                    ));
                    valid = false;
                }
            }
        }
        for effect in &effects.custom {
            valid &= self.validate_trait_source_effect(
                trait_name,
                member_name,
                effect,
                compile_parameters,
            );
        }
        valid
    }

    pub(super) fn validate_trait_source_effect(
        &mut self,
        trait_name: &str,
        member_name: &str,
        effect: &Type,
        compile_parameters: &HashMap<String, Sort>,
    ) -> bool {
        let (name, arguments) = match effect {
            Type::Named(name, arguments) => (name, arguments.as_slice()),
            Type::NamedArgs(name, arguments) => {
                let mut valid = true;
                for argument in arguments {
                    valid &= self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        &argument.ty,
                        compile_parameters,
                    );
                }
                if let Some(kind) = compile_parameters.get(name).cloned() {
                    return match kind {
                        Sort::EffectConstructor { parameter_groups } => {
                            let parameter_count =
                                parameter_groups.iter().map(Vec::len).sum::<usize>();
                            if arguments.len() == parameter_count {
                                valid
                            } else {
                                self.error(format!(
                                    "effect constructor parameter `{name}` in trait member `{trait_name}.{member_name}` expects {parameter_count} type arguments, found {}",
                                    arguments.len()
                                ));
                                false
                            }
                        }
                        Sort::Effect => {
                            self.error(format!(
                                "effect identity parameter `{name}` in trait member `{trait_name}.{member_name}` is not an effect constructor"
                            ));
                            false
                        }
                        Sort::Effects => {
                            self.error(format!(
                                "effects row parameter `{name}` in trait member `{trait_name}.{member_name}` is not an effect constructor"
                            ));
                            false
                        }
                        _ => {
                            self.error(format!(
                                "compile-time parameter `{name}` in trait member `{trait_name}.{member_name}` has sort {}, not `effect`",
                                describe_compile_sort(kind)
                            ));
                            false
                        }
                    };
                }
                return valid;
            }
            _ => return true,
        };

        if let Some(kind) = compile_parameters.get(name).cloned() {
            match kind {
                Sort::EffectConstructor { parameter_groups } => {
                    let parameter_count = parameter_groups.iter().map(Vec::len).sum::<usize>();
                    let mut valid = true;
                    if arguments.len() != parameter_count {
                        self.error(format!(
                            "effect constructor parameter `{name}` in trait member `{trait_name}.{member_name}` expects {parameter_count} type arguments, found {}",
                            arguments.len()
                        ));
                        valid = false;
                    }
                    for argument in arguments {
                        valid &= self.validate_trait_source_type(
                            trait_name,
                            member_name,
                            argument,
                            compile_parameters,
                        );
                    }
                    valid
                }
                Sort::Effect => {
                    self.error(format!(
                        "effect identity parameter `{name}` in trait member `{trait_name}.{member_name}` is not an effect constructor"
                    ));
                    false
                }
                Sort::Effects => {
                    self.error(format!(
                        "effects row parameter `{name}` in trait member `{trait_name}.{member_name}` is not an effect constructor"
                    ));
                    false
                }
                _ => {
                    self.error(format!(
                        "compile-time parameter `{name}` in trait member `{trait_name}.{member_name}` has sort {}, not `effect`",
                        describe_compile_sort(kind)
                    ));
                    false
                }
            }
        } else {
            let mut valid = true;
            for argument in arguments {
                valid &= self.validate_trait_source_type(
                    trait_name,
                    member_name,
                    argument,
                    compile_parameters,
                );
            }
            valid
        }
    }

    pub(super) fn source_type_is_concrete(&self, source: &Type) -> bool {
        match source {
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::I128
            | Type::ISize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::USize
            | Type::Bool
            | Type::Unit => true,
            Type::CompileUSize(_) => false,
            Type::Borrow { pointee, .. } => self.source_type_is_concrete(pointee),
            Type::Tuple(fields) => fields
                .iter()
                .all(|field| self.source_type_is_concrete(field)),
            Type::Array(element, _) => self.source_type_is_concrete(element),
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm)
                    && matches!(length, crate::ast::USizeConst::Literal(_))
                    && self.source_type_is_concrete(element)
            }
            Type::Function {
                groups,
                effects,
                result,
            } => {
                effects.parameters.is_empty()
                    && groups
                        .iter()
                        .flatten()
                        .all(|ty| self.source_type_is_concrete(ty))
                    && self.source_type_is_concrete(result)
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Named(name, arguments) if arguments.is_empty() => {
                self.is_lang_item_name(name, LangItemKind::StrTypeForm)
                    || self.collection.struct_defs.contains_key(name)
                    || self.collection.enum_defs.contains_key(name)
            }
            Type::Named(name, arguments) => {
                let expected = self
                    .collection
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.collection
                            .enum_templates
                            .get(name)
                            .map(|template| template.compile_groups.iter().flatten().count())
                    });
                expected == Some(arguments.len())
                    && arguments
                        .iter()
                        .all(|argument| self.source_type_is_concrete(argument))
            }
            Type::NamedArgs(_, _) => false,
        }
    }

    pub(super) fn source_type_is_abstract_or_concrete(&self, source: &Type) -> bool {
        match source {
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::I128
            | Type::ISize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::USize
            | Type::Bool
            | Type::Unit => true,
            Type::CompileUSize(_) => false,
            Type::Borrow { pointee, .. } => self.source_type_is_abstract_or_concrete(pointee),
            Type::Tuple(fields) => fields
                .iter()
                .all(|field| self.source_type_is_abstract_or_concrete(field)),
            Type::Array(element, _) => self.source_type_is_abstract_or_concrete(element),
            Type::ArrayApplication {
                constructor,
                element,
                ..
            } => {
                self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm)
                    && self.source_type_is_abstract_or_concrete(element)
            }
            Type::Function { groups, result, .. } => {
                groups
                    .iter()
                    .flatten()
                    .all(|ty| self.source_type_is_abstract_or_concrete(ty))
                    && self.source_type_is_abstract_or_concrete(result)
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                self.collection.abstract_type_parameters.contains_key(name)
                    || self.is_lang_item_name(name, LangItemKind::StrTypeForm)
                    || self.collection.struct_defs.contains_key(name)
                    || self.collection.enum_defs.contains_key(name)
            }
            Type::Named(name, arguments) => {
                (self.collection.struct_templates.contains_key(name)
                    || self.collection.enum_templates.contains_key(name))
                    && arguments
                        .iter()
                        .all(|argument| self.source_type_is_abstract_or_concrete(argument))
            }
            Type::NamedArgs(_, _) => false,
        }
    }

    pub(super) fn normalize_concrete_trait_argument(
        &self,
        parameter: &CompileParam,
        source: &Type,
    ) -> Option<Type> {
        if !parameter.kind.is_effect_classifier() {
            return self.source_type_is_concrete(source).then(|| source.clone());
        }
        if matches!(source, Type::Unit)
            || matches!(source, Type::Named(name, arguments) if (name == "()" || name == "pure") && arguments.is_empty())
        {
            return (parameter.kind == Sort::Effects).then(|| effect_row_source(false, None, &[]));
        }
        if effect_row_from_source(source).is_some() {
            return if parameter.kind == Sort::Effect {
                let (unsafety, failure, custom) = effect_row_from_source(source)?;
                (usize::from(unsafety) + usize::from(failure.is_some()) + custom.len() == 1)
                    .then(|| source.clone())
            } else {
                Some(source.clone())
            };
        }
        if self.is_standard_unsafety_source(source) {
            return Some(effect_row_source(true, None, &[]));
        }
        let Type::Named(name, arguments) = source else {
            return None;
        };
        self.collection.effects.contains(name).then(|| {
            effect_row_source(
                false,
                None,
                std::slice::from_ref(&source_effect_identity(&Type::Named(
                    name.clone(),
                    arguments.clone(),
                ))),
            )
        })
    }

    pub(super) fn resolve_trait_impl_target(&mut self, source: &Type) -> Option<Ty> {
        if matches!(
            source,
            Type::I8
                | Type::I16
                | Type::I32
                | Type::I64
                | Type::I128
                | Type::ISize
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::U128
                | Type::USize
                | Type::Bool
        ) {
            return Some(self.lower_source_type(source));
        }
        if matches!(source, Type::Array(_, _) | Type::ArrayApplication { .. }) {
            let target = self.lower_source_type(source);
            return matches!(target, Ty::Array(_, _)).then_some(target);
        }
        let Type::Named(name, arguments) = source else {
            self.error("trait implementation target must be a nominal type");
            return None;
        };
        if arguments.is_empty() && self.is_lang_item_name(name, LangItemKind::StrTypeForm) {
            return Some(Ty::Str);
        }
        if (self.collection.struct_templates.contains_key(name)
            || self.collection.enum_templates.contains_key(name))
            && (arguments.is_empty() || !self.source_type_is_concrete(source))
        {
            self.error(format!(
                "generic trait implementation for `{name}` is not supported; use a concrete type such as `{name}(i32)`"
            ));
            return None;
        }
        if arguments.is_empty()
            && !self.collection.struct_defs.contains_key(name)
            && !self.collection.enum_defs.contains_key(name)
        {
            self.error(format!("unknown extension target `{name}`"));
            return None;
        }
        let target = self.lower_source_type(source);
        match target {
            Ty::Struct(_) | Ty::Enum(_) | Ty::Str | Ty::Slice(_) | Ty::Array(_, _) => Some(target),
            _ if primitive_scalar_type(&target) => Some(target),
            Ty::Error => None,
            _ => {
                self.error("trait implementation target must be a nominal type");
                None
            }
        }
    }

    pub(super) fn type_constructor_impl_target(
        &self,
        source: &Type,
    ) -> Option<TypeConstructorImplTarget> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        if let Some(template) = self.collection.struct_templates.get(name) {
            return Some(TypeConstructorImplTarget {
                name: name.clone(),
                kind: NominalKind::Struct,
                parameter_count: template.compile_groups.iter().flatten().count(),
                parameter_groups: template
                    .compile_groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| parameter.kind.clone())
                            .collect()
                    })
                    .collect(),
            });
        }
        if let Some(template) = self.collection.enum_templates.get(name) {
            return Some(TypeConstructorImplTarget {
                name: name.clone(),
                kind: NominalKind::Enum,
                parameter_count: template.compile_groups.iter().flatten().count(),
                parameter_groups: template
                    .compile_groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| parameter.kind.clone())
                            .collect()
                    })
                    .collect(),
            });
        }
        None
    }

    pub(super) fn partial_nominal_constructor_trait_target(
        &mut self,
        source: &Type,
        declared_parameters: &[CompileParam],
    ) -> Option<GenericConstructorTraitExtensionTarget> {
        let Type::Named(target_name, supplied_arguments) = source else {
            return None;
        };
        let base =
            self.type_constructor_impl_target(&Type::Named(target_name.clone(), Vec::new()))?;
        if supplied_arguments.is_empty() || supplied_arguments.len() >= base.parameter_count {
            return None;
        }
        let declared = declared_parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        let mut determined = HashSet::new();
        for argument in supplied_arguments {
            let Type::Named(name, arguments) = argument else {
                self.error(
                    "generic constructor trait extend target arguments must be bare declared type parameters",
                );
                return None;
            };
            if !arguments.is_empty() || !declared.contains(name) || !determined.insert(name.clone())
            {
                self.error(
                    "generic constructor trait extend target arguments must use every declared type parameter exactly once",
                );
                return None;
            }
        }
        if determined.len() != declared_parameters.len() {
            self.error(
                "every generic constructor trait extend parameter must be determined by the target constructor",
            );
            return None;
        }
        Some(GenericConstructorTraitExtensionTarget {
            target: TypeConstructorImplTarget {
                name: target_name.clone(),
                kind: base.kind,
                parameter_count: base.parameter_count - supplied_arguments.len(),
                parameter_groups: remaining_sort_groups(
                    &base.parameter_groups,
                    supplied_arguments.len(),
                ),
            },
            self_constructor: source.clone(),
        })
    }

    pub(super) fn partial_alias_constructor_trait_target(
        &mut self,
        source: &Type,
        declared_parameters: &[CompileParam],
    ) -> Option<GenericConstructorTraitExtensionTarget> {
        let Type::Named(alias_name, supplied_arguments) = source else {
            return None;
        };
        let alias = self.collection.type_aliases.get(alias_name).cloned()?;
        let alias_parameters = alias
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if supplied_arguments.is_empty() || supplied_arguments.len() >= alias_parameters.len() {
            return None;
        }
        if alias_parameters
            .iter()
            .any(|parameter| parameter.kind != Sort::Type)
        {
            self.error(format!(
                "constructor trait implementation target alias `{alias_name}` must contain only type parameters"
            ));
            return None;
        }

        let declared = declared_parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        let mut determined = HashSet::new();
        for argument in supplied_arguments {
            let Type::Named(name, arguments) = argument else {
                self.error(
                    "generic constructor trait extend target arguments must be bare declared type parameters",
                );
                return None;
            };
            if !arguments.is_empty() || !declared.contains(name) || !determined.insert(name.clone())
            {
                self.error(
                    "generic constructor trait extend target arguments must use every declared type parameter exactly once",
                );
                return None;
            }
        }
        if determined.len() != declared_parameters.len() {
            self.error(
                "every generic constructor trait extend parameter must be determined by the target constructor",
            );
            return None;
        }

        let substitutions = alias_parameters
            .iter()
            .zip(supplied_arguments.iter())
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        let remaining_parameters = alias_parameters
            .iter()
            .skip(supplied_arguments.len())
            .enumerate()
            .map(|(index, parameter)| (parameter.name.clone(), index))
            .collect::<HashMap<_, _>>();
        let mut target = alias.target.clone();
        substitute_type_parameters(&mut target, &substitutions);
        let Type::Named(target_name, target_arguments) = target else {
            self.error(format!(
                "constructor trait implementation target alias `{alias_name}` must expand to a nominal type constructor"
            ));
            return None;
        };
        let Some(base) =
            self.type_constructor_impl_target(&Type::Named(target_name.clone(), Vec::new()))
        else {
            self.error(format!(
                "constructor trait implementation target alias `{alias_name}` must expand to a generic nominal type constructor"
            ));
            return None;
        };
        let expected_arguments = match base.kind {
            NominalKind::Struct => self.collection.struct_templates[&target_name]
                .compile_groups
                .iter()
                .flatten()
                .count(),
            NominalKind::Enum => self.collection.enum_templates[&target_name]
                .compile_groups
                .iter()
                .flatten()
                .count(),
        };
        if target_arguments.len() != expected_arguments {
            self.error(format!(
                "constructor trait implementation target alias `{alias_name}` expands to `{target_name}` with {} argument{}, expected {expected_arguments}",
                target_arguments.len(),
                if target_arguments.len() == 1 { "" } else { "s" }
            ));
            return None;
        }
        let mut open_counts = vec![0_usize; remaining_parameters.len()];
        for argument in &target_arguments {
            if let Type::Named(name, arguments) = argument {
                if arguments.is_empty() {
                    if let Some(index) = remaining_parameters.get(name) {
                        open_counts[*index] += 1;
                    }
                }
            }
        }
        if open_counts.iter().any(|count| *count != 1) {
            self.error(format!(
                "constructor trait implementation target alias `{alias_name}` must use each remaining constructor parameter exactly once"
            ));
            return None;
        }
        Some(GenericConstructorTraitExtensionTarget {
            target: TypeConstructorImplTarget {
                name: target_name,
                kind: base.kind,
                parameter_count: remaining_parameters.len(),
                parameter_groups: remaining_sort_groups(
                    &alias
                        .compile_groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.kind.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>(),
                    supplied_arguments.len(),
                ),
            },
            self_constructor: source.clone(),
        })
    }

    pub(super) fn expand_function_aliases_after_substitution(
        &mut self,
        function: &mut Function,
        context: &str,
    ) -> bool {
        let aliases = self.collection.type_aliases.clone();
        let mut diagnostics = Vec::new();
        expand_function_aliases(function, &aliases, &mut diagnostics);
        if diagnostics.is_empty() {
            return true;
        }
        for diagnostic in diagnostics {
            self.error(format!("{context}: {diagnostic}"));
        }
        false
    }

    pub(super) fn validate_associated_type_constructor(
        &mut self,
        trait_name: &str,
        associated: &str,
        source: &Type,
        expected_parameters: &[CompileParam],
    ) -> bool {
        let Type::Named(name, arguments) = source else {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` must name a generic type constructor"
            ));
            return false;
        };

        let actual_parameters = self
            .remaining_nominal_constructor_parameters(source)
            .or_else(|| self.remaining_type_alias_constructor_parameters(source))
            .or_else(|| {
                arguments
                    .is_empty()
                    .then(|| {
                        self.type_constructor_impl_target(source).map(|target| {
                            (0..target.parameter_count)
                                .map(|index| CompileParam {
                                    name: format!("T{index}"),
                                    kind: Sort::Type,
                                    default: None,
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .flatten()
            });
        let Some(actual_parameters) = actual_parameters else {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` must name a generic type constructor"
            ));
            return false;
        };
        if actual_parameters.len() != expected_parameters.len() {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` expects {} compile-time parameter{}, but `{name}` has {}",
                expected_parameters.len(),
                if expected_parameters.len() == 1 { "" } else { "s" },
                actual_parameters.len()
            ));
            return false;
        }
        for (index, (expected, actual)) in expected_parameters
            .iter()
            .zip(&actual_parameters)
            .enumerate()
        {
            if expected.kind != actual.kind {
                self.error(format!(
                    "associated type constructor `{trait_name}.{associated}` parameter {} expects sort `{}`, but `{name}` uses sort `{}`",
                    index + 1,
                    compile_parameter_sort_label(&expected.kind),
                    compile_parameter_sort_label(&actual.kind)
                ));
                return false;
            }
        }
        true
    }

    pub(super) fn remaining_type_alias_constructor_parameters(
        &self,
        source: &Type,
    ) -> Option<Vec<CompileParam>> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        let alias = self.collection.type_aliases.get(name)?;
        let parameters = alias.compile_groups.iter().flatten().collect::<Vec<_>>();
        if arguments.len() >= parameters.len() {
            return None;
        }
        Some(
            parameters[arguments.len()..]
                .iter()
                .cloned()
                .cloned()
                .collect(),
        )
    }

    pub(super) fn remaining_nominal_constructor_parameters(
        &self,
        source: &Type,
    ) -> Option<Vec<CompileParam>> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        let parameters =
            self.collection
                .struct_templates
                .get(name)
                .map(|template| template.compile_groups.iter().flatten().collect::<Vec<_>>())
                .or_else(|| {
                    self.collection.enum_templates.get(name).map(|template| {
                        template.compile_groups.iter().flatten().collect::<Vec<_>>()
                    })
                })?;
        if arguments.len() >= parameters.len() {
            return None;
        }
        Some(
            parameters[arguments.len()..]
                .iter()
                .cloned()
                .cloned()
                .collect(),
        )
    }

    pub(super) fn trait_ref_has_constructor_subject(&self, source: &Type) -> bool {
        let Type::Named(name, _) = source else {
            return false;
        };
        self.collection.traits.get(name).is_some_and(|schema| {
            matches!(schema.self_parameter.kind, Sort::TypeConstructor { .. })
        })
    }

    pub(super) fn resolve_trait_impl_ref(
        &mut self,
        source: &Type,
    ) -> Option<(TraitRefKey, TraitSchema, HashMap<String, Type>)> {
        let Type::Named(name, source_arguments) = source else {
            self.error("trait reference must name a trait");
            return None;
        };
        let Some(schema) = self.collection.traits.get(name).cloned() else {
            self.error(format!("unknown trait `{name}`"));
            return None;
        };
        if !schema.valid {
            return None;
        }
        if schema.self_parameter.kind != Sort::Type {
            self.error(format!(
                "trait `{name}` expects a type-constructor implementation target"
            ));
            return None;
        }
        if source_arguments.len() != schema.compile_parameters.len() {
            self.error(format!(
                "trait argument count mismatch for `{name}`: expected {}, found {}",
                schema.compile_parameters.len(),
                source_arguments.len()
            ));
            return None;
        }
        let normalized_arguments = schema
            .compile_parameters
            .iter()
            .zip(source_arguments)
            .map(|(parameter, argument)| {
                self.normalize_concrete_trait_argument(parameter, argument)
                    .or_else(|| {
                        (self.collection.instantiating_generic_trait_extension > 0
                            && self.source_type_is_abstract_or_concrete(argument))
                        .then(|| argument.clone())
                    })
            })
            .collect::<Option<Vec<_>>>();
        let Some(normalized_arguments) = normalized_arguments else {
            self.error(format!(
                "generic trait implementation of `{name}` is not supported; trait arguments must be concrete"
            ));
            return None;
        };
        let mut arguments = Vec::new();
        let mut substitutions = HashMap::new();
        for (parameter, source_argument) in
            schema.compile_parameters.iter().zip(&normalized_arguments)
        {
            let argument = self.lower_source_type(source_argument);
            if argument == Ty::Error {
                return None;
            }
            arguments.push(argument);
            substitutions.insert(parameter.name.clone(), source_argument.clone());
        }
        Some((
            TraitRefKey {
                name: name.clone(),
                arguments,
            },
            schema,
            substitutions,
        ))
    }

    pub(super) fn normalize_trait_impl_associated_type(
        &mut self,
        trait_name: &str,
        type_name: &str,
        raw: &HashMap<String, Type>,
        base_substitutions: &HashMap<String, Type>,
        normalized: &mut HashMap<String, Type>,
        visiting: &mut Vec<String>,
    ) -> Option<Type> {
        if let Some(ty) = normalized.get(type_name) {
            return Some(ty.clone());
        }
        if let Some(cycle_start) = visiting.iter().position(|name| name == type_name) {
            let mut cycle = visiting[cycle_start..].to_vec();
            cycle.push(type_name.to_owned());
            self.error(format!(
                "associated type cycle in implementation of `{trait_name}`: {}",
                cycle.join(" -> ")
            ));
            return None;
        }
        let source = raw.get(type_name)?.clone();
        visiting.push(type_name.to_owned());
        let resolved = self.normalize_trait_impl_type(
            trait_name,
            &source,
            raw,
            base_substitutions,
            normalized,
            visiting,
        );
        visiting.pop();
        if let Some(resolved) = &resolved {
            normalized.insert(type_name.to_owned(), resolved.clone());
        }
        resolved
    }

    pub(super) fn normalize_trait_impl_type(
        &mut self,
        trait_name: &str,
        source: &Type,
        raw: &HashMap<String, Type>,
        base_substitutions: &HashMap<String, Type>,
        normalized: &mut HashMap<String, Type>,
        visiting: &mut Vec<String>,
    ) -> Option<Type> {
        match source {
            Type::Tuple(fields) => Some(Type::Tuple(
                fields
                    .iter()
                    .map(|field| {
                        self.normalize_trait_impl_type(
                            trait_name,
                            field,
                            raw,
                            base_substitutions,
                            normalized,
                            visiting,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?,
            )),
            Type::Borrow {
                mutable,
                access,
                region,
                pointee,
            } => Some(Type::Borrow {
                mutable: *mutable,
                access: access.clone(),
                region: region.clone(),
                pointee: Box::new(self.normalize_trait_impl_type(
                    trait_name,
                    pointee,
                    raw,
                    base_substitutions,
                    normalized,
                    visiting,
                )?),
            }),
            Type::Named(name, arguments) if arguments.is_empty() => {
                if raw.contains_key(name) {
                    self.normalize_trait_impl_associated_type(
                        trait_name,
                        name,
                        raw,
                        base_substitutions,
                        normalized,
                        visiting,
                    )
                } else {
                    Some(
                        base_substitutions
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| source.clone()),
                    )
                }
            }
            Type::Array(element, length) => Some(Type::Array(
                Box::new(self.normalize_trait_impl_type(
                    trait_name,
                    element,
                    raw,
                    base_substitutions,
                    normalized,
                    visiting,
                )?),
                *length,
            )),
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => Some(Type::ArrayApplication {
                constructor: constructor.clone(),
                element: Box::new(self.normalize_trait_impl_type(
                    trait_name,
                    element,
                    raw,
                    base_substitutions,
                    normalized,
                    visiting,
                )?),
                length: match length {
                    crate::ast::USizeConst::Parameter(name) => match base_substitutions.get(name) {
                        Some(Type::CompileUSize(value)) => crate::ast::USizeConst::Literal(*value),
                        _ => length.clone(),
                    },
                    _ => length.clone(),
                },
            }),
            Type::Function {
                groups,
                effects,
                result,
            } => Some(Type::Function {
                groups: groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|ty| {
                                self.normalize_trait_impl_type(
                                    trait_name,
                                    ty,
                                    raw,
                                    base_substitutions,
                                    normalized,
                                    visiting,
                                )
                            })
                            .collect::<Option<Vec<_>>>()
                    })
                    .collect::<Option<Vec<_>>>()?,
                effects: effects.clone(),
                result: Box::new(self.normalize_trait_impl_type(
                    trait_name,
                    result,
                    raw,
                    base_substitutions,
                    normalized,
                    visiting,
                )?),
            }),
            Type::Named(name, arguments) => Some(Type::Named(
                name.clone(),
                arguments
                    .iter()
                    .map(|argument| {
                        self.normalize_trait_impl_type(
                            trait_name,
                            argument,
                            raw,
                            base_substitutions,
                            normalized,
                            visiting,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?,
            )),
            Type::NamedArgs(name, arguments) => Some(Type::NamedArgs(
                name.clone(),
                arguments
                    .iter()
                    .map(|argument| {
                        Some(crate::ast::TypeArg {
                            label: argument.label.clone(),
                            ty: self.normalize_trait_impl_type(
                                trait_name,
                                &argument.ty,
                                raw,
                                base_substitutions,
                                normalized,
                                visiting,
                            )?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?,
            )),
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::I128
            | Type::ISize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::USize
            | Type::Bool
            | Type::Unit
            | Type::CompileUSize(_) => Some(source.clone()),
        }
    }

    pub(super) fn function_shape(&mut self, function: &Function) -> Option<FunctionShape> {
        let groups = function
            .groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|parameter| {
                        let ty = self.lower_source_type(&parameter.ty);
                        (ty != Ty::Error).then_some((parameter.mode, ty))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        let result_source = function.return_type.as_ref()?;
        let result = self.lower_source_type(result_source);
        (result != Ty::Error).then_some(FunctionShape {
            groups,
            result,
            effects: function.effects.clone(),
        })
    }
}
