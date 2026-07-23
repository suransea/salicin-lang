//! Type checking and textual LLVM IR generation for Salicin's M0 subset.
//!
//! The backend intentionally consumes the parser AST directly but first lowers
//! it to a small typed representation.  No malformed program reaches the LLVM
//! emitter, which keeps the generated IR simple enough to inspect in tests.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use crate::alloc::AllocBundle;
use crate::ast::{
    BinaryOp, Binding, CallArg, CompileParam, CompileParamKind, EffectDef, EnumDef, Expr,
    ExtendDef, ExtendMember, Function, FunctionEffects, HandlerChainCall, Item, ItemOrigin,
    MatchArm, Param, PassMode, Pattern, PatternFields, Program, Stmt, StructDef, TraitDef,
    TraitMember, Type, UnaryOp, VariantFields, Visibility, WherePredicate,
};
use crate::core::{
    copy_trait_has_required_shape, drop_trait_has_required_shape,
    operator_trait_has_required_shape, unary_operator_trait_has_required_shape, CoreBundle,
    LangItemKind, LangItems,
};
use crate::manifest::Edition;
use crate::modules::PackageId;

mod access;
mod arrays;
mod assignment;
mod chain;
mod cleanup_plan;
mod coalesce;
mod compile_time;
mod control;
mod effects;
mod emitter;
mod fallible;
mod flow;
mod functions;
mod hir;
mod lower;
mod names;
mod operators;
mod ownership;
mod raw;
mod references;
mod registry;
mod source_rewrite;
mod throws;

use cleanup_plan::build_and_verify_cleanup_plans;
use compile_time::*;
use effects::*;
use emitter::{evaluate_globals, Emitter};
use fallible::*;
use flow::*;
use hir::*;
use lower::*;
use names::*;
use operators::*;
use registry::*;
use source_rewrite::*;

#[cfg(test)]
use cleanup_plan::{HirCleanupPlanner, MAX_CLEANUP_MOVE_PATHS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
}

impl Diagnostic {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Type-check `program` and emit portable textual LLVM IR using opaque
/// pointers.  The returned module deliberately omits a target triple so that
/// the caller can compile it for the selected LLVM target. The program must
/// already have passed module resolution; use the crate-level source entry
/// points when compiling parser input.
pub fn compile(program: &Program) -> Result<String, Vec<Diagnostic>> {
    compile_target(program, true)
}

/// Type-check `program` and emit LLVM IR for a library target. Unlike
/// [`compile`], this does not require `main` or generate the platform entry
/// wrapper. The program must already have passed module resolution.
pub fn compile_library(program: &Program) -> Result<String, Vec<Diagnostic>> {
    compile_target(program, false)
}

fn compile_target(program: &Program, require_entry_point: bool) -> Result<String, Vec<Diagnostic>> {
    let mut analyzer =
        Analyzer::try_new(program).map_err(|error| vec![Diagnostic::new(error.to_string())])?;
    let hir = if require_entry_point {
        analyzer.analyze()
    } else {
        analyzer.analyze_target(false)
    };
    if !analyzer.diagnostics.is_empty() {
        return Err(analyzer.diagnostics);
    }

    let hir = hir.expect("analysis without diagnostics must produce HIR");
    let cleanup_plans = build_and_verify_cleanup_plans(&hir)?;
    let constants = evaluate_globals(&hir)?;

    match Emitter::new(&hir, constants, &cleanup_plans).emit_module(require_entry_point) {
        Ok(ir) => Ok(ir),
        Err(error) => Err(vec![error]),
    }
}

/// Type-check a library target without requiring or emitting a binary entry
/// point. Global constants are still evaluated so library checks report the
/// same constant-expression diagnostics as binary compilation. The program
/// must already have passed module resolution.
pub fn check_library(program: &Program) -> Result<(), Vec<Diagnostic>> {
    let mut analyzer =
        Analyzer::try_new(program).map_err(|error| vec![Diagnostic::new(error.to_string())])?;
    let hir = analyzer.analyze_target(false);
    if !analyzer.diagnostics.is_empty() {
        return Err(analyzer.diagnostics);
    }

    let hir = hir.expect("analysis without diagnostics must produce HIR");
    let _cleanup_plans = build_and_verify_cleanup_plans(&hir)?;
    evaluate_globals(&hir).map(|_| ())
}

struct Analyzer {
    lang_items: LangItems,
    functions: HashMap<String, Function>,
    function_origins: HashMap<String, ItemOrigin>,
    function_accesses: HashMap<String, AccessBoundary>,
    function_templates: HashMap<String, Function>,
    function_overloads: HashMap<String, Vec<String>>,
    function_template_origins: HashMap<String, ItemOrigin>,
    function_template_order: Vec<String>,
    function_instance_names: HashMap<FunctionInstanceKey, String>,
    function_instances: HashMap<String, FunctionInstanceInfo>,
    function_type_substitutions: HashMap<String, HashMap<String, Type>>,
    abstract_type_parameters: HashMap<String, String>,
    globals: HashMap<String, Binding>,
    global_origins: HashMap<String, ItemOrigin>,
    global_accesses: HashMap<String, AccessBoundary>,
    struct_defs: HashMap<String, StructDef>,
    enum_defs: HashMap<String, EnumDef>,
    struct_templates: HashMap<String, StructDef>,
    enum_templates: HashMap<String, EnumDef>,
    type_aliases: HashMap<String, crate::ast::TypeAliasDef>,
    struct_template_order: Vec<String>,
    enum_template_order: Vec<String>,
    nominal_instance_names: HashMap<NominalInstanceKey, String>,
    nominal_instances: HashMap<String, NominalInstanceInfo>,
    nominal_instance_states: HashMap<NominalInstanceKey, NominalInstanceState>,
    invalid_recursive_nominals: HashSet<String>,
    struct_layouts: HashMap<String, StructLayout>,
    enum_layouts: HashMap<String, EnumLayout>,
    nominal_accesses: HashMap<String, AccessBoundary>,
    inherent_members: HashMap<String, InherentMemberSet>,
    inherent_overload_counts: HashMap<InherentOverloadKey, usize>,
    inherent_overloads: HashMap<InherentOverloadKey, Vec<String>>,
    inherent_overload_shapes: HashMap<InherentOverloadKey, HashSet<ParameterLabelShape>>,
    generic_inherent_extensions: HashMap<String, Vec<GenericInherentExtension>>,
    generic_trait_extensions: HashMap<String, Vec<GenericTraitExtension>>,
    instantiating_generic_trait_extension: usize,
    generic_inherent_functions: HashMap<(String, String), String>,
    suppress_generic_inherent_instantiation: usize,
    traits: HashMap<String, TraitSchema>,
    effects: HashSet<String>,
    effect_defs: HashMap<String, EffectDef>,
    trait_impl_headers: HashSet<TraitImplKey>,
    constructor_trait_impl_headers: HashSet<ConstructorTraitImplKey>,
    constructor_trait_impl_methods: HashMap<ConstructorTraitImplKey, HashMap<String, String>>,
    trait_impls: HashMap<TraitImplKey, TraitImplInfo>,
    trait_methods_by_receiver: HashMap<(Ty, String), Vec<TraitImplKey>>,
    copy_nominals: HashSet<Ty>,
    copy_impls_finalized: bool,
    function_order: Vec<String>,
    global_order: Vec<String>,
    struct_order: Vec<String>,
    enum_order: Vec<String>,
    signatures: HashMap<String, FunctionSig>,
    global_annotations: HashMap<String, Option<Ty>>,
    function_states: HashMap<String, ResolutionState>,
    global_states: HashMap<String, ResolutionState>,
    hir_functions: HashMap<String, HirFunction>,
    lifted_functions: Vec<HirFunction>,
    next_closure: usize,
    handler_frame_parameter_modes: HashMap<String, Vec<PassMode>>,
    hir_globals: HashMap<String, HirGlobal>,
    array_types: HashSet<Ty>,
    continuation_adapters: Vec<ContinuationAdapter>,
    effect_callable_adapters: Vec<EffectCallableAdapter>,
    runtime_handler_actions: HashMap<(String, usize, usize), RuntimeHandlerAction>,
    diagnostics: Vec<Diagnostic>,
}

impl Analyzer {
    fn try_new(program: &Program) -> Result<Self, String> {
        let core =
            CoreBundle::for_edition(Edition::Edition2026).map_err(|error| error.to_string())?;
        let alloc =
            AllocBundle::for_edition(Edition::Edition2026).map_err(|error| error.to_string())?;
        let mut analyzer = Self {
            lang_items: core.lang_items().clone(),
            functions: HashMap::new(),
            function_origins: HashMap::new(),
            function_accesses: HashMap::new(),
            function_templates: HashMap::new(),
            function_overloads: HashMap::new(),
            function_template_origins: HashMap::new(),
            function_template_order: Vec::new(),
            function_instance_names: HashMap::new(),
            function_instances: HashMap::new(),
            function_type_substitutions: HashMap::new(),
            abstract_type_parameters: HashMap::new(),
            globals: HashMap::new(),
            global_origins: HashMap::new(),
            global_accesses: HashMap::new(),
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            struct_templates: HashMap::new(),
            enum_templates: HashMap::new(),
            type_aliases: HashMap::new(),
            struct_template_order: Vec::new(),
            enum_template_order: Vec::new(),
            nominal_instance_names: HashMap::new(),
            nominal_instances: HashMap::new(),
            nominal_instance_states: HashMap::new(),
            invalid_recursive_nominals: HashSet::new(),
            struct_layouts: HashMap::new(),
            enum_layouts: HashMap::new(),
            nominal_accesses: HashMap::new(),
            inherent_members: HashMap::new(),
            inherent_overload_counts: HashMap::new(),
            inherent_overloads: HashMap::new(),
            inherent_overload_shapes: HashMap::new(),
            generic_inherent_extensions: HashMap::new(),
            generic_trait_extensions: HashMap::new(),
            instantiating_generic_trait_extension: 0,
            generic_inherent_functions: HashMap::new(),
            suppress_generic_inherent_instantiation: 0,
            traits: HashMap::new(),
            effects: HashSet::new(),
            effect_defs: HashMap::new(),
            trait_impl_headers: HashSet::new(),
            constructor_trait_impl_headers: HashSet::new(),
            constructor_trait_impl_methods: HashMap::new(),
            trait_impls: HashMap::new(),
            trait_methods_by_receiver: HashMap::new(),
            copy_nominals: HashSet::new(),
            copy_impls_finalized: false,
            function_order: Vec::new(),
            global_order: Vec::new(),
            struct_order: Vec::new(),
            enum_order: Vec::new(),
            signatures: HashMap::new(),
            global_annotations: HashMap::new(),
            function_states: HashMap::new(),
            global_states: HashMap::new(),
            hir_functions: HashMap::new(),
            lifted_functions: Vec::new(),
            next_closure: 0,
            handler_frame_parameter_modes: HashMap::new(),
            hir_globals: HashMap::new(),
            array_types: HashSet::new(),
            continuation_adapters: Vec::new(),
            effect_callable_adapters: Vec::new(),
            runtime_handler_actions: HashMap::new(),
            diagnostics: Vec::new(),
        };
        if !program.uses.is_empty() {
            analyzer.error(
                "unresolved `use` declarations reached semantic analysis; resolve source modules before code generation",
            );
        }
        let mut core_program = core.program().clone();
        let mut alloc_program = alloc.program().clone();
        let mut source_program = program.clone();
        erase_region_parameters(&mut core_program);
        erase_region_parameters(&mut alloc_program);
        erase_region_parameters(&mut source_program);
        for diagnostic in normalize_labeled_type_arguments([
            &mut core_program,
            &mut alloc_program,
            &mut source_program,
        ]) {
            analyzer.error(diagnostic);
        }
        promote_inferred_type_aliases([&mut core_program, &mut alloc_program, &mut source_program]);
        analyzer.type_aliases =
            collect_type_aliases([&core_program, &alloc_program, &source_program]);
        for diagnostic in
            expand_type_aliases([&mut core_program, &mut alloc_program, &mut source_program])
        {
            analyzer.error(diagnostic);
        }
        analyzer.collect_items(&core_program, &alloc_program, &source_program);
        Ok(analyzer)
    }

    #[cfg(test)]
    fn new(program: &Program) -> Self {
        Self::try_new(program)
            .expect("the compiler-embedded edition 2026 core bundle must be valid")
    }

    fn lang_item_name(&self, kind: LangItemKind) -> &str {
        self.lang_items.get(kind).canonical_name()
    }

    fn collect_items(&mut self, core: &Program, alloc: &Program, program: &Program) {
        let mut names = HashMap::<String, HashSet<TopLevelNamespace>>::new();
        for reserved in [
            "Ptr",
            "MutPtr",
            "raw_alloc",
            "raw_dealloc",
            "raw_init",
            "raw_take",
            "raw_offset",
            "raw_borrow",
            "raw_trap",
            "forget",
            "size_of",
            "align_of",
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
        let all_items = prelude_items
            .chain(alloc_items)
            .chain(source_items)
            .collect::<Vec<_>>();
        let mut function_counts = HashMap::<String, usize>::new();
        for (item, _, _) in &all_items {
            if let Item::Function(function) = item {
                *function_counts.entry(function.name.clone()).or_default() += 1;
            }
        }
        let mut overload_shapes = HashMap::<String, HashSet<ParameterLabelShape>>::new();
        let mut overload_visibilities = HashMap::<String, Visibility>::new();
        for (item, visibility, origin) in all_items {
            let name = match item {
                Item::Function(function) => &function.name,
                Item::Global(binding) => &binding.name,
                Item::Struct(definition) => &definition.name,
                Item::Enum(definition) => &definition.name,
                Item::Effect(definition) => &definition.name,
                Item::Domain(definition) => &definition.name,
                Item::TypeAlias(definition) => &definition.name,
                Item::Trait(definition) => &definition.name,
                Item::Extend(extension) => {
                    extensions.push((extension.clone(), origin));
                    continue;
                }
            };
            let namespace = top_level_namespace(item);
            let overloaded_function = matches!(item, Item::Function(_))
                && function_counts.get(name).copied().unwrap_or_default() > 1;
            let occupied = names.get(name).cloned().unwrap_or_default();
            let duplicate = match namespace {
                TopLevelNamespace::Function => {
                    occupied.contains(&TopLevelNamespace::Other)
                        || (occupied.contains(&TopLevelNamespace::Function) && !overloaded_function)
                }
                TopLevelNamespace::Type => {
                    occupied.contains(&TopLevelNamespace::Other)
                        || occupied.contains(&TopLevelNamespace::Type)
                }
                TopLevelNamespace::Other => !occupied.is_empty(),
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
                    if origin.package != PackageId::CORE.0
                        && matches!(
                            source_name.rsplit("::").next(),
                            Some("do" | "try" | "throw" | "unsafe" | "loop")
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
                        self.function_overloads
                            .entry(source_name)
                            .or_default()
                            .push(function.name.clone());
                    }
                    self.function_accesses.insert(
                        function.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                    if function.compile_groups.is_empty() {
                        self.function_order.push(function.name.clone());
                        self.functions
                            .insert(function.name.clone(), function.clone());
                        self.function_origins
                            .insert(function.name.clone(), origin.clone());
                    } else {
                        self.function_template_order.push(function.name.clone());
                        self.function_templates
                            .insert(function.name.clone(), function.clone());
                        self.function_template_origins
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
                    self.global_order.push(binding.name.clone());
                    self.globals.insert(binding.name.clone(), binding.clone());
                    self.global_origins
                        .insert(binding.name.clone(), origin.clone());
                    self.global_accesses.insert(
                        binding.name.clone(),
                        AccessBoundary {
                            visibility,
                            origin: origin.clone(),
                        },
                    );
                }
                Item::Struct(definition) => {
                    self.nominal_accesses.insert(
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
                        self.nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.struct_order.push(definition.name.clone());
                        self.struct_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        self.struct_template_order.push(definition.name.clone());
                        self.struct_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                    for derive in &definition.derives {
                        match derive.as_str() {
                            "Copy" => {
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
                    self.nominal_accesses.insert(
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
                        self.nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.enum_order.push(definition.name.clone());
                        self.enum_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        self.enum_template_order.push(definition.name.clone());
                        self.enum_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                }
                Item::Effect(definition) => {
                    if definition.compile_groups.len() > 1
                        || definition
                            .compile_groups
                            .iter()
                            .flatten()
                            .any(|parameter| parameter.kind != CompileParamKind::Type)
                    {
                        self.error(format!(
                            "effect `{}` currently accepts one compile-time group containing only `type` parameters",
                            definition.name
                        ));
                    }
                    self.effects.insert(definition.name.clone());
                    self.effect_defs
                        .insert(definition.name.clone(), definition.clone());
                }
                Item::Domain(_) => {}
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
                    .inherent_overload_counts
                    .entry((target.clone(), function.name.clone(), is_method))
                    .or_default() += 1;
            }
        }
        let mut remaining_extensions = Vec::new();
        for (extension, origin) in extensions {
            if self.is_core_copy_extension(&extension) {
                self.collect_extension(extension, origin);
            } else {
                remaining_extensions.push((extension, origin));
            }
        }
        self.validate_copy_implementations();
        self.activate_generic_copy_extensions();
        self.validate_copy_implementations();
        self.copy_impls_finalized = true;
        self.validate_trait_schemas();
        for (extension, origin) in remaining_extensions {
            self.collect_extension(extension, origin);
        }
        self.validate_trait_inheritance_implementations();

        let never = self.lang_item_name(LangItemKind::Never);
        if !self.enum_defs.contains_key(never) {
            self.error("compiler core did not register its validated `Never` declaration");
        }

        for name in self.function_order.clone() {
            let function = self.functions[&name].clone();
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
            let throws_error = function
                .effects
                .throws
                .as_deref()
                .map(|error| self.lower_source_type(error));
            self.signatures.insert(
                name,
                FunctionSig {
                    groups,
                    unsafe_effect: self.function_effects_unsafe(&function.effects),
                    throws_error,
                    custom_effects: self.function_effects_custom_identities(&function.effects),
                    result,
                },
            );
        }
        self.register_runtime_handler_actions();
        for name in self.global_order.clone() {
            let binding = self.globals[&name].clone();
            let annotation = binding
                .annotation
                .as_ref()
                .map(|ty| self.lower_source_type(ty));
            self.global_annotations.insert(name, annotation);
        }

        self.validate_nominal_templates();
        self.validate_function_templates();
    }

    fn register_runtime_handler_actions(&mut self) {
        for function_name in self.function_order.clone() {
            let function = self.functions[&function_name].clone();
            let Some(body) = function.body.as_ref() else {
                continue;
            };
            let Some(answer_source) = function.return_type.as_ref() else {
                continue;
            };
            let answer = self.lower_source_type(answer_source);
            for (group_index, group) in function.groups.iter().enumerate() {
                for (parameter_index, parameter) in group.iter().enumerate() {
                    if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                        continue;
                    }
                    let Type::Function {
                        groups,
                        effects,
                        result,
                    } = &parameter.ty
                    else {
                        continue;
                    };
                    if self.function_effects_unsafe(effects)
                        || effects.throws.is_some()
                        || !effects.parameters.is_empty()
                        || self.function_effects_custom_identities(effects).len() != 1
                        || groups.len() != 1
                        || groups[0].len() > 1
                    {
                        continue;
                    }
                    let effect = self
                        .function_effects_custom_identities(effects)
                        .into_iter()
                        .next()
                        .expect("exactly one normalized custom effect");
                    let root = effect.split('(').next().unwrap_or(&effect);
                    if self
                        .effect_defs
                        .get(root)
                        .is_none_or(|definition| definition.operations.is_empty())
                        || function
                            .effects
                            .custom
                            .iter()
                            .filter(|candidate| !self.is_standard_unsafe_effect_source(candidate))
                            .any(|candidate| source_effect_identity(candidate) == effect)
                        || !expression_handles_effect(body, &effect)
                    {
                        continue;
                    }
                    let input = groups[0]
                        .first()
                        .map(|input| self.lower_source_type(input))
                        .unwrap_or(Ty::Unit);
                    let output = self.lower_source_type(result);
                    self.runtime_handler_actions.insert(
                        (function_name.clone(), group_index, parameter_index),
                        RuntimeHandlerAction {
                            effect,
                            input,
                            output,
                            answer: answer.clone(),
                            accepts_input: !groups[0].is_empty(),
                        },
                    );
                }
            }
        }
    }

    fn validate_program_effects(&mut self, program: &Program) {
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
                Item::Global(_) | Item::Struct(_) | Item::Enum(_) | Item::Domain(_) => Vec::new(),
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
                if !self.effects.contains(name) {
                    self.error(format!(
                        "unknown custom effect `{}` in function `{}`",
                        source_effect_identity(effect),
                        function.name
                    ));
                } else if let Some(definition) = self.effect_defs.get(name) {
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

    fn is_core_copy_extension(&self, extension: &ExtendDef) -> bool {
        matches!(
            extension.trait_ref.as_ref(),
            Some(Type::Named(name, arguments))
                if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty()
        )
    }

    fn derived_copy_extension(&mut self, definition: &StructDef) -> Option<ExtendDef> {
        let parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        for parameter in &parameters {
            if parameter.kind != CompileParamKind::Type {
                self.error(format!(
                    "struct `{}` cannot derive `Copy` with non-type compile-time parameter `{}`",
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

    fn validate_copy_implementations(&mut self) {
        let copy_name = self.lang_item_name(LangItemKind::Copy);
        let mut candidates = self
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
                    "`{target_name}` cannot implement `Copy`: {member} has type `{ty}`, which does not implement `Copy`"
                ));
            } else {
                self.error(format!(
                    "`{target_name}` cannot implement `Copy` because its value layout is not Copy"
                ));
            }
            self.trait_impls.remove(key);
        }
        self.copy_nominals = valid;
    }

    fn validate_dynamic_copy_implementation(&mut self, key: &TraitImplKey) {
        let target = &key.self_ty;
        if self.copy_layout_is_valid(target, &self.copy_nominals) {
            self.copy_nominals.insert(target.clone());
            return;
        }
        let target_name = self.diagnostic_type_name(target);
        if let Some((member, ty)) = self.first_non_copy_member(target, &self.copy_nominals) {
            let ty = self.diagnostic_type_name(&ty);
            self.error(format!(
                "`{target_name}` cannot implement `Copy`: {member} has type `{ty}`, which does not implement `Copy`"
            ));
        } else {
            self.error(format!(
                "`{target_name}` cannot implement `Copy` because its value layout is not Copy"
            ));
        }
        self.trait_impls.remove(key);
        self.trait_impl_headers.remove(key);
    }

    fn activate_generic_copy_extensions(&mut self) {
        let copy_name = self.lang_item_name(LangItemKind::Copy).to_owned();
        let template_names = self
            .generic_trait_extensions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for template_name in template_names {
            let extensions = self
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
            self.generic_trait_extensions
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

    fn generic_copy_extension_is_structural(
        &mut self,
        template_name: &str,
        extension: &GenericTraitExtension,
    ) -> bool {
        let (kind, parameters) = if let Some(template) = self.struct_templates.get(template_name) {
            (
                NominalKind::Struct,
                template
                    .compile_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        } else if let Some(template) = self.enum_templates.get(template_name) {
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
            self.abstract_type_parameters
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
        let copy_before = self.copy_nominals.clone();
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
                self.copy_nominals.insert(subject);
            }
        }
        self.suppress_generic_inherent_instantiation += 1;
        let instance =
            self.ensure_nominal_instance(kind, template_name, source_arguments, arguments);
        self.suppress_generic_inherent_instantiation -= 1;
        let valid = instance.as_ref().is_some_and(|canonical| {
            let target = match kind {
                NominalKind::Struct => Ty::Struct(canonical.clone()),
                NominalKind::Enum => Ty::Enum(canonical.clone()),
            };
            self.copy_layout_is_valid(&target, &self.copy_nominals)
        });
        if !valid {
            self.error(format!(
                "blanket `Copy` implementation for `{template_name}` is not structurally valid for every instance allowed by its where predicates"
            ));
        }
        self.restore_nominals(nominals_before);
        self.copy_nominals = copy_before;
        valid
    }

    fn collect_trait_schema(
        &mut self,
        definition: TraitDef,
        visibility: Visibility,
        origin: ItemOrigin,
    ) {
        let mut valid = true;
        if definition.name == self.lang_item_name(LangItemKind::Copy)
            && !copy_trait_has_required_shape(&definition)
        {
            self.error("`Copy` language trait must have shape `let Copy = trait {}`");
            valid = false;
        }
        if definition.name == self.lang_item_name(LangItemKind::Drop)
            && !drop_trait_has_required_shape(&definition)
        {
            self.error(
                "`Drop` language trait must have shape `let Drop = trait { let drop(borrow(mut) self)(): () }`",
            );
            valid = false;
        }
        let operator_trait = BINARY_OPERATOR_TRAITS
            .iter()
            .copied()
            .find(|candidate| definition.name == self.lang_item_name(candidate.lang_item));
        if let Some(operator_trait) = operator_trait {
            if !operator_trait_has_required_shape(operator_trait.lang_item, &definition) {
                let trait_name = operator_trait.lang_item.source_name();
                let method = operator_trait.method();
                let shape = match operator_trait.lang_item {
                    LangItemKind::Eq => format!(
                        "let Eq(Rhs: type) = trait {{ let {method}(borrow self)(borrow rhs: Rhs): bool }}"
                    ),
                    LangItemKind::PartialOrd => format!(
                        "let PartialOrd(Rhs: type) = trait {{ let {method}(borrow self)(borrow rhs: Rhs): PartialOrdering }}"
                    ),
                    _ => format!(
                        "let {trait_name}(Rhs: type) = trait {{ let Output: type; let {method}(move self)(move rhs: Rhs): Output }}"
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
            .copied()
            .find(|candidate| definition.name == self.lang_item_name(candidate.lang_item));
        if let Some(operator) = unary_operator {
            if !unary_operator_trait_has_required_shape(operator.lang_item, &definition) {
                let trait_name = operator.lang_item.source_name();
                let method = operator.method();
                self.error(format!(
                    "`{trait_name}` language trait must have shape `let {trait_name} = trait {{ let Output: type; let {method}(move self)(): Output }}`"
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
        if definition.self_parameter.name != "Self" {
            self.error(format!(
                "trait `{}` self kind parameter must be named `Self`",
                definition.name
            ));
            valid = false;
        }
        if !matches!(
            definition.self_parameter.kind,
            CompileParamKind::Type
                | CompileParamKind::TypeConstructor { .. }
                | CompileParamKind::Effect
        ) {
            self.error(format!(
                "trait `{}` self kind must be `type`, a type-constructor kind, or `effect`, found {}",
                definition.name,
                describe_compile_param_kind(definition.self_parameter.kind)
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
            if parameter.name == "Self" {
                self.error(format!(
                    "trait `{}` cannot declare reserved type parameter `Self`",
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
        let mut methods = HashMap::new();
        let mut method_overloads = HashMap::<String, Vec<String>>::new();
        let mut overload_shapes = HashMap::<String, HashSet<ParameterLabelShape>>::new();
        let mut method_order = Vec::new();
        for member in definition.members {
            match member {
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
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
                    if name == "Self" || compile_parameter_names.contains(&name) {
                        self.error(format!(
                            "associated type `{}.{name}` conflicts with a trait type parameter",
                            definition.name
                        ));
                        valid = false;
                    }
                    let kind = if compile_groups.is_empty() {
                        CompileParamKind::Type
                    } else {
                        let mut parameter_count = 0usize;
                        let mut groups_valid = true;
                        for parameter in compile_groups.iter().flatten() {
                            parameter_count += 1;
                            if parameter.kind != CompileParamKind::Type {
                                self.error(format!(
                                    "generic associated type `{}.{name}` parameters currently must have kind `type`",
                                    definition.name
                                ));
                                groups_valid = false;
                            }
                        }
                        if groups_valid {
                            CompileParamKind::TypeConstructor { parameter_count }
                        } else {
                            valid = false;
                            CompileParamKind::Type
                        }
                    };
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
        self.traits.insert(
            definition.name,
            TraitSchema {
                self_parameter: definition.self_parameter,
                compile_parameters,
                where_predicates: definition.where_predicates,
                associated_types,
                associated_type_kinds,
                methods,
                method_overloads,
                method_order,
                access: AccessBoundary { visibility, origin },
                valid,
            },
        );
    }

    fn validate_trait_schemas(&mut self) {
        let mut trait_names = self.traits.keys().cloned().collect::<Vec<_>>();
        trait_names.sort();
        for trait_name in trait_names {
            let schema = self.traits[&trait_name].clone();
            let mut compile_parameter_kinds = schema
                .compile_parameters
                .iter()
                .map(|parameter| (parameter.name.clone(), parameter.kind))
                .collect::<HashMap<_, _>>();
            compile_parameter_kinds.insert("Self".to_owned(), schema.self_parameter.kind);
            compile_parameter_kinds.extend(
                schema
                    .associated_types
                    .iter()
                    .map(|name| (name.clone(), schema.associated_type_kinds[name])),
            );
            let mut valid = schema.valid;
            valid &= self.validate_where_predicate_shapes(
                &format!("trait `{trait_name}`"),
                &schema.where_predicates,
                &compile_parameter_kinds,
            );
            for method_name in &schema.method_order {
                let method = &schema.methods[method_name];
                let mut method_compile_parameter_kinds = compile_parameter_kinds.clone();
                method_compile_parameter_kinds.extend(
                    method
                        .compile_groups
                        .iter()
                        .flatten()
                        .map(|parameter| (parameter.name.clone(), parameter.kind)),
                );
                for parameter in method.groups.iter().flatten() {
                    valid &= self.validate_trait_source_type(
                        &trait_name,
                        method_name,
                        &parameter.ty,
                        &method_compile_parameter_kinds,
                    );
                    if parameter.mode == PassMode::Copy
                        && !self.trait_source_type_is_definitely_copy(&parameter.ty)
                    {
                        self.error(format!(
                            "trait method `{}.{method_name}` parameter `{}` requires `Copy`, but its type is not provably Copy without a trait bound",
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
                        &method_compile_parameter_kinds,
                    );
                }
                valid &= self.validate_trait_source_effects(
                    &trait_name,
                    method_name,
                    &method.effects,
                    &method_compile_parameter_kinds,
                );
            }
            self.traits
                .get_mut(&trait_name)
                .expect("trait schema exists")
                .valid = valid;
            if valid {
                self.register_trait_default_validation_templates(&trait_name, &schema);
            }
        }
    }

    fn register_trait_default_validation_templates(
        &mut self,
        trait_name: &str,
        schema: &TraitSchema,
    ) {
        let self_parameter = "$default$Self".to_owned();
        let mut compile_parameters = schema.compile_parameters.clone();
        compile_parameters.push(CompileParam {
            name: self_parameter.clone(),
            kind: schema.self_parameter.kind,
        });
        compile_parameters.extend(schema.associated_types.iter().map(|name| CompileParam {
            name: name.clone(),
            kind: schema.associated_type_kinds[name],
        }));
        let trait_arguments = schema
            .compile_parameters
            .iter()
            .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
            .collect();
        let associated_types = schema
            .associated_types
            .iter()
            .filter(|name| schema.associated_type_kinds[*name] == CompileParamKind::Type)
            .map(|name| crate::ast::AssociatedTypeBinding {
                name: name.clone(),
                ty: Type::Named(name.clone(), Vec::new()),
            })
            .collect();
        let predicate = crate::ast::WherePredicate {
            subject: Type::Named(self_parameter.clone(), Vec::new()),
            trait_ref: Type::Named(trait_name.to_owned(), trait_arguments),
            associated_types,
        };
        let mut self_substitution = HashMap::new();
        self_substitution.insert("Self".to_owned(), Type::Named(self_parameter, Vec::new()));
        for method_name in &schema.method_order {
            let method = &schema.methods[method_name];
            if method.body.is_none() {
                continue;
            }
            let canonical = format!(
                "$trait$default$validation${trait_name}${method_name}${}",
                self.function_template_order.len()
            );
            let mut template = method.clone();
            template.name = canonical.clone();
            template.compile_groups = vec![compile_parameters.clone()];
            template.where_predicates = vec![predicate.clone()];
            substitute_function_types(&mut template, &self_substitution);
            if let Some(body) = &mut template.body {
                rewrite_abstract_self_qualified_methods(body);
            }
            self.function_template_order.push(canonical.clone());
            self.function_templates.insert(canonical.clone(), template);
            self.function_template_origins
                .insert(canonical.clone(), schema.access.origin.clone());
            self.function_accesses
                .insert(canonical, schema.access.clone());
        }
    }

    fn trait_source_type_is_definitely_copy(&self, source: &Type) -> bool {
        self.probe_source_ty(source)
            .is_some_and(|ty| self.is_copy_type(&ty))
    }

    fn validate_trait_source_type(
        &mut self,
        trait_name: &str,
        member_name: &str,
        source: &Type,
        compile_parameters: &HashMap<String, CompileParamKind>,
    ) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => true,
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
                    .copied()
                    .expect("checked compile parameter exists");
                match kind {
                    CompileParamKind::Type => {
                        if arguments.is_empty() {
                            true
                        } else {
                            self.error(format!(
                                "trait type parameter `{name}` in `{trait_name}.{member_name}` does not accept type arguments"
                            ));
                            false
                        }
                    }
                    CompileParamKind::TypeConstructor { parameter_count } => {
                        let mut valid = true;
                        if arguments.len() != parameter_count {
                            self.error(format!(
                                "type constructor parameter `{name}` in `{trait_name}.{member_name}` expects {parameter_count} type arguments, found {}",
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
                    CompileParamKind::EffectConstructor { .. } => {
                        self.error(format!(
                            "effect constructor parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    CompileParamKind::Effect => {
                        self.error(format!(
                            "effect row parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    CompileParamKind::Access => {
                        self.error(format!(
                            "access parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    CompileParamKind::Passing => {
                        self.error(format!(
                            "passing parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                    CompileParamKind::Region => {
                        self.error(format!(
                            "region parameter `{name}` in `{trait_name}.{member_name}` cannot be used as a runtime type"
                        ));
                        false
                    }
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                    true
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
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
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.enum_templates
                            .get(name)
                            .map(|template| template.compile_groups.iter().flatten().count())
                    });
                let Some(expected) = expected else {
                    if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
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

    fn validate_trait_source_effects(
        &mut self,
        trait_name: &str,
        member_name: &str,
        effects: &FunctionEffects,
        compile_parameters: &HashMap<String, CompileParamKind>,
    ) -> bool {
        let mut valid = true;
        if let Some(error) = &effects.throws {
            valid &=
                self.validate_trait_source_type(trait_name, member_name, error, compile_parameters);
        }
        for parameter in &effects.parameters {
            match compile_parameters.get(parameter).copied() {
                Some(CompileParamKind::Effect) => {}
                Some(kind) => {
                    self.error(format!(
                        "effect row `{parameter}` in trait member `{trait_name}.{member_name}` has incompatible compile-time kind {}",
                        describe_compile_param_kind(kind)
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

    fn validate_trait_source_effect(
        &mut self,
        trait_name: &str,
        member_name: &str,
        effect: &Type,
        compile_parameters: &HashMap<String, CompileParamKind>,
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
                if let Some(kind) = compile_parameters.get(name).copied() {
                    return match kind {
                        CompileParamKind::EffectConstructor { parameter_count } => {
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
                        CompileParamKind::Effect => {
                            self.error(format!(
                                "effect row parameter `{name}` in trait member `{trait_name}.{member_name}` does not accept effect arguments"
                            ));
                            false
                        }
                        _ => {
                            self.error(format!(
                                "compile-time parameter `{name}` in trait member `{trait_name}.{member_name}` has kind {}, not `effect`",
                                describe_compile_param_kind(kind)
                            ));
                            false
                        }
                    };
                }
                return valid;
            }
            _ => return true,
        };

        if let Some(kind) = compile_parameters.get(name).copied() {
            match kind {
                CompileParamKind::EffectConstructor { parameter_count } => {
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
                CompileParamKind::Effect => {
                    self.error(format!(
                        "effect row parameter `{name}` in trait member `{trait_name}.{member_name}` does not accept effect arguments"
                    ));
                    false
                }
                _ => {
                    self.error(format!(
                        "compile-time parameter `{name}` in trait member `{trait_name}.{member_name}` has kind {}, not `effect`",
                        describe_compile_param_kind(kind)
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

    fn source_type_is_concrete(&self, source: &Type) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => true,
            Type::Borrow { pointee, .. } => self.source_type_is_concrete(pointee),
            Type::Array(element, _) => self.source_type_is_concrete(element),
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
                self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name)
            }
            Type::Named(name, arguments) => {
                let expected = self
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.enum_templates
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

    fn source_type_is_abstract_or_concrete(&self, source: &Type) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => true,
            Type::Borrow { pointee, .. } => self.source_type_is_abstract_or_concrete(pointee),
            Type::Array(element, _) => self.source_type_is_abstract_or_concrete(element),
            Type::Function { groups, result, .. } => {
                groups
                    .iter()
                    .flatten()
                    .all(|ty| self.source_type_is_abstract_or_concrete(ty))
                    && self.source_type_is_abstract_or_concrete(result)
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                self.abstract_type_parameters.contains_key(name)
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
            }
            Type::Named(name, arguments) => {
                (self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name))
                    && arguments
                        .iter()
                        .all(|argument| self.source_type_is_abstract_or_concrete(argument))
            }
            Type::NamedArgs(_, _) => false,
        }
    }

    fn resolve_trait_impl_target(&mut self, source: &Type) -> Option<Ty> {
        let Type::Named(name, arguments) = source else {
            self.error("trait implementation target must be a nominal type");
            return None;
        };
        if (self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name))
            && (arguments.is_empty() || !self.source_type_is_concrete(source))
        {
            self.error(format!(
                "generic trait implementation for `{name}` is not supported; use a concrete type such as `{name}(i32)`"
            ));
            return None;
        }
        if arguments.is_empty()
            && !self.struct_defs.contains_key(name)
            && !self.enum_defs.contains_key(name)
        {
            self.error(format!("unknown extension target `{name}`"));
            return None;
        }
        let target = self.lower_source_type(source);
        match target {
            Ty::Struct(_) | Ty::Enum(_) => Some(target),
            Ty::Error => None,
            _ => {
                self.error("trait implementation target must be a nominal type");
                None
            }
        }
    }

    fn type_constructor_impl_target(&self, source: &Type) -> Option<TypeConstructorImplTarget> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        if let Some(template) = self.struct_templates.get(name) {
            return Some(TypeConstructorImplTarget {
                name: name.clone(),
                kind: NominalKind::Struct,
                parameter_count: template.compile_groups.iter().flatten().count(),
            });
        }
        if let Some(template) = self.enum_templates.get(name) {
            return Some(TypeConstructorImplTarget {
                name: name.clone(),
                kind: NominalKind::Enum,
                parameter_count: template.compile_groups.iter().flatten().count(),
            });
        }
        None
    }

    fn partial_nominal_constructor_trait_target(
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
            },
            self_constructor: source.clone(),
        })
    }

    fn partial_alias_constructor_trait_target(
        &mut self,
        source: &Type,
        declared_parameters: &[CompileParam],
    ) -> Option<GenericConstructorTraitExtensionTarget> {
        let Type::Named(alias_name, supplied_arguments) = source else {
            return None;
        };
        let alias = self.type_aliases.get(alias_name).cloned()?;
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
            .any(|parameter| parameter.kind != CompileParamKind::Type)
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
            NominalKind::Struct => self.struct_templates[&target_name]
                .compile_groups
                .iter()
                .flatten()
                .count(),
            NominalKind::Enum => self.enum_templates[&target_name]
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
            },
            self_constructor: source.clone(),
        })
    }

    fn expand_function_aliases_after_substitution(
        &mut self,
        function: &mut Function,
        context: &str,
    ) -> bool {
        let aliases = self.type_aliases.clone();
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

    fn validate_associated_type_constructor(
        &mut self,
        trait_name: &str,
        associated: &str,
        source: &Type,
        expected_count: usize,
    ) -> bool {
        let Type::Named(name, arguments) = source else {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` must name a generic type constructor"
            ));
            return false;
        };

        let actual_count = if arguments.is_empty() {
            self.type_constructor_impl_target(source)
                .map(|target| target.parameter_count)
        } else {
            self.remaining_nominal_constructor_parameter_count(source)
                .or_else(|| self.remaining_type_alias_constructor_parameter_count(source))
        };
        let Some(actual_count) = actual_count else {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` must name a generic type constructor"
            ));
            return false;
        };
        if actual_count != expected_count {
            self.error(format!(
                "associated type constructor `{trait_name}.{associated}` expects {expected_count} type parameter{}, but `{name}` has {}",
                if expected_count == 1 { "" } else { "s" },
                actual_count
            ));
            return false;
        }
        true
    }

    fn remaining_type_alias_constructor_parameter_count(&self, source: &Type) -> Option<usize> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        let alias = self.type_aliases.get(name)?;
        let parameters = alias.compile_groups.iter().flatten().collect::<Vec<_>>();
        if parameters
            .iter()
            .any(|parameter| parameter.kind != CompileParamKind::Type)
            || arguments.len() >= parameters.len()
        {
            return None;
        }
        Some(parameters.len() - arguments.len())
    }

    fn remaining_nominal_constructor_parameter_count(&self, source: &Type) -> Option<usize> {
        let Type::Named(name, arguments) = source else {
            return None;
        };
        let total = self.type_constructor_impl_target(&Type::Named(name.clone(), Vec::new()))?;
        if arguments.is_empty() || arguments.len() >= total.parameter_count {
            return None;
        }
        Some(total.parameter_count - arguments.len())
    }

    fn trait_ref_has_constructor_subject(&self, source: &Type) -> bool {
        let Type::Named(name, _) = source else {
            return false;
        };
        self.traits.get(name).is_some_and(|schema| {
            matches!(
                schema.self_parameter.kind,
                CompileParamKind::TypeConstructor { .. }
            )
        })
    }

    fn resolve_trait_impl_ref(
        &mut self,
        source: &Type,
    ) -> Option<(TraitRefKey, TraitSchema, HashMap<String, Type>)> {
        let Type::Named(name, source_arguments) = source else {
            self.error("trait reference must name a trait");
            return None;
        };
        let Some(schema) = self.traits.get(name).cloned() else {
            self.error(format!("unknown trait `{name}`"));
            return None;
        };
        if !schema.valid {
            return None;
        }
        if schema.self_parameter.kind != CompileParamKind::Type {
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
        if source_arguments.iter().any(|argument| {
            !(self.source_type_is_concrete(argument)
                || self.instantiating_generic_trait_extension > 0
                    && self.source_type_is_abstract_or_concrete(argument))
        }) {
            self.error(format!(
                "generic trait implementation of `{name}` is not supported; trait arguments must be concrete"
            ));
            return None;
        }
        let mut arguments = Vec::new();
        let mut substitutions = HashMap::new();
        for (parameter, source_argument) in schema.compile_parameters.iter().zip(source_arguments) {
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

    fn normalize_trait_impl_associated_type(
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

    fn normalize_trait_impl_type(
        &mut self,
        trait_name: &str,
        source: &Type,
        raw: &HashMap<String, Type>,
        base_substitutions: &HashMap<String, Type>,
        normalized: &mut HashMap<String, Type>,
        visiting: &mut Vec<String>,
    ) -> Option<Type> {
        match source {
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
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {
                Some(source.clone())
            }
        }
    }

    fn function_shape(&mut self, function: &Function) -> Option<FunctionShape> {
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

    fn collect_trait_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
        if let Some(target) = self.type_constructor_impl_target(&extension.target) {
            if extension
                .trait_ref
                .as_ref()
                .is_some_and(|trait_ref| self.trait_ref_has_constructor_subject(trait_ref))
            {
                self.collect_constructor_trait_extension(extension, origin, target);
                return;
            }
        }
        let target_source = extension.target.clone();
        let Some(target) = self.resolve_trait_impl_target(&target_source) else {
            return;
        };
        let trait_source = extension
            .trait_ref
            .as_ref()
            .expect("trait extension has a trait reference");
        let Some((trait_ref, schema, mut substitutions)) =
            self.resolve_trait_impl_ref(trait_source)
        else {
            return;
        };
        let key = TraitImplKey {
            self_ty: target.clone(),
            trait_ref,
        };
        let is_copy = key.trait_ref.name == self.lang_item_name(LangItemKind::Copy);
        let is_drop = key.trait_ref.name == self.lang_item_name(LangItemKind::Drop);
        if (is_copy || is_drop)
            && self
                .nominal_accesses
                .get(nominal_name(&target).expect("trait targets are nominal"))
                .is_some_and(|access| access.origin.package != origin.package)
        {
            let target = self.diagnostic_type_name(&target);
            let trait_name = if is_copy { "Copy" } else { "Drop" };
            self.error(format!(
                "`{trait_name}` for `{target}` must be implemented in the package that defines the type"
            ));
            return;
        }
        let target_package = self
            .nominal_accesses
            .get(nominal_name(&target).expect("trait targets are nominal"))
            .map(|access| access.origin.package);
        if target_package != Some(origin.package) && schema.access.origin.package != origin.package
        {
            let target = self.diagnostic_type_name(&target);
            self.error(format!(
                "trait implementation of `{}` for `{target}` must be declared in the package that defines the trait or the type",
                key.trait_ref.name
            ));
            return;
        }
        if is_drop && self.copy_nominals.contains(&target) {
            let target = self.diagnostic_type_name(&target);
            self.error(format!(
                "`{target}` cannot implement both `Copy` and `Drop`"
            ));
            return;
        }
        if self.instantiating_generic_trait_extension == 0
            && self.concrete_trait_impl_overlaps_generic(&target, &key.trait_ref)
        {
            let target = self.diagnostic_type_name(&target);
            self.error(format!(
                "concrete trait implementation of `{}` for `{target}` overlaps a blanket generic implementation",
                key.trait_ref.name
            ));
            return;
        }
        let mut implementation_access =
            self.restrict_access_boundary_to_type(&schema.access, &target, &origin);
        for argument in &key.trait_ref.arguments {
            implementation_access =
                self.restrict_access_boundary_to_type(&implementation_access, argument, &origin);
        }
        if !self.trait_impl_headers.insert(key.clone()) {
            self.error(format!(
                "duplicate trait implementation of `{}` for `{target}`",
                key.trait_ref.name
            ));
            return;
        }
        substitutions.insert("Self".to_owned(), target_source);

        let mut raw_associated = HashMap::new();
        let mut supplied_methods = HashMap::new();
        let mut valid = true;
        for member in extension.members {
            match member {
                ExtendMember::Const(binding) => {
                    if !schema.associated_types.contains(&binding.name) {
                        self.error(format!(
                            "unknown trait member `{}.{}`",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                        continue;
                    }
                    if binding.annotation.is_some() {
                        self.error(format!(
                            "associated type `{}.{}` must not have a value annotation",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                    }
                    let Some(source) = self.type_argument_from_expr(&binding.value, &substitutions)
                    else {
                        valid = false;
                        continue;
                    };
                    if raw_associated
                        .insert(binding.name.clone(), source)
                        .is_some()
                    {
                        self.error(format!(
                            "duplicate associated type `{}.{}`",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                    }
                }
                ExtendMember::Function(function) => {
                    let method_name = function.name.clone();
                    let method_id = trait_method_identity(&schema, &function);
                    if method_id.is_none() {
                        self.error(format!(
                            "unknown trait member `{}.{}`",
                            key.trait_ref.name, method_name
                        ));
                        valid = false;
                        continue;
                    }
                    let method_id = method_id.expect("checked trait method identity");
                    if supplied_methods
                        .insert(method_id, function.clone())
                        .is_some()
                    {
                        self.error(format!(
                            "duplicate trait method `{}.{}`",
                            key.trait_ref.name, method_name
                        ));
                        valid = false;
                    }
                }
            }
        }

        for associated in &schema.associated_types {
            if !raw_associated.contains_key(associated) {
                self.error(format!(
                    "missing associated type `{}.{associated}` in trait implementation",
                    key.trait_ref.name
                ));
                valid = false;
            }
        }
        for method_id in &schema.method_order {
            let method = &schema.methods[method_id];
            if !supplied_methods.contains_key(method_id) && method.body.is_none() {
                self.error(format!(
                    "missing trait method `{}.{}` in implementation for `{target}`",
                    key.trait_ref.name, method.name
                ));
                valid = false;
            }
        }
        if !valid {
            return;
        }

        let mut normalized_sources = HashMap::new();
        for associated in &schema.associated_types {
            match schema.associated_type_kinds[associated] {
                CompileParamKind::Type => {
                    if self
                        .normalize_trait_impl_associated_type(
                            &key.trait_ref.name,
                            associated,
                            &raw_associated,
                            &substitutions,
                            &mut normalized_sources,
                            &mut Vec::new(),
                        )
                        .is_none()
                    {
                        valid = false;
                    }
                }
                CompileParamKind::TypeConstructor { .. } => {}
                CompileParamKind::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{}.{associated}` implementations are not supported yet",
                        key.trait_ref.name
                    ));
                    valid = false;
                }
                CompileParamKind::Region
                | CompileParamKind::Access
                | CompileParamKind::Passing
                | CompileParamKind::Effect => {
                    unreachable!("associated types only store type kinds")
                }
            }
        }
        if !valid {
            return;
        }
        let mut associated_types = HashMap::new();
        let mut associated_type_sources = HashMap::new();
        for (name, source) in &normalized_sources {
            let ty = self.lower_source_type(source);
            if ty == Ty::Error {
                valid = false;
            } else {
                associated_types.insert(name.clone(), ty);
                associated_type_sources.insert(name.clone(), source.clone());
                substitutions.insert(name.clone(), source.clone());
            }
        }
        for associated in &schema.associated_types {
            let CompileParamKind::TypeConstructor { parameter_count } =
                schema.associated_type_kinds[associated]
            else {
                continue;
            };
            let source = raw_associated
                .get(associated)
                .expect("missing associated constructors were diagnosed");
            if !self.validate_associated_type_constructor(
                &key.trait_ref.name,
                associated,
                source,
                parameter_count,
            ) {
                valid = false;
                continue;
            }
            associated_type_sources.insert(associated.clone(), source.clone());
            substitutions.insert(associated.clone(), source.clone());
        }
        if !valid {
            return;
        }

        let mut api_diagnostics = Vec::new();
        for (name, ty) in &associated_types {
            self.collect_type_api_leaks(
                ty,
                &implementation_access,
                &format!(
                    "trait implementation `{} for {target}` associated type `{name}`",
                    key.trait_ref.name
                ),
                &mut HashSet::new(),
                &mut api_diagnostics,
            );
        }
        for (name, source) in &associated_type_sources {
            if associated_types.contains_key(name) {
                continue;
            }
            let Type::Named(constructor, arguments) = source else {
                continue;
            };
            if !arguments.is_empty() {
                continue;
            }
            if let Some(referenced) = self.nominal_accesses.get(constructor) {
                if !Self::api_audience_is_contained(&implementation_access, referenced) {
                    let exposed_visibility = match implementation_access.visibility {
                        Visibility::Private => "private",
                        Visibility::Package => "pub(package)",
                        Visibility::Public => "public",
                    };
                    let referenced_visibility = match referenced.visibility {
                        Visibility::Private => "private",
                        Visibility::Package => "pub(package)",
                        Visibility::Public => "public",
                    };
                    api_diagnostics.push(format!(
                        "trait implementation `{} for {target}` associated type constructor `{name}` with {exposed_visibility} visibility exposes {referenced_visibility} type constructor `{constructor}` beyond its access boundary",
                        key.trait_ref.name
                    ));
                }
            }
        }
        api_diagnostics.sort();
        api_diagnostics.dedup();
        if !api_diagnostics.is_empty() {
            for diagnostic in api_diagnostics {
                self.error(diagnostic);
            }
            return;
        }

        let mut registered = Vec::new();
        for method_id in &schema.method_order {
            let declaration = &schema.methods[method_id];
            let method_name = declaration.name.clone();
            let mut expected = declaration.clone();
            substitute_function_types(&mut expected, &substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut expected,
                "trait expected signature",
            ) {
                valid = false;
                continue;
            }

            let (mut function, function_origin) = supplied_methods
                .get(method_id)
                .cloned()
                .map(|function| (function, origin.clone()))
                .unwrap_or_else(|| (declaration.clone(), schema.access.origin.clone()));
            if function.body.is_none() {
                self.error(format!(
                    "trait implementation method `{}.{method_name}` requires a body",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            if schema_function_has_receiver(declaration) != schema_function_has_receiver(&function)
            {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch: contextual `self` receiver does not match the trait declaration",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            substitute_function_types(&mut function, &substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut function,
                "trait implementation signature",
            ) {
                valid = false;
                continue;
            }
            if let Some(body) = &mut function.body {
                let target_name =
                    nominal_name(&target).expect("concrete trait implementation target is nominal");
                substitute_self_expression_target(body, target_name);
            }
            if !compile_parameter_groups_match(&expected.compile_groups, &function.compile_groups) {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch: compile-time parameter groups do not match the trait declaration",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            if function.compile_groups.is_empty() {
                let Some(expected_shape) = self.function_shape(&expected) else {
                    valid = false;
                    continue;
                };
                let Some(actual_shape) = self.function_shape(&function) else {
                    self.error(format!(
                        "trait method `{}.{method_name}` signature mismatch",
                        key.trait_ref.name
                    ));
                    valid = false;
                    continue;
                };
                if actual_shape != expected_shape {
                    self.error(format!(
                        "trait method `{}.{method_name}` signature mismatch: expected {expected_shape:?}, found {actual_shape:?}",
                        key.trait_ref.name
                    ));
                    valid = false;
                    continue;
                }
            } else if !source_function_shapes_match(&expected, &function) {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            let canonical = trait_method_name(&key, method_id);
            function.name = canonical.clone();
            registered.push((method_id.clone(), canonical, function, function_origin));
        }
        if !valid {
            return;
        }

        let mut methods = HashMap::new();
        for (method_id, canonical, function, function_origin) in registered {
            if function.compile_groups.is_empty() {
                let groups = function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| ParamSig {
                                name: parameter.name.clone(),
                                ty: self.lower_source_type(&parameter.ty),
                                mode: parameter.mode,
                            })
                            .collect()
                    })
                    .collect();
                let result = function
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result));
                let throws_error = function
                    .effects
                    .throws
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafe_effect: self.function_effects_unsafe(&function.effects),
                        throws_error,
                        custom_effects: self.function_effects_custom_identities(&function.effects),
                        result,
                    },
                );
                self.function_order.push(canonical.clone());
                self.functions.insert(canonical.clone(), function);
                self.function_origins
                    .insert(canonical.clone(), function_origin);
            } else {
                self.function_template_order.push(canonical.clone());
                self.function_templates.insert(canonical.clone(), function);
                self.function_template_origins
                    .insert(canonical.clone(), function_origin);
            }
            self.function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            self.function_type_substitutions
                .insert(canonical.clone(), substitutions.clone());
            methods.insert(method_id.clone(), canonical);
            let declaration = &schema.methods[&method_id];
            if schema_function_has_receiver(declaration) {
                let candidates = self
                    .trait_methods_by_receiver
                    .entry((target.clone(), declaration.name.clone()))
                    .or_default();
                if !candidates.contains(&key) {
                    candidates.push(key.clone());
                }
            }
        }
        self.trait_impls.insert(
            key.clone(),
            TraitImplInfo {
                key: key.clone(),
                associated_types,
                associated_type_sources,
                methods,
                access: implementation_access,
            },
        );
        if is_copy && self.copy_impls_finalized {
            self.validate_dynamic_copy_implementation(&key);
        }
    }

    fn collect_constructor_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        target: TypeConstructorImplTarget,
    ) {
        if !extension.where_predicates.is_empty() {
            self.error(format!(
                "constructor trait implementation for `{}` does not support `where` clauses yet",
                target.name
            ));
            return;
        }
        let trait_source = extension
            .trait_ref
            .as_ref()
            .expect("constructor trait extension has a trait reference");
        let Type::Named(trait_name, source_arguments) = trait_source else {
            self.error("constructor trait implementation must reference a named trait");
            return;
        };
        let Some(schema) = self.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid {
            return;
        }
        let CompileParamKind::TypeConstructor { parameter_count } = schema.self_parameter.kind
        else {
            self.error(format!(
                "trait `{trait_name}` does not accept a type-constructor implementation target"
            ));
            return;
        };
        if parameter_count != target.parameter_count {
            self.error(format!(
                "type constructor `{}` has {} parameter{}, but trait `{trait_name}` expects a constructor with {parameter_count}",
                target.name,
                target.parameter_count,
                if target.parameter_count == 1 { "" } else { "s" }
            ));
            return;
        }
        let expected_arguments = schema.compile_parameters.len();
        if source_arguments.len() != expected_arguments {
            self.error(format!(
                "trait argument count mismatch for `{trait_name}`: expected {expected_arguments}, found {}",
                source_arguments.len()
            ));
            return;
        }

        let mut trait_arguments = Vec::new();
        let mut trait_argument_sources = Vec::new();
        for (parameter, source_argument) in schema.compile_parameters.iter().zip(source_arguments) {
            if parameter.kind != CompileParamKind::Type {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` has unsupported compile-time kind {}",
                    parameter.name,
                    describe_compile_param_kind(parameter.kind)
                ));
                return;
            }
            if !self.source_type_is_concrete(source_argument) {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` must be a concrete type",
                    parameter.name
                ));
                return;
            }
            let argument = self.lower_source_type(source_argument);
            if argument == Ty::Error {
                return;
            }
            trait_argument_sources.push(source_argument.clone());
            trait_arguments.push(argument);
        }

        let target_package = self
            .nominal_accesses
            .get(&target.name)
            .map(|access| access.origin.package);
        if target_package != Some(origin.package) && schema.access.origin.package != origin.package
        {
            self.error(format!(
                "constructor trait implementation of `{trait_name}` for `{}` must be declared in the package that defines the trait or the type constructor",
                target.name
            ));
            return;
        }
        let key = ConstructorTraitImplKey {
            target,
            trait_ref: ConstructorTraitRefKey {
                name: trait_name.clone(),
                arguments: trait_arguments,
            },
        };
        if !self.constructor_trait_impl_headers.insert(key.clone()) {
            self.error(format!(
                "duplicate constructor trait implementation of `{}` for `{}`",
                key.trait_ref.name, key.target.name
            ));
            return;
        }
        if !schema.associated_types.is_empty() {
            self.error(format!(
                "constructor trait implementation of `{trait_name}` for `{}` does not support associated types yet",
                key.target.name
            ));
            return;
        }

        let mut substitutions = HashMap::new();
        substitutions.insert(
            "Self".to_owned(),
            Type::Named(key.target.name.clone(), Vec::new()),
        );
        for (parameter, argument) in schema.compile_parameters.iter().zip(trait_argument_sources) {
            substitutions.insert(parameter.name.clone(), argument);
        }

        let mut supplied_methods = HashMap::new();
        let mut valid = true;
        for member in extension.members {
            match member {
                ExtendMember::Const(binding) => {
                    self.error(format!(
                        "unknown constructor trait member `{}.{}`",
                        key.trait_ref.name, binding.name
                    ));
                    valid = false;
                }
                ExtendMember::Function(function) => {
                    let method_name = function.name.clone();
                    let Some(method_id) = trait_method_identity(&schema, &function) else {
                        self.error(format!(
                            "unknown constructor trait member `{}.{method_name}`",
                            key.trait_ref.name
                        ));
                        valid = false;
                        continue;
                    };
                    if supplied_methods.insert(method_id, function).is_some() {
                        self.error(format!(
                            "duplicate constructor trait method `{}.{method_name}`",
                            key.trait_ref.name
                        ));
                        valid = false;
                    }
                }
            }
        }

        let target_access = self.nominal_access_or_internal(&key.target.name);
        let mut implementation_access =
            Self::intersect_access_boundaries(&schema.access, &target_access, &origin);
        for argument in &key.trait_ref.arguments {
            implementation_access =
                self.restrict_access_boundary_to_type(&implementation_access, argument, &origin);
        }

        let mut registered_methods = HashMap::new();
        for method_id in &schema.method_order {
            let declaration = &schema.methods[method_id];
            let method_name = &declaration.name;
            let mut expected = declaration.clone();
            substitute_function_types(&mut expected, &substitutions);
            let (mut function, function_origin) = supplied_methods
                .get(method_id)
                .cloned()
                .map(|function| (function, origin.clone()))
                .unwrap_or_else(|| (declaration.clone(), schema.access.origin.clone()));
            if function.body.is_none() {
                self.error(format!(
                    "constructor trait method `{}.{method_name}` requires a body in implementation for `{}`",
                    key.trait_ref.name, key.target.name
                ));
                valid = false;
                continue;
            }
            substitute_function_types(&mut function, &substitutions);
            if schema_function_has_receiver(&expected) != schema_function_has_receiver(&function)
                || !compile_parameter_groups_match(
                    &expected.compile_groups,
                    &function.compile_groups,
                )
                || !source_function_shapes_match(&expected, &function)
            {
                self.error(format!(
                    "constructor trait method `{}.{method_name}` signature mismatch in implementation for `{}`",
                    key.trait_ref.name, key.target.name
                ));
                valid = false;
                continue;
            }
            let canonical = constructor_trait_method_name(&key, method_id);
            function.name = canonical.clone();
            if function.compile_groups.is_empty() {
                let groups = function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| ParamSig {
                                name: parameter.name.clone(),
                                ty: self.lower_source_type(&parameter.ty),
                                mode: parameter.mode,
                            })
                            .collect()
                    })
                    .collect();
                let result = function
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result));
                let throws_error = function
                    .effects
                    .throws
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafe_effect: self.function_effects_unsafe(&function.effects),
                        throws_error,
                        custom_effects: self.function_effects_custom_identities(&function.effects),
                        result,
                    },
                );
                self.function_order.push(canonical.clone());
                self.functions.insert(canonical.clone(), function);
                self.function_origins
                    .insert(canonical.clone(), function_origin);
            } else {
                self.function_template_order.push(canonical.clone());
                self.function_templates.insert(canonical.clone(), function);
                self.function_template_origins
                    .insert(canonical.clone(), function_origin);
            }
            self.function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            registered_methods.insert(method_id.clone(), canonical);
        }
        if !valid {
            return;
        }
        self.constructor_trait_impl_methods
            .insert(key, registered_methods);
    }

    fn collect_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
        if !extension.compile_groups.is_empty() {
            if extension.trait_ref.is_some() {
                if extension
                    .trait_ref
                    .as_ref()
                    .is_some_and(|trait_ref| self.trait_ref_has_constructor_subject(trait_ref))
                {
                    self.collect_generic_constructor_trait_extension(extension, origin);
                    return;
                }
                self.collect_generic_trait_extension(extension, origin);
            } else {
                self.collect_generic_inherent_extension(extension, origin);
            }
            return;
        }
        if extension.trait_ref.is_some() {
            self.collect_trait_extension(extension, origin);
            return;
        }
        let target = match extension.target {
            Type::Named(name, arguments) if arguments.is_empty() => name,
            Type::Named(name, _) => {
                self.error(format!(
                    "generic extend target `{name}` is not supported in M1"
                ));
                return;
            }
            _ => {
                self.error("extend target must be a non-generic nominal type in M1");
                return;
            }
        };
        if self.struct_templates.contains_key(&target) || self.enum_templates.contains_key(&target)
        {
            self.error(format!(
                "generic extend target `{target}` is not supported in the first generic slice"
            ));
            return;
        }
        if !self.struct_defs.contains_key(&target) && !self.enum_defs.contains_key(&target) {
            self.error(format!("unknown extension target `{target}`"));
            return;
        }
        if self
            .nominal_accesses
            .get(&target)
            .is_some_and(|access| access.origin.package != origin.package)
        {
            self.error(format!(
                "inherent extension for `{target}` must be declared in the package that defines the type"
            ));
            return;
        }
        let mut member_access = self.nominal_access_or_internal(&target);
        let target_ty = if self.struct_defs.contains_key(&target) {
            Ty::Struct(target.clone())
        } else {
            Ty::Enum(target.clone())
        };
        member_access = self.restrict_access_boundary_to_type(&member_access, &target_ty, &origin);

        for member in extension.members {
            match member {
                ExtendMember::Function(mut function) => {
                    let generic_member = !function.compile_groups.is_empty();
                    let short_name = function.name.clone();
                    let is_method = function
                        .groups
                        .first()
                        .is_some_and(|group| group.len() == 1 && group[0].name == "self");
                    let overload_key = (target.clone(), short_name.clone(), is_method);
                    let overloaded = self
                        .inherent_overload_counts
                        .get(&overload_key)
                        .copied()
                        .unwrap_or_default()
                        > 1;
                    if is_method
                        && self.struct_layouts.get(&target).is_some_and(|layout| {
                            layout.fields.iter().any(|field| field.name == short_name)
                        })
                    {
                        self.error(format!(
                            "inherent method `{target}.{short_name}` conflicts with field `{short_name}`"
                        ));
                        continue;
                    }
                    if !is_method
                        && self.enum_layouts.get(&target).is_some_and(|layout| {
                            layout
                                .variants
                                .iter()
                                .any(|variant| variant.name == short_name)
                        })
                    {
                        self.error(format!(
                            "associated function `{target}.{short_name}` conflicts with variant `{short_name}`"
                        ));
                        continue;
                    }

                    let duplicate = {
                        let members = self.inherent_members.entry(target.clone()).or_default();
                        if is_method {
                            members.methods.contains_key(&short_name)
                        } else {
                            members.functions.contains_key(&short_name)
                                || members.constants.contains_key(&short_name)
                        }
                    };
                    if duplicate && !overloaded {
                        self.error(if is_method {
                            format!("duplicate inherent method `{target}.{short_name}`")
                        } else {
                            format!("duplicate associated member `{target}.{short_name}`")
                        });
                        continue;
                    }

                    let overload_shape = if overloaded {
                        let shape = function_parameter_labels(&function);
                        if !self
                            .inherent_overload_shapes
                            .entry(overload_key.clone())
                            .or_default()
                            .insert(shape.clone())
                        {
                            self.error(if is_method {
                                format!(
                                    "duplicate inherent method overload `{target}.{short_name}` with parameter labels {}",
                                    display_parameter_label_shape(&shape)
                                )
                            } else {
                                format!(
                                    "duplicate associated member overload `{target}.{short_name}` with parameter labels {}",
                                    display_parameter_label_shape(&shape)
                                )
                            });
                            continue;
                        }
                        Some(shape)
                    } else {
                        None
                    };

                    let mut self_substitution = HashMap::new();
                    self_substitution
                        .insert("Self".to_owned(), Type::Named(target.clone(), Vec::new()));
                    substitute_function_types(&mut function, &self_substitution);
                    if let Some(body) = &mut function.body {
                        substitute_self_expression_target(body, &target);
                    }
                    let mut canonical = if is_method {
                        inherent_method_name(&target, &short_name)
                    } else {
                        associated_function_name(&target, &short_name)
                    };
                    if let Some(shape) = &overload_shape {
                        canonical = overloaded_function_name(&canonical, shape);
                        self.inherent_overloads
                            .entry(overload_key)
                            .or_default()
                            .push(canonical.clone());
                    }
                    function.name = canonical.clone();
                    if generic_member {
                        self.function_template_order.push(canonical.clone());
                        self.function_templates.insert(canonical.clone(), function);
                        self.function_template_origins
                            .insert(canonical.clone(), origin.clone());
                    } else {
                        let groups = function
                            .groups
                            .iter()
                            .map(|group| {
                                group
                                    .iter()
                                    .map(|parameter| ParamSig {
                                        name: parameter.name.clone(),
                                        ty: self.lower_source_type(&parameter.ty),
                                        mode: parameter.mode,
                                    })
                                    .collect()
                            })
                            .collect();
                        let result = function
                            .return_type
                            .as_ref()
                            .map(|result| self.lower_source_type(result));
                        let throws_error = function
                            .effects
                            .throws
                            .as_deref()
                            .map(|error| self.lower_source_type(error));
                        self.signatures.insert(
                            canonical.clone(),
                            FunctionSig {
                                groups,
                                unsafe_effect: self.function_effects_unsafe(&function.effects),
                                throws_error,
                                custom_effects: self
                                    .function_effects_custom_identities(&function.effects),
                                result,
                            },
                        );
                        self.function_order.push(canonical.clone());
                        self.functions.insert(canonical.clone(), function);
                        self.function_origins
                            .insert(canonical.clone(), origin.clone());
                    }
                    self.function_accesses
                        .insert(canonical.clone(), member_access.clone());
                    let members = self.inherent_members.entry(target.clone()).or_default();
                    if is_method {
                        members.methods.entry(short_name).or_insert(canonical);
                    } else {
                        members.functions.entry(short_name).or_insert(canonical);
                    }
                }
                ExtendMember::Const(mut binding) => {
                    let short_name = binding.name.clone();
                    if self.enum_layouts.get(&target).is_some_and(|layout| {
                        layout
                            .variants
                            .iter()
                            .any(|variant| variant.name == short_name)
                    }) {
                        self.error(format!(
                            "associated constant `{target}.{short_name}` conflicts with variant `{short_name}`"
                        ));
                        continue;
                    }
                    let duplicate = self
                        .inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .contains_key(&short_name)
                        || self
                            .inherent_members
                            .get(&target)
                            .is_some_and(|members| members.functions.contains_key(&short_name));
                    if duplicate {
                        self.error(format!(
                            "duplicate associated member `{target}.{short_name}`"
                        ));
                        continue;
                    }
                    if let Some(annotation) = &mut binding.annotation {
                        substitute_self_type(annotation, &target);
                    }
                    substitute_self_expression_target(&mut binding.value, &target);
                    let canonical = associated_constant_name(&target, &short_name);
                    binding.name = canonical.clone();
                    self.global_order.push(canonical.clone());
                    self.globals.insert(canonical.clone(), binding);
                    self.global_origins
                        .insert(canonical.clone(), origin.clone());
                    self.global_accesses
                        .insert(canonical.clone(), member_access.clone());
                    self.inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .insert(short_name, canonical);
                }
            }
        }
    }

    fn concrete_trait_impl_overlaps_generic(&self, target: &Ty, trait_ref: &TraitRefKey) -> bool {
        let Some(name) = nominal_name(target) else {
            return false;
        };
        let Some(instance) = self.nominal_instances.get(name) else {
            return false;
        };
        if instance.key.arguments.is_empty() {
            return false;
        }
        let Some(extensions) = self.generic_trait_extensions.get(&instance.key.template) else {
            return false;
        };
        let concrete_arguments = trait_ref
            .arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>();
        let target_arguments = instance
            .key
            .arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>();
        extensions.iter().any(|extension| {
            let Type::Named(name, arguments) = &extension.trait_ref else {
                return false;
            };
            if name != &trait_ref.name || arguments.len() != trait_ref.arguments.len() {
                return false;
            }
            concrete_arguments
                .as_ref()
                .zip(target_arguments.as_ref())
                .is_none_or(|(arguments, target_arguments)| {
                    generic_trait_pattern_overlaps_concrete(
                        extension,
                        trait_ref,
                        arguments,
                        target_arguments,
                    )
                })
        })
    }

    fn generic_trait_extension_overlaps_concrete(
        &self,
        target_template: &str,
        extension: &GenericTraitExtension,
    ) -> bool {
        self.trait_impls.keys().any(|key| {
            let Some(name) = nominal_name(&key.self_ty) else {
                return false;
            };
            let Some(instance) = self.nominal_instances.get(name) else {
                return false;
            };
            if instance.key.template != target_template || instance.key.arguments.is_empty() {
                return false;
            }
            let Type::Named(name, arguments) = &extension.trait_ref else {
                return false;
            };
            if name != &key.trait_ref.name || arguments.len() != key.trait_ref.arguments.len() {
                return false;
            }
            let trait_arguments = key
                .trait_ref
                .arguments
                .iter()
                .map(|argument| self.source_type_for_ty(argument))
                .collect::<Option<Vec<_>>>();
            let target_arguments = instance
                .key
                .arguments
                .iter()
                .map(|argument| self.source_type_for_ty(argument))
                .collect::<Option<Vec<_>>>();
            trait_arguments
                .as_ref()
                .zip(target_arguments.as_ref())
                .is_none_or(|(arguments, target_arguments)| {
                    generic_trait_pattern_overlaps_concrete(
                        extension,
                        &key.trait_ref,
                        arguments,
                        target_arguments,
                    )
                })
        })
    }

    fn collect_generic_trait_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
        let compile_parameter_kinds = compile_parameter_kinds(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic trait extension",
            &extension.where_predicates,
            &compile_parameter_kinds,
        ) {
            return;
        }
        let parameters = extension
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if extension.compile_groups.len() != 1 || parameters.is_empty() {
            self.error("generic trait extend requires exactly one non-empty type parameter group");
            return;
        }
        let mut declared = HashSet::new();
        for parameter in &parameters {
            if parameter.name == "Self" || !declared.insert(parameter.name.clone()) {
                self.error(format!(
                    "invalid or duplicate generic extend parameter `{}`",
                    parameter.name
                ));
                return;
            }
        }
        let Type::Named(target_template, target_sources) = &extension.target else {
            self.error("generic trait extend target must be a generic nominal type");
            return;
        };
        let expected = self
            .struct_templates
            .get(target_template)
            .map(|definition| definition.compile_groups.iter().flatten().count())
            .or_else(|| {
                self.enum_templates
                    .get(target_template)
                    .map(|definition| definition.compile_groups.iter().flatten().count())
            });
        let Some(expected) = expected else {
            self.error(format!(
                "generic trait extend target `{target_template}` is not a generic nominal type"
            ));
            return;
        };
        if target_sources.len() != expected {
            self.error(format!(
                "generic extend target `{target_template}` expects {expected} type arguments, found {}",
                target_sources.len()
            ));
            return;
        }
        let mut target_arguments = Vec::new();
        let mut determined = HashSet::new();
        for source in target_sources {
            let Type::Named(name, arguments) = source else {
                self.error(
                    "generic trait extend target arguments must be bare declared type parameters",
                );
                return;
            };
            if !arguments.is_empty() || !declared.contains(name) || !determined.insert(name.clone())
            {
                self.error(
                    "generic trait extend target arguments must use every declared type parameter exactly once",
                );
                return;
            }
            target_arguments.push(name.clone());
        }
        if determined.len() != parameters.len() {
            self.error(
                "every generic trait extend parameter must be determined by the target type",
            );
            return;
        }

        let trait_ref = extension
            .trait_ref
            .as_ref()
            .expect("generic trait extension has a trait reference");
        let Type::Named(trait_name, trait_arguments) = trait_ref else {
            self.error("generic trait extension must reference a named trait");
            return;
        };
        let Some(schema) = self.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid {
            return;
        }
        if trait_arguments.len() != schema.compile_parameters.len() {
            self.error(format!(
                "trait argument count mismatch for `{trait_name}`: expected {}, found {}",
                schema.compile_parameters.len(),
                trait_arguments.len()
            ));
            return;
        }
        if !self.validate_generic_trait_members(trait_name, &schema, &extension.members) {
            return;
        }
        if !self.validate_generic_trait_method_shapes(
            trait_name,
            &schema,
            trait_arguments,
            &extension,
        ) {
            return;
        }
        let is_copy = trait_name == self.lang_item_name(LangItemKind::Copy);
        let is_drop = trait_name == self.lang_item_name(LangItemKind::Drop);
        let target_package = self
            .nominal_accesses
            .get(target_template)
            .map(|access| access.origin.package);
        if (is_copy || is_drop) && target_package != Some(origin.package) {
            let trait_name = if is_copy { "Copy" } else { "Drop" };
            self.error(format!(
                "generic `{trait_name}` for `{target_template}` must be implemented in the package that defines the type"
            ));
            return;
        }
        if target_package != Some(origin.package) && schema.access.origin.package != origin.package
        {
            self.error(format!(
                "generic trait implementation of `{trait_name}` for `{target_template}` must be declared in the package that defines the trait or the type"
            ));
            return;
        }
        let template = GenericTraitExtension {
            target_arguments,
            trait_ref: trait_ref.clone(),
            where_predicates: extension.where_predicates.clone(),
            members: extension.members.clone(),
            origin: origin.clone(),
        };
        if self
            .generic_trait_extensions
            .get(target_template)
            .is_some_and(|extensions| {
                extensions
                    .iter()
                    .any(|existing| trait_reference_patterns_overlap(existing, &template))
            })
        {
            self.error(format!(
                "overlapping generic trait implementation of `{trait_name}` for `{target_template}`"
            ));
            return;
        }
        if self.generic_trait_extension_overlaps_concrete(target_template, &template) {
            self.error(format!(
                "generic trait implementation of `{trait_name}` for `{target_template}` overlaps an existing concrete implementation"
            ));
            return;
        }
        self.generic_trait_extensions
            .entry(target_template.clone())
            .or_default()
            .push(template.clone());
        self.register_generic_trait_validation_templates(
            target_template,
            trait_name,
            &extension,
            &schema.access,
            &origin,
        );

        if is_copy {
            return;
        }

        let existing = self
            .nominal_instances
            .iter()
            .filter(|(_, instance)| instance.key.template == *target_template)
            .map(|(canonical, instance)| (canonical.clone(), instance.key.arguments.clone()))
            .collect::<Vec<_>>();
        for (canonical, arguments) in existing {
            let Some(source_arguments) = arguments
                .iter()
                .map(|argument| self.source_type_for_ty(argument))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            self.instantiate_generic_trait_extension(
                target_template,
                &canonical,
                &source_arguments,
                &template,
            );
        }
    }

    fn collect_generic_constructor_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
    ) {
        let compile_parameter_kinds = compile_parameter_kinds(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic constructor trait extension",
            &extension.where_predicates,
            &compile_parameter_kinds,
        ) {
            return;
        }
        if !extension.where_predicates.is_empty() {
            self.error(
                "generic constructor trait implementation does not support `where` clauses yet",
            );
            return;
        }
        let parameters = extension
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if extension.compile_groups.len() != 1 || parameters.is_empty() {
            self.error(
                "generic constructor trait extend requires exactly one non-empty type parameter group",
            );
            return;
        }
        let mut declared = HashSet::new();
        for parameter in &parameters {
            if parameter.kind != CompileParamKind::Type
                || parameter.name == "Self"
                || !declared.insert(parameter.name.clone())
            {
                self.error(format!(
                    "invalid or duplicate generic constructor extend parameter `{}`",
                    parameter.name
                ));
                return;
            }
        }
        let Some(target) = self
            .partial_nominal_constructor_trait_target(&extension.target, &parameters)
            .or_else(|| {
                self.partial_alias_constructor_trait_target(&extension.target, &parameters)
            })
        else {
            self.error(
                "generic constructor trait extend target must be a partially applied nominal constructor or transparent type alias",
            );
            return;
        };
        let trait_source = extension
            .trait_ref
            .as_ref()
            .expect("generic constructor trait extension has a trait reference");
        let Type::Named(trait_name, source_arguments) = trait_source else {
            self.error("generic constructor trait implementation must reference a named trait");
            return;
        };
        let Some(schema) = self.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid {
            return;
        }
        let CompileParamKind::TypeConstructor { parameter_count } = schema.self_parameter.kind
        else {
            self.error(format!(
                "trait `{trait_name}` does not accept a type-constructor implementation target"
            ));
            return;
        };
        if parameter_count != target.target.parameter_count {
            self.error(format!(
                "type constructor `{}` has {} parameter{}, but trait `{trait_name}` expects a constructor with {parameter_count}",
                target.target.name,
                target.target.parameter_count,
                if target.target.parameter_count == 1 { "" } else { "s" }
            ));
            return;
        }
        if source_arguments.len() != schema.compile_parameters.len() {
            self.error(format!(
                "trait argument count mismatch for `{trait_name}`: expected {}, found {}",
                schema.compile_parameters.len(),
                source_arguments.len()
            ));
            return;
        }
        for parameter in &schema.compile_parameters {
            if parameter.kind != CompileParamKind::Type {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` has unsupported compile-time kind {}",
                    parameter.name,
                    describe_compile_param_kind(parameter.kind)
                ));
                return;
            }
        }
        if !self.validate_generic_trait_members(trait_name, &schema, &extension.members) {
            return;
        }

        let target_package = self
            .nominal_accesses
            .get(&target.target.name)
            .map(|access| access.origin.package);
        if target_package != Some(origin.package) && schema.access.origin.package != origin.package
        {
            self.error(format!(
                "generic constructor trait implementation of `{trait_name}` for `{}` must be declared in the package that defines the trait or the type constructor",
                target.target.name
            ));
            return;
        }

        let mut trait_arguments = Vec::new();
        let mut trait_argument_sources = Vec::new();
        for (parameter, source_argument) in schema.compile_parameters.iter().zip(source_arguments) {
            if !self.source_type_is_concrete(source_argument) {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` must be a concrete type",
                    parameter.name
                ));
                return;
            }
            let argument = self.lower_source_type(source_argument);
            if argument == Ty::Error {
                return;
            }
            trait_argument_sources.push(source_argument.clone());
            trait_arguments.push(argument);
        }

        let key = ConstructorTraitImplKey {
            target: target.target.clone(),
            trait_ref: ConstructorTraitRefKey {
                name: trait_name.clone(),
                arguments: trait_arguments,
            },
        };
        if !self.constructor_trait_impl_headers.insert(key.clone()) {
            self.error(format!(
                "duplicate constructor trait implementation of `{}` for `{}`",
                key.trait_ref.name, key.target.name
            ));
            return;
        }
        if !schema.associated_types.is_empty() {
            self.error(format!(
                "constructor trait implementation of `{trait_name}` for `{}` does not support associated types yet",
                key.target.name
            ));
            return;
        }

        let mut substitutions = HashMap::new();
        substitutions.insert("Self".to_owned(), target.self_constructor.clone());
        for (parameter, argument) in schema.compile_parameters.iter().zip(trait_argument_sources) {
            substitutions.insert(parameter.name.clone(), argument);
        }

        let mut supplied_methods = HashMap::new();
        let mut valid = true;
        for member in extension.members {
            match member {
                ExtendMember::Const(binding) => {
                    self.error(format!(
                        "unknown constructor trait member `{}.{}`",
                        key.trait_ref.name, binding.name
                    ));
                    valid = false;
                }
                ExtendMember::Function(function) => {
                    let method_name = function.name.clone();
                    let Some(method_id) = trait_method_identity(&schema, &function) else {
                        self.error(format!(
                            "unknown constructor trait member `{}.{method_name}`",
                            key.trait_ref.name
                        ));
                        valid = false;
                        continue;
                    };
                    if supplied_methods.insert(method_id, function).is_some() {
                        self.error(format!(
                            "duplicate constructor trait method `{}.{method_name}`",
                            key.trait_ref.name
                        ));
                        valid = false;
                    }
                }
            }
        }

        let target_access = self.nominal_access_or_internal(&key.target.name);
        let mut implementation_access =
            Self::intersect_access_boundaries(&schema.access, &target_access, &origin);
        for argument in &key.trait_ref.arguments {
            implementation_access =
                self.restrict_access_boundary_to_type(&implementation_access, argument, &origin);
        }

        let mut registered_methods = HashMap::new();
        for method_id in &schema.method_order {
            let declaration = &schema.methods[method_id];
            let method_name = &declaration.name;
            let mut expected = declaration.clone();
            substitute_function_types(&mut expected, &substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut expected,
                "constructor trait expected signature",
            ) {
                valid = false;
                continue;
            }
            let (mut function, function_origin) = supplied_methods
                .get(method_id)
                .cloned()
                .map(|function| (function, origin.clone()))
                .unwrap_or_else(|| (declaration.clone(), schema.access.origin.clone()));
            if function.body.is_none() {
                self.error(format!(
                    "constructor trait method `{}.{method_name}` requires a body in implementation for `{}`",
                    key.trait_ref.name, key.target.name
                ));
                valid = false;
                continue;
            }
            substitute_function_types(&mut function, &substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut function,
                "constructor trait implementation signature",
            ) {
                valid = false;
                continue;
            }
            if schema_function_has_receiver(&expected) != schema_function_has_receiver(&function)
                || !compile_parameter_groups_match(
                    &expected.compile_groups,
                    &function.compile_groups,
                )
                || !source_function_shapes_match(&expected, &function)
            {
                self.error(format!(
                    "constructor trait method `{}.{method_name}` signature mismatch in implementation for `{}`",
                    key.trait_ref.name, key.target.name
                ));
                valid = false;
                continue;
            }
            let canonical = constructor_trait_method_name(&key, method_id);
            function.name = canonical.clone();
            let mut compile_groups = extension.compile_groups.clone();
            compile_groups.extend(function.compile_groups.clone());
            function.compile_groups = compile_groups;
            function.where_predicates = extension.where_predicates.clone();
            self.function_template_order.push(canonical.clone());
            self.function_templates.insert(canonical.clone(), function);
            self.function_template_origins
                .insert(canonical.clone(), function_origin);
            self.function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            registered_methods.insert(method_id.clone(), canonical);
        }
        if !valid {
            return;
        }
        self.constructor_trait_impl_methods
            .insert(key, registered_methods);
    }

    fn validate_generic_trait_members(
        &mut self,
        trait_name: &str,
        schema: &TraitSchema,
        members: &[ExtendMember],
    ) -> bool {
        let mut associated = HashSet::new();
        let mut methods = HashSet::new();
        let mut valid = true;
        for member in members {
            match member {
                ExtendMember::Const(binding) => {
                    if !schema.associated_types.contains(&binding.name) {
                        self.error(format!(
                            "unknown trait member `{trait_name}.{}`",
                            binding.name
                        ));
                        valid = false;
                    } else if matches!(
                        schema.associated_type_kinds[&binding.name],
                        CompileParamKind::EffectConstructor { .. }
                    ) {
                        self.error(format!(
                            "effect associated constructor `{trait_name}.{}` implementations are not supported yet",
                            binding.name
                        ));
                        valid = false;
                    } else if !associated.insert(binding.name.clone()) {
                        self.error(format!(
                            "duplicate associated type `{trait_name}.{}`",
                            binding.name
                        ));
                        valid = false;
                    }
                    if binding.annotation.is_some() {
                        self.error(format!(
                            "associated type `{trait_name}.{}` must not have a value annotation",
                            binding.name
                        ));
                        valid = false;
                    }
                }
                ExtendMember::Function(function) => {
                    let Some(method_id) = trait_method_identity(schema, function) else {
                        self.error(format!(
                            "unknown trait member `{trait_name}.{}`",
                            function.name
                        ));
                        valid = false;
                        continue;
                    };
                    if !methods.insert(method_id) {
                        self.error(format!(
                            "duplicate trait method `{trait_name}.{}`",
                            function.name
                        ));
                        valid = false;
                    }
                    if function.body.is_none() {
                        self.error(format!(
                            "trait implementation method `{trait_name}.{}` requires a body",
                            function.name
                        ));
                        valid = false;
                    }
                }
            }
        }
        for name in &schema.associated_types {
            match schema.associated_type_kinds[name] {
                CompileParamKind::Type | CompileParamKind::TypeConstructor { .. } => {}
                CompileParamKind::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{trait_name}.{name}` implementations are not supported yet"
                    ));
                    valid = false;
                    continue;
                }
                CompileParamKind::Region
                | CompileParamKind::Access
                | CompileParamKind::Passing
                | CompileParamKind::Effect => {
                    unreachable!("associated types only store type kinds")
                }
            }
            if !associated.contains(name) {
                self.error(format!(
                    "missing associated type `{trait_name}.{name}` in generic trait implementation"
                ));
                valid = false;
            }
        }
        for method_id in &schema.method_order {
            let declaration = &schema.methods[method_id];
            if !methods.contains(method_id) && declaration.body.is_none() {
                self.error(format!(
                    "missing trait method `{trait_name}.{}` in generic trait implementation",
                    declaration.name
                ));
                valid = false;
            }
        }
        valid
    }

    fn validate_generic_trait_method_shapes(
        &mut self,
        trait_name: &str,
        schema: &TraitSchema,
        trait_arguments: &[Type],
        extension: &ExtendDef,
    ) -> bool {
        let mut expected_substitutions = schema
            .compile_parameters
            .iter()
            .zip(trait_arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        expected_substitutions.insert("Self".to_owned(), extension.target.clone());
        let mut raw_associated = HashMap::new();
        let mut valid = true;
        for member in &extension.members {
            let ExtendMember::Const(binding) = member else {
                continue;
            };
            if matches!(
                schema.associated_type_kinds.get(&binding.name),
                Some(CompileParamKind::EffectConstructor { .. })
            ) {
                self.error(format!(
                    "effect associated constructor `{trait_name}.{}` implementations are not supported yet",
                    binding.name
                ));
                valid = false;
                continue;
            }
            let Some(source) =
                self.type_argument_from_expr(&binding.value, &expected_substitutions)
            else {
                valid = false;
                continue;
            };
            raw_associated.insert(binding.name.clone(), source);
        }
        let mut normalized = HashMap::new();
        for associated in &schema.associated_types {
            match schema.associated_type_kinds[associated] {
                CompileParamKind::Type => {
                    if let Some(source) = self.normalize_trait_impl_associated_type(
                        trait_name,
                        associated,
                        &raw_associated,
                        &expected_substitutions,
                        &mut normalized,
                        &mut Vec::new(),
                    ) {
                        expected_substitutions.insert(associated.clone(), source);
                    } else {
                        valid = false;
                    }
                }
                CompileParamKind::TypeConstructor { parameter_count } => {
                    let Some(source) = raw_associated.get(associated) else {
                        valid = false;
                        continue;
                    };
                    if self.validate_associated_type_constructor(
                        trait_name,
                        associated,
                        source,
                        parameter_count,
                    ) {
                        expected_substitutions.insert(associated.clone(), source.clone());
                    } else {
                        valid = false;
                    }
                }
                CompileParamKind::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{trait_name}.{associated}` implementations are not supported yet"
                    ));
                    valid = false;
                }
                CompileParamKind::Region
                | CompileParamKind::Access
                | CompileParamKind::Passing
                | CompileParamKind::Effect => {
                    unreachable!("associated types only store type kinds")
                }
            }
        }
        let mut actual_self = HashMap::new();
        actual_self.insert("Self".to_owned(), extension.target.clone());
        for method_id in &schema.method_order {
            let Some(ExtendMember::Function(actual)) = extension.members.iter().find(|member| {
                matches!(member, ExtendMember::Function(function)
                    if trait_method_identity(schema, function).as_ref() == Some(method_id))
            }) else {
                continue;
            };
            let declaration = &schema.methods[method_id];
            let method_name = &declaration.name;
            let mut expected = declaration.clone();
            substitute_function_types(&mut expected, &expected_substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut expected,
                "generic trait expected signature",
            ) {
                valid = false;
                continue;
            }
            let mut actual = actual.clone();
            if !compile_parameter_groups_match(&expected.compile_groups, &actual.compile_groups) {
                self.error(format!(
                    "trait method `{trait_name}.{method_name}` signature mismatch in generic implementation: compile-time parameter groups do not match the trait declaration"
                ));
                valid = false;
                continue;
            }
            if schema_function_has_receiver(&expected) != schema_function_has_receiver(&actual) {
                self.error(format!(
                    "trait method `{trait_name}.{method_name}` signature mismatch in generic implementation"
                ));
                valid = false;
                continue;
            }
            substitute_function_types(&mut actual, &actual_self);
            if !self.expand_function_aliases_after_substitution(
                &mut actual,
                "generic trait implementation signature",
            ) {
                valid = false;
                continue;
            }
            if !source_function_shapes_match(&expected, &actual) {
                self.error(format!(
                    "trait method `{trait_name}.{method_name}` signature mismatch in generic implementation"
                ));
                valid = false;
            }
        }
        valid
    }

    fn instantiate_generic_trait_extensions_for_instance(
        &mut self,
        target_template: &str,
        canonical: &str,
        source_arguments: &[Type],
    ) {
        if self.suppress_generic_inherent_instantiation != 0 {
            return;
        }
        if !source_arguments
            .iter()
            .all(|source| self.source_type_is_concrete(source))
        {
            return;
        }
        let extensions = self
            .generic_trait_extensions
            .get(target_template)
            .cloned()
            .unwrap_or_default();
        for extension in &extensions {
            if self.generic_trait_extension_impl_exists(canonical, source_arguments, extension) {
                continue;
            }
            self.instantiate_generic_trait_extension(
                target_template,
                canonical,
                source_arguments,
                extension,
            );
        }
    }

    fn generic_trait_extension_impl_exists(
        &mut self,
        canonical: &str,
        source_arguments: &[Type],
        extension: &GenericTraitExtension,
    ) -> bool {
        let Some(instance) = self.nominal_instances.get(canonical).cloned() else {
            return false;
        };
        if source_arguments.len() != extension.target_arguments.len() {
            return false;
        }
        let substitutions = extension
            .target_arguments
            .iter()
            .cloned()
            .zip(source_arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let mut trait_ref = extension.trait_ref.clone();
        substitute_type_parameters(&mut trait_ref, &substitutions);
        let Some((trait_ref, _, _)) = self.resolve_trait_impl_ref(&trait_ref) else {
            return false;
        };
        let self_ty = match instance.key.kind {
            NominalKind::Struct => Ty::Struct(canonical.to_owned()),
            NominalKind::Enum => Ty::Enum(canonical.to_owned()),
        };
        let key = TraitImplKey { self_ty, trait_ref };
        self.trait_impl_headers.contains(&key) || self.trait_impls.contains_key(&key)
    }

    fn register_generic_trait_validation_templates(
        &mut self,
        target_template: &str,
        trait_name: &str,
        extension: &ExtendDef,
        access: &AccessBoundary,
        origin: &ItemOrigin,
    ) {
        // Edition-pinned core extensions are validated as part of the core
        // bundle and again whenever a concrete instance materializes. Keeping
        // an abstract user-program validation template would let unrelated
        // user declarations shadow short core enum variants in those bodies.
        if origin.package == PackageId::CORE.0 {
            return;
        }
        let mut self_substitution = HashMap::new();
        self_substitution.insert("Self".to_owned(), extension.target.clone());
        let target_access = self.nominal_access_or_internal(target_template);
        let validation_access = Self::intersect_access_boundaries(access, &target_access, origin);
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                continue;
            };
            let canonical = format!(
                "$generic$trait$validation${target_template}${trait_name}${}${}",
                function.name,
                self.function_template_order.len()
            );
            let mut template = function.clone();
            template.name = canonical.clone();
            template.compile_groups = extension.compile_groups.clone();
            template
                .compile_groups
                .extend(function.compile_groups.clone());
            template.where_predicates = extension.where_predicates.clone();
            template
                .where_predicates
                .extend(function.where_predicates.clone());
            substitute_function_types(&mut template, &self_substitution);
            if let Some(body) = &mut template.body {
                substitute_self_expression_target(body, target_template);
            }
            self.function_template_order.push(canonical.clone());
            self.function_templates.insert(canonical.clone(), template);
            self.function_template_origins
                .insert(canonical.clone(), origin.clone());
            self.function_accesses
                .insert(canonical, validation_access.clone());
        }
    }

    fn instantiate_generic_trait_extension(
        &mut self,
        target_template: &str,
        canonical: &str,
        source_arguments: &[Type],
        extension: &GenericTraitExtension,
    ) {
        if source_arguments.len() != extension.target_arguments.len() {
            self.error(format!(
                "internal error: invalid generic trait extension arguments for `{target_template}`"
            ));
            return;
        }
        let substitutions = extension
            .target_arguments
            .iter()
            .cloned()
            .zip(source_arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let mut predicates = extension.where_predicates.clone();
        for predicate in &mut predicates {
            substitute_type_parameters(&mut predicate.subject, &substitutions);
            substitute_type_parameters(&mut predicate.trait_ref, &substitutions);
            for binding in &mut predicate.associated_types {
                substitute_type_parameters(&mut binding.ty, &substitutions);
            }
        }
        if predicates
            .iter()
            .any(|predicate| !self.concrete_where_predicate_holds(predicate))
        {
            return;
        }
        let mut trait_ref = extension.trait_ref.clone();
        substitute_type_parameters(&mut trait_ref, &substitutions);
        self.instantiating_generic_trait_extension += 1;
        let already_registered = self
            .instantiated_generic_trait_key(canonical, &trait_ref)
            .is_some_and(|key| self.trait_impl_headers.contains(&key));
        if already_registered {
            self.instantiating_generic_trait_extension -= 1;
            return;
        }
        let mut members = extension.members.clone();
        for member in &mut members {
            match member {
                ExtendMember::Function(function) => {
                    substitute_function_types(function, &substitutions)
                }
                ExtendMember::Const(binding) => {
                    if let Some(annotation) = &mut binding.annotation {
                        substitute_type_parameters(annotation, &substitutions);
                    }
                    substitute_type_expression_parameters(&mut binding.value, &substitutions);
                }
            }
        }
        self.collect_trait_extension(
            ExtendDef {
                compile_groups: Vec::new(),
                target: Type::Named(canonical.to_owned(), Vec::new()),
                trait_ref: Some(trait_ref),
                where_predicates: Vec::new(),
                members,
            },
            extension.origin.clone(),
        );
        self.instantiating_generic_trait_extension -= 1;
    }

    fn instantiated_generic_trait_key(
        &mut self,
        canonical: &str,
        trait_ref: &Type,
    ) -> Option<TraitImplKey> {
        let instance = self.nominal_instances.get(canonical).cloned()?;
        let (trait_ref, _, _) = self.resolve_trait_impl_ref(trait_ref)?;
        let self_ty = match instance.key.kind {
            NominalKind::Struct => Ty::Struct(canonical.to_owned()),
            NominalKind::Enum => Ty::Enum(canonical.to_owned()),
        };
        Some(TraitImplKey { self_ty, trait_ref })
    }

    fn collect_generic_inherent_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
        let compile_parameter_kinds = compile_parameter_kinds(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic inherent extension",
            &extension.where_predicates,
            &compile_parameter_kinds,
        ) {
            return;
        }
        let parameters = extension
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if extension.compile_groups.len() != 1 || parameters.is_empty() {
            self.error(
                "generic inherent extend requires exactly one non-empty type parameter group",
            );
            return;
        }
        let mut declared = HashSet::new();
        for parameter in &parameters {
            if parameter.name == "Self" || !declared.insert(parameter.name.clone()) {
                self.error(format!(
                    "invalid or duplicate generic extend parameter `{}`",
                    parameter.name
                ));
                return;
            }
        }
        let Type::Named(target_template, target_sources) = &extension.target else {
            self.error("generic inherent extend target must be a generic nominal type");
            return;
        };
        let expected = self
            .struct_templates
            .get(target_template)
            .map(|definition| definition.compile_groups.iter().flatten().count())
            .or_else(|| {
                self.enum_templates
                    .get(target_template)
                    .map(|definition| definition.compile_groups.iter().flatten().count())
            });
        let Some(expected) = expected else {
            self.error(format!(
                "generic inherent extend target `{target_template}` is not a generic nominal type"
            ));
            return;
        };
        if self
            .nominal_accesses
            .get(target_template)
            .is_some_and(|access| access.origin.package != origin.package)
        {
            self.error(format!(
                "generic inherent extension for `{target_template}` must be declared in the package that defines the type"
            ));
            return;
        }
        if target_sources.len() != expected {
            self.error(format!(
                "generic extend target `{target_template}` expects {expected} type arguments, found {}",
                target_sources.len()
            ));
            return;
        }
        let mut target_arguments = Vec::new();
        let mut determined = HashSet::new();
        for source in target_sources {
            let Type::Named(name, arguments) = source else {
                self.error(
                    "generic inherent extend target arguments must be bare declared type parameters in the first version",
                );
                return;
            };
            if !arguments.is_empty() || !declared.contains(name) || !determined.insert(name.clone())
            {
                self.error(
                    "generic inherent extend target arguments must use every declared type parameter exactly once",
                );
                return;
            }
            target_arguments.push(name.clone());
        }
        if determined.len() != parameters.len() {
            self.error(
                "every generic inherent extend parameter must be determined by the target type",
            );
            return;
        }

        let mut extension_access = self.nominal_access_or_internal(target_template);
        for predicate in &extension.where_predicates {
            if let Type::Named(trait_name, _) = &predicate.trait_ref {
                if let Some(schema) = self.traits.get(trait_name) {
                    extension_access = Self::intersect_access_boundaries(
                        &extension_access,
                        &schema.access,
                        &origin,
                    );
                }
            }
            for binding in &predicate.associated_types {
                if self.source_type_is_concrete(&binding.ty) {
                    let ty = self.lower_source_type(&binding.ty);
                    extension_access =
                        self.restrict_access_boundary_to_type(&extension_access, &ty, &origin);
                }
            }
        }

        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                self.error("generic inherent associated constants are not supported yet");
                return;
            };
            let outer_names = parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<HashSet<_>>();
            if let Some(parameter) = function
                .compile_groups
                .iter()
                .flatten()
                .find(|parameter| outer_names.contains(parameter.name.as_str()))
            {
                self.error(format!(
                    "generic inherent member `{target_template}.{}` redeclares outer compile-time parameter `{}`",
                    function.name, parameter.name
                ));
                return;
            }
            let is_method = function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self");
            let overload_key = (target_template.clone(), function.name.clone(), is_method);
            let overloaded = self
                .inherent_overload_counts
                .get(&overload_key)
                .copied()
                .unwrap_or_default()
                > 1;
            if overloaded {
                let shape = function_parameter_labels(function);
                if !self
                    .inherent_overload_shapes
                    .entry(overload_key)
                    .or_default()
                    .insert(shape.clone())
                {
                    self.error(format!(
                        "duplicate generic inherent overload `{target_template}.{}` with parameter labels {}",
                        function.name,
                        display_parameter_label_shape(&shape)
                    ));
                    return;
                }
            }
            if self
                .generic_inherent_extensions
                .get(target_template)
                .is_some_and(|extensions| {
                    extensions.iter().any(|existing| {
                        existing.members.iter().any(|member| {
                            let ExtendMember::Function(existing) = member else {
                                return false;
                            };
                            existing.name == function.name
                                && existing.groups.first().is_some_and(|group| {
                                    group.len() == 1 && group[0].name == "self"
                                }) == is_method
                        })
                    })
                })
            {
                if overloaded {
                    continue;
                }
                self.error(if is_method {
                    format!(
                        "duplicate generic inherent method `{target_template}.{}`",
                        function.name
                    )
                } else {
                    format!(
                        "duplicate generic associated function `{target_template}.{}`",
                        function.name
                    )
                });
                return;
            }
        }

        let template = GenericInherentExtension {
            target_arguments,
            where_predicates: extension.where_predicates.clone(),
            members: extension.members.clone(),
            access: extension_access.clone(),
            origin: origin.clone(),
        };

        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                unreachable!("generic associated constants were rejected")
            };
            let is_method = function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self");
            if is_method {
                continue;
            }
            let key = (target_template.clone(), function.name.clone());
            let overload_key = (target_template.clone(), function.name.clone(), false);
            let overloaded = self
                .inherent_overload_counts
                .get(&overload_key)
                .copied()
                .unwrap_or_default()
                > 1;
            if self.generic_inherent_functions.contains_key(&key) && !overloaded {
                self.error(format!(
                    "duplicate generic associated function `{target_template}.{}`",
                    function.name
                ));
                continue;
            }
            let mut canonical = generic_inherent_function_name(target_template, &function.name);
            if overloaded {
                canonical =
                    overloaded_function_name(&canonical, &function_parameter_labels(function));
                self.inherent_overloads
                    .entry(overload_key)
                    .or_default()
                    .push(canonical.clone());
            }
            let mut generic = function.clone();
            generic.name = canonical.clone();
            let mut compile_groups = extension.compile_groups.clone();
            compile_groups.extend(generic.compile_groups.clone());
            generic.compile_groups = compile_groups;
            let mut where_predicates = extension.where_predicates.clone();
            where_predicates.extend(generic.where_predicates.clone());
            generic.where_predicates = where_predicates;
            let mut self_substitution = HashMap::new();
            self_substitution.insert("Self".to_owned(), extension.target.clone());
            substitute_function_types(&mut generic, &self_substitution);
            if let Some(body) = &mut generic.body {
                substitute_self_expression_target(body, target_template);
            }
            self.function_template_order.push(canonical.clone());
            self.function_templates.insert(canonical.clone(), generic);
            self.function_template_origins
                .insert(canonical.clone(), origin.clone());
            self.function_accesses
                .insert(canonical.clone(), extension_access.clone());
            self.generic_inherent_functions
                .entry(key)
                .or_insert(canonical);
        }

        self.generic_inherent_extensions
            .entry(target_template.clone())
            .or_default()
            .push(template.clone());

        let existing = self
            .nominal_instances
            .iter()
            .filter(|(_, instance)| instance.key.template == *target_template)
            .map(|(canonical, instance)| {
                (
                    canonical.clone(),
                    instance.key.arguments.clone(),
                    instance.key.kind,
                )
            })
            .collect::<Vec<_>>();
        for (canonical, arguments, _) in existing {
            let Some(source_arguments) = arguments
                .iter()
                .map(|argument| self.source_type_for_ty(argument))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            self.instantiate_generic_inherent_extension(
                target_template,
                &canonical,
                &source_arguments,
                &template,
            );
        }
    }

    fn instantiate_generic_inherent_extension(
        &mut self,
        target_template: &str,
        canonical: &str,
        source_arguments: &[Type],
        extension: &GenericInherentExtension,
    ) {
        if source_arguments.len() != extension.target_arguments.len() {
            self.error(format!(
                "internal error: invalid generic extension arguments for `{target_template}`"
            ));
            return;
        }
        let mut substitutions = HashMap::new();
        for (name, source) in extension.target_arguments.iter().zip(source_arguments) {
            substitutions.insert(name.clone(), source.clone());
        }
        let mut predicates = extension.where_predicates.clone();
        for predicate in &mut predicates {
            substitute_type_parameters(&mut predicate.subject, &substitutions);
            substitute_type_parameters(&mut predicate.trait_ref, &substitutions);
            for binding in &mut predicate.associated_types {
                substitute_type_parameters(&mut binding.ty, &substitutions);
            }
        }
        if predicates
            .iter()
            .any(|predicate| !self.concrete_where_predicate_holds(predicate))
        {
            return;
        }
        let mut members = extension.members.clone();
        let registered_members = members
            .iter()
            .filter_map(|member| match member {
                ExtendMember::Function(function) => Some((
                    function.name.clone(),
                    function
                        .groups
                        .first()
                        .is_some_and(|group| group.len() == 1 && group[0].name == "self"),
                    function_parameter_labels(function),
                )),
                ExtendMember::Const(_) => None,
            })
            .collect::<Vec<_>>();
        for (member, is_method, _) in &registered_members {
            let count = self
                .inherent_overload_counts
                .get(&(target_template.to_owned(), member.clone(), *is_method))
                .copied()
                .unwrap_or(1);
            self.inherent_overload_counts
                .insert((canonical.to_owned(), member.clone(), *is_method), count);
        }
        for member in &mut members {
            match member {
                ExtendMember::Function(function) => {
                    substitute_function_types(function, &substitutions)
                }
                ExtendMember::Const(binding) => {
                    if let Some(annotation) = &mut binding.annotation {
                        substitute_type_parameters(annotation, &substitutions);
                    }
                    substitute_expr_types(&mut binding.value, &substitutions);
                }
            }
        }
        self.collect_extension(
            ExtendDef {
                compile_groups: Vec::new(),
                target: Type::Named(canonical.to_owned(), Vec::new()),
                trait_ref: None,
                where_predicates: Vec::new(),
                members,
            },
            extension.origin.clone(),
        );
        for (member, is_method, shape) in registered_members {
            let mut name = if is_method {
                inherent_method_name(canonical, &member)
            } else {
                associated_function_name(canonical, &member)
            };
            if self
                .inherent_overload_counts
                .get(&(canonical.to_owned(), member.clone(), is_method))
                .copied()
                .unwrap_or_default()
                > 1
            {
                name = overloaded_function_name(&name, &shape);
            }
            if let Some(access) = self.function_accesses.get(&name).cloned() {
                self.function_accesses.insert(
                    name,
                    Self::intersect_access_boundaries(
                        &access,
                        &extension.access,
                        &extension.origin,
                    ),
                );
            }
        }
    }

    fn collect_nominal_layouts(&mut self) {
        for name in self.struct_order.clone() {
            let is_ready = self
                .nominal_instances
                .get(&name)
                .and_then(|instance| self.nominal_instance_states.get(&instance.key))
                == Some(&NominalInstanceState::Ready);
            if is_ready {
                continue;
            }
            let definition = self.struct_defs[&name].clone();
            self.build_struct_layout(&name, definition);
        }

        for name in self.enum_order.clone() {
            let is_ready = self
                .nominal_instances
                .get(&name)
                .and_then(|instance| self.nominal_instance_states.get(&instance.key))
                == Some(&NominalInstanceState::Ready);
            if is_ready {
                continue;
            }
            let definition = self.enum_defs[&name].clone();
            self.build_enum_layout(&name, definition);
        }
    }

    fn build_struct_layout(&mut self, name: &str, definition: StructDef) {
        let owner_access = self.nominal_access_or_internal(name);
        let mut seen = HashSet::new();
        let mut fields = Vec::new();
        for field in definition.fields {
            if !seen.insert(field.name.clone()) {
                self.error(format!(
                    "duplicate field `{}` in struct `{name}`",
                    field.name
                ));
                continue;
            }
            let mut ty = self.lower_source_type(&field.ty);
            if matches!(ty, Ty::Reference { .. }) {
                self.error(format!(
                    "borrow-typed field `{}.{}` is not supported until stored-reference drop and variance rules are implemented",
                    name, field.name
                ));
                ty = Ty::Error;
            }
            fields.push(FieldLayout {
                name: field.name,
                ty,
                access: Self::effective_member_access(&owner_access, field.visibility),
            });
        }
        self.struct_layouts.insert(
            name.to_owned(),
            StructLayout {
                name: name.to_owned(),
                fields,
            },
        );
        if let Some(info) = self.nominal_instances.get(name) {
            self.nominal_instance_states
                .insert(info.key.clone(), NominalInstanceState::Ready);
        }
    }

    fn build_enum_layout(&mut self, name: &str, definition: EnumDef) {
        let owner_access = self.nominal_access_or_internal(name);
        let mut seen_variants = HashSet::new();
        let mut variants = Vec::new();
        let mut payload_offset = 0;
        for variant in definition.variants {
            if !seen_variants.insert(variant.name.clone()) {
                self.error(format!(
                    "duplicate variant `{}` in enum `{name}`",
                    variant.name
                ));
                continue;
            }
            let (source_fields, named) = match variant.fields {
                VariantFields::Unit => (Vec::new(), false),
                VariantFields::Positional(types) => (
                    types
                        .into_iter()
                        .enumerate()
                        .map(|(index, ty)| (index.to_string(), ty, owner_access.visibility))
                        .collect(),
                    false,
                ),
                VariantFields::Named(fields) => (
                    fields
                        .into_iter()
                        .map(|field| (field.name, field.ty, field.visibility))
                        .collect(),
                    true,
                ),
            };
            let mut seen_fields = HashSet::new();
            let mut fields = Vec::new();
            for (field_name, source_ty, visibility) in source_fields {
                if !seen_fields.insert(field_name.clone()) {
                    self.error(format!(
                        "duplicate field `{field_name}` in variant `{name}.{}`",
                        variant.name
                    ));
                    continue;
                }
                let mut ty = self.lower_source_type(&source_ty);
                if matches!(ty, Ty::Reference { .. }) {
                    self.error(format!(
                        "borrow-typed enum field `{name}.{}.{field_name}` is not supported until stored-reference drop and variance rules are implemented",
                        variant.name
                    ));
                    ty = Ty::Error;
                }
                fields.push(FieldLayout {
                    name: field_name,
                    ty,
                    access: Self::effective_member_access(&owner_access, visibility),
                });
            }
            let field_count = fields.len();
            variants.push(VariantLayout {
                name: variant.name,
                fields,
                payload_offset,
                named,
            });
            payload_offset += field_count;
        }
        self.enum_layouts.insert(
            name.to_owned(),
            EnumLayout {
                name: name.to_owned(),
                variants,
            },
        );
        if let Some(info) = self.nominal_instances.get(name) {
            self.nominal_instance_states
                .insert(info.key.clone(), NominalInstanceState::Ready);
        }
    }

    fn snapshot_nominals(&self) -> NominalSnapshot {
        NominalSnapshot {
            struct_defs: self.struct_defs.clone(),
            enum_defs: self.enum_defs.clone(),
            struct_layouts: self.struct_layouts.clone(),
            enum_layouts: self.enum_layouts.clone(),
            nominal_accesses: self.nominal_accesses.clone(),
            struct_order: self.struct_order.clone(),
            enum_order: self.enum_order.clone(),
            instance_names: self.nominal_instance_names.clone(),
            instances: self.nominal_instances.clone(),
            states: self.nominal_instance_states.clone(),
            invalid_recursive_nominals: self.invalid_recursive_nominals.clone(),
        }
    }

    fn restore_nominals(&mut self, snapshot: NominalSnapshot) {
        self.struct_defs = snapshot.struct_defs;
        self.enum_defs = snapshot.enum_defs;
        self.struct_layouts = snapshot.struct_layouts;
        self.enum_layouts = snapshot.enum_layouts;
        self.nominal_accesses = snapshot.nominal_accesses;
        self.struct_order = snapshot.struct_order;
        self.enum_order = snapshot.enum_order;
        self.nominal_instance_names = snapshot.instance_names;
        self.nominal_instances = snapshot.instances;
        self.nominal_instance_states = snapshot.states;
        self.invalid_recursive_nominals = snapshot.invalid_recursive_nominals;
    }

    fn validate_generic_nominal_cycles(&mut self) {
        let nominal_names: HashSet<_> = self
            .struct_defs
            .keys()
            .chain(self.enum_defs.keys())
            .chain(self.struct_templates.keys())
            .chain(self.enum_templates.keys())
            .cloned()
            .collect();
        let generic_names: HashSet<_> = self
            .struct_templates
            .keys()
            .chain(self.enum_templates.keys())
            .cloned()
            .collect();
        let mut dependencies = HashMap::new();
        for (name, definition) in self.struct_defs.iter().chain(&self.struct_templates) {
            let bound: HashSet<_> = definition
                .compile_groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.as_str())
                .collect();
            let mut direct = Vec::new();
            for field in &definition.fields {
                collect_nominal_type_dependencies(&field.ty, &nominal_names, &bound, &mut direct);
            }
            dependencies.insert(name.clone(), direct);
        }
        for (name, definition) in self.enum_defs.iter().chain(&self.enum_templates) {
            let bound: HashSet<_> = definition
                .compile_groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.as_str())
                .collect();
            let mut direct = Vec::new();
            for variant in &definition.variants {
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Positional(types) => {
                        for ty in types {
                            collect_nominal_type_dependencies(
                                ty,
                                &nominal_names,
                                &bound,
                                &mut direct,
                            );
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            collect_nominal_type_dependencies(
                                &field.ty,
                                &nominal_names,
                                &bound,
                                &mut direct,
                            );
                        }
                    }
                }
            }
            dependencies.insert(name.clone(), direct);
        }

        let mut states = HashMap::new();
        let mut stack = Vec::new();
        let names: Vec<_> = nominal_names.into_iter().collect();
        for name in names {
            self.visit_generic_nominal_cycle(
                &name,
                &dependencies,
                &generic_names,
                &mut states,
                &mut stack,
            );
        }
    }

    fn visit_generic_nominal_cycle(
        &mut self,
        name: &str,
        dependencies: &HashMap<String, Vec<String>>,
        generic_names: &HashSet<String>,
        states: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
    ) {
        match states.get(name).copied() {
            Some(2) => return,
            Some(1) => {
                let start = stack.iter().position(|item| item == name).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.to_owned());
                if cycle.iter().any(|item| generic_names.contains(item)) {
                    for item in &cycle {
                        if generic_names.contains(item) {
                            self.invalid_recursive_nominals.insert(item.clone());
                        }
                    }
                    self.error(format!(
                        "recursive generic value layout has infinite size: {}",
                        cycle.join(" -> ")
                    ));
                }
                return;
            }
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        stack.push(name.to_owned());
        if let Some(items) = dependencies.get(name) {
            for dependency in items {
                self.visit_generic_nominal_cycle(
                    dependency,
                    dependencies,
                    generic_names,
                    states,
                    stack,
                );
            }
        }
        stack.pop();
        states.insert(name.to_owned(), 2);
    }

    fn validate_nominal_templates(&mut self) {
        let templates: Vec<_> = self
            .struct_template_order
            .iter()
            .map(|name| (NominalKind::Struct, name.clone()))
            .chain(
                self.enum_template_order
                    .iter()
                    .map(|name| (NominalKind::Enum, name.clone())),
            )
            .collect();
        for (kind, template_name) in templates {
            if self.invalid_recursive_nominals.contains(&template_name) {
                continue;
            }
            let parameters = match kind {
                NominalKind::Struct => self.struct_templates[&template_name]
                    .compile_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
                NominalKind::Enum => self.enum_templates[&template_name]
                    .compile_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            };
            let mut source_arguments = Vec::new();
            let mut arguments = Vec::new();
            for (index, parameter) in parameters.iter().enumerate() {
                let owner = format!("nominal::{template_name}");
                let marker = generic_parameter_marker(&owner, index, &parameter.name);
                self.abstract_type_parameters
                    .insert(marker.clone(), parameter.name.clone());
                source_arguments.push(Type::Named(marker.clone(), Vec::new()));
                arguments.push(Ty::Struct(marker));
            }
            let snapshot = self.snapshot_nominals();
            self.suppress_generic_inherent_instantiation += 1;
            let instance =
                self.ensure_nominal_instance(kind, &template_name, source_arguments, arguments);
            self.suppress_generic_inherent_instantiation -= 1;
            if let Some(canonical) = instance {
                let mut states = HashMap::new();
                let mut stack = Vec::new();
                self.visit_nominal_layout(&canonical, &mut states, &mut stack);
            }
            let dynamically_invalid = self.invalid_recursive_nominals.contains(&template_name);
            self.restore_nominals(snapshot);
            if dynamically_invalid {
                self.invalid_recursive_nominals.insert(template_name);
            }
        }
    }

    fn ensure_nominal_instance(
        &mut self,
        kind: NominalKind,
        template_name: &str,
        source_arguments: Vec<Type>,
        arguments: Vec<Ty>,
    ) -> Option<String> {
        if self.invalid_recursive_nominals.contains(template_name) {
            return None;
        }
        let key = NominalInstanceKey {
            kind,
            template: template_name.to_owned(),
            arguments,
        };
        if let Some(canonical) = self.nominal_instance_names.get(&key) {
            let canonical = canonical.clone();
            let info = &self.nominal_instances[&canonical];
            debug_assert_eq!(info.key, key);
            debug_assert_eq!(info.canonical, canonical);
            match self.nominal_instance_states.get(&key) {
                Some(NominalInstanceState::Ready) => {
                    self.instantiate_generic_trait_extensions_for_instance(
                        template_name,
                        &canonical,
                        &source_arguments,
                    );
                    return Some(canonical);
                }
                Some(NominalInstanceState::Building) => {
                    self.error(format!(
                        "recursive generic value layout has infinite size while instantiating `{template_name}`"
                    ));
                    self.invalid_recursive_nominals
                        .insert(template_name.to_owned());
                    return None;
                }
                None => {
                    self.error(format!(
                        "internal error: missing construction state for nominal instance `{canonical}`"
                    ));
                    return None;
                }
            }
        }
        let growing_recursive_instance =
            self.nominal_instance_states.iter().any(|(active, state)| {
                if *state != NominalInstanceState::Building
                    || active.kind != kind
                    || active.template != template_name
                    || active.arguments.is_empty()
                {
                    return false;
                }
                let Some(active_canonical) = self.nominal_instance_names.get(active) else {
                    return false;
                };
                let active_complexity = active
                    .arguments
                    .iter()
                    .map(|argument| self.nominal_type_complexity(argument))
                    .sum::<usize>();
                let next_complexity = key
                    .arguments
                    .iter()
                    .map(|argument| self.nominal_type_complexity(argument))
                    .sum::<usize>();
                key.arguments
                    .iter()
                    .any(|argument| ty_contains_nominal(argument, active_canonical))
                    || next_complexity >= active_complexity
            });
        if growing_recursive_instance {
            self.error(format!(
                "recursive generic value layout has infinite size while instantiating `{template_name}` with growing type arguments"
            ));
            self.invalid_recursive_nominals
                .insert(template_name.to_owned());
            return None;
        }
        let instance_count = self
            .nominal_instances
            .values()
            .filter(|instance| !instance.key.arguments.is_empty())
            .count();
        if instance_count >= MAX_NOMINAL_INSTANCES {
            self.error(format!(
                "generic nominal instance limit of {MAX_NOMINAL_INSTANCES} exceeded while instantiating `{template_name}`"
            ));
            return None;
        }

        let parameters = match kind {
            NominalKind::Struct => self.struct_templates[template_name]
                .compile_groups
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
            NominalKind::Enum => self.enum_templates[template_name]
                .compile_groups
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
        };
        if parameters.len() != source_arguments.len() {
            self.error(format!(
                "type argument count mismatch for `{template_name}`: expected {}, found {}",
                parameters.len(),
                source_arguments.len()
            ));
            return None;
        }
        let instance_source_arguments = source_arguments.clone();
        let mut substitutions = HashMap::new();
        for (parameter, argument) in parameters.iter().zip(source_arguments) {
            if substitutions
                .insert(parameter.name.clone(), argument)
                .is_some()
            {
                self.error(format!(
                    "duplicate compile-time parameter `{}` in generic nominal `{template_name}`",
                    parameter.name
                ));
                return None;
            }
        }

        let canonical = nominal_instance_name(&key);
        if let Some(existing) = self.nominal_instances.get(&canonical) {
            self.error(format!(
                "internal error: nominal instance name collision between `{}` and `{template_name}`",
                existing.key.template
            ));
            return None;
        }
        self.nominal_instance_names
            .insert(key.clone(), canonical.clone());
        self.nominal_instances.insert(
            canonical.clone(),
            NominalInstanceInfo {
                key: key.clone(),
                canonical: canonical.clone(),
            },
        );
        self.nominal_instance_states
            .insert(key.clone(), NominalInstanceState::Building);
        let access = self
            .nominal_accesses
            .get(template_name)
            .cloned()
            .unwrap_or_else(|| {
                self.error(format!(
                    "internal error: missing visibility metadata for nominal template `{template_name}`"
                ));
                AccessBoundary {
                    visibility: Visibility::Private,
                    origin: ItemOrigin::default(),
                }
            });
        self.nominal_accesses.insert(canonical.clone(), access);

        match kind {
            NominalKind::Struct => {
                self.struct_layouts.insert(
                    canonical.clone(),
                    StructLayout {
                        name: canonical.clone(),
                        fields: Vec::new(),
                    },
                );
                let mut definition = self.struct_templates[template_name].clone();
                substitute_struct_types(&mut definition, &substitutions);
                definition.name = canonical.clone();
                definition.compile_groups.clear();
                self.struct_defs
                    .insert(canonical.clone(), definition.clone());
                self.build_struct_layout(&canonical, definition);
                self.struct_order.push(canonical.clone());
            }
            NominalKind::Enum => {
                self.enum_layouts.insert(
                    canonical.clone(),
                    EnumLayout {
                        name: canonical.clone(),
                        variants: Vec::new(),
                    },
                );
                let mut definition = self.enum_templates[template_name].clone();
                substitute_enum_types(&mut definition, &substitutions);
                definition.name = canonical.clone();
                definition.compile_groups.clear();
                self.enum_defs.insert(canonical.clone(), definition.clone());
                self.build_enum_layout(&canonical, definition);
                self.enum_order.push(canonical.clone());
            }
        }
        self.nominal_instance_states
            .insert(key, NominalInstanceState::Ready);
        if self.suppress_generic_inherent_instantiation == 0 {
            let extensions = self
                .generic_inherent_extensions
                .get(template_name)
                .cloned()
                .unwrap_or_default();
            for extension in &extensions {
                self.instantiate_generic_inherent_extension(
                    template_name,
                    &canonical,
                    &instance_source_arguments,
                    extension,
                );
            }
            let trait_extensions = self
                .generic_trait_extensions
                .get(template_name)
                .cloned()
                .unwrap_or_default();
            for extension in &trait_extensions {
                self.instantiate_generic_trait_extension(
                    template_name,
                    &canonical,
                    &instance_source_arguments,
                    extension,
                );
            }
        }
        Some(canonical)
    }

    fn nominal_type_complexity(&self, ty: &Ty) -> usize {
        let mut seen = HashSet::new();
        self.nominal_type_complexity_with_seen(ty, &mut seen)
    }

    fn nominal_type_complexity_with_seen(&self, ty: &Ty, seen: &mut HashSet<String>) -> usize {
        match ty {
            Ty::Struct(name) | Ty::Enum(name) => {
                if !seen.insert(name.clone()) {
                    return 1;
                }
                let nominal_complexity = canonical_type_encoding(ty).len();
                let arguments = self
                    .nominal_instances
                    .get(name)
                    .map(|instance| instance.key.arguments.as_slice())
                    .unwrap_or(&[]);
                nominal_complexity
                    + arguments
                        .iter()
                        .map(|argument| self.nominal_type_complexity_with_seen(argument, seen))
                        .sum::<usize>()
            }
            Ty::Pointer { pointee, .. } | Ty::Reference { pointee, .. } | Ty::Array(pointee, _) => {
                1 + self.nominal_type_complexity_with_seen(pointee, seen)
            }
            Ty::Function(function) => 1 + self.function_type_complexity(function, seen),
            Ty::Callable(callable) => {
                1 + self.function_type_complexity(&callable.signature, seen)
                    + callable
                        .captures
                        .iter()
                        .map(|capture| self.nominal_type_complexity_with_seen(&capture.ty, seen))
                        .sum::<usize>()
            }
            Ty::Continuation { input, output } => {
                1 + self.nominal_type_complexity_with_seen(input, seen)
                    + self.nominal_type_complexity_with_seen(output, seen)
            }
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => {
                1 + self.nominal_type_complexity_with_seen(input, seen)
                    + self.nominal_type_complexity_with_seen(output, seen)
                    + self.nominal_type_complexity_with_seen(answer, seen)
            }
            Ty::EffectRow { throws_error, .. } => {
                1 + throws_error
                    .as_deref()
                    .map(|error| self.nominal_type_complexity_with_seen(error, seen))
                    .unwrap_or(0)
            }
            Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool | Ty::Unit | Ty::Never | Ty::Error => {
                1
            }
        }
    }

    fn function_type_complexity(&self, function: &FunctionTy, seen: &mut HashSet<String>) -> usize {
        function
            .groups
            .iter()
            .flatten()
            .map(|parameter| self.nominal_type_complexity_with_seen(parameter, seen))
            .sum::<usize>()
            + function
                .throws_error
                .as_deref()
                .map(|error| self.nominal_type_complexity_with_seen(error, seen))
                .unwrap_or(0)
            + self.nominal_type_complexity_with_seen(&function.result, seen)
    }

    fn validate_nominal_layouts(&mut self) {
        let mut states = HashMap::new();
        let mut stack = Vec::new();
        let names: Vec<_> = self
            .struct_order
            .iter()
            .chain(&self.enum_order)
            .cloned()
            .collect();
        for name in names {
            self.visit_nominal_layout(&name, &mut states, &mut stack);
        }
    }

    fn visit_nominal_layout(
        &mut self,
        name: &str,
        states: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
    ) {
        match states.get(name).copied() {
            Some(2) => return,
            Some(1) => {
                let start = stack.iter().position(|item| item == name).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.to_owned());
                self.error(format!(
                    "recursive value layout has infinite size: {}",
                    cycle.join(" -> ")
                ));
                return;
            }
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        stack.push(name.to_owned());
        let dependencies: Vec<String> = if let Some(layout) = self.struct_layouts.get(name) {
            layout
                .fields
                .iter()
                .filter_map(|field| nominal_name(&field.ty).map(str::to_owned))
                .collect()
        } else if let Some(layout) = self.enum_layouts.get(name) {
            layout
                .variants
                .iter()
                .flat_map(|variant| &variant.fields)
                .filter_map(|field| nominal_name(&field.ty).map(str::to_owned))
                .collect()
        } else {
            Vec::new()
        };
        for dependency in dependencies {
            self.visit_nominal_layout(&dependency, states, stack);
        }
        stack.pop();
        states.insert(name.to_owned(), 2);
    }

    fn analyze(&mut self) -> Option<HirProgram> {
        self.analyze_target(true)
    }

    fn analyze_target(&mut self, require_entry_point: bool) -> Option<HirProgram> {
        for name in self.global_order.clone() {
            self.lower_global(&name);
        }
        let mut function_index = 0;
        while function_index < self.function_order.len() {
            let name = self.function_order[function_index].clone();
            self.lower_function(&name);
            function_index += 1;
        }
        self.validate_nominal_layouts();
        self.validate_inferred_api_visibility();
        if require_entry_point {
            self.validate_entry_point();
        }

        if !self.diagnostics.is_empty() {
            return None;
        }

        let mut functions: Vec<_> = self
            .function_order
            .iter()
            .filter_map(|name| self.hir_functions.get(name).cloned())
            .collect();
        functions.extend(self.lifted_functions.clone());
        Some(HirProgram {
            structs: self
                .struct_order
                .iter()
                .map(|name| self.struct_layouts[name].clone())
                .collect(),
            enums: self
                .enum_order
                .iter()
                .map(|name| self.enum_layouts[name].clone())
                .collect(),
            globals: self
                .global_order
                .iter()
                .map(|name| self.hir_globals[name].clone())
                .collect(),
            functions,
            drop_methods: self
                .trait_impls
                .iter()
                .filter(|(key, _)| {
                    key.trait_ref.name == self.lang_item_name(LangItemKind::Drop)
                        && key.trait_ref.arguments.is_empty()
                })
                .filter_map(|(key, implementation)| {
                    implementation
                        .methods
                        .get("drop")
                        .map(|method| (key.self_ty.clone(), method.clone()))
                })
                .collect(),
            box_pointees: self
                .nominal_instances
                .iter()
                .filter(|(_, instance)| {
                    instance.key.kind == NominalKind::Struct
                        && instance.key.template == "alloc::boxed::Box"
                        && instance.key.arguments.len() == 1
                })
                .map(|(name, instance)| (name.clone(), instance.key.arguments[0].clone()))
                .collect(),
            array_types: self.array_types.clone(),
            continuation_adapters: self.continuation_adapters.clone(),
            effect_callable_adapters: self.effect_callable_adapters.clone(),
        })
    }

    fn validate_function_templates(&mut self) {
        for template_name in self.function_template_order.clone() {
            let template = self.function_templates[&template_name].clone();
            if template.body.is_none()
                && [
                    LangItemKind::Do,
                    LangItemKind::Try,
                    LangItemKind::Throw,
                    LangItemKind::Unsafe,
                    LangItemKind::Loop,
                ]
                .into_iter()
                .any(|kind| self.lang_item_name(kind) == template_name)
            {
                continue;
            }
            if template.return_type.is_none() {
                self.error(format!(
                    "generic function `{template_name}` requires an explicit return type"
                ));
                continue;
            }
            let compile_parameter_kinds = compile_parameter_kinds(&template.compile_groups);
            if !self.validate_where_predicate_shapes(
                &format!("generic function `{template_name}`"),
                &template.where_predicates,
                &compile_parameter_kinds,
            ) {
                continue;
            }
            if template.compile_groups.iter().flatten().any(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::TypeConstructor { .. }
                        | CompileParamKind::EffectConstructor { .. }
                )
            }) {
                continue;
            }

            let mut substitutions = HashMap::new();
            for (index, parameter) in template.compile_groups.iter().flatten().enumerate() {
                let marker = match parameter.kind {
                    CompileParamKind::Type => {
                        let marker =
                            generic_parameter_marker(&template_name, index, &parameter.name);
                        self.abstract_type_parameters
                            .insert(marker.clone(), parameter.name.clone());
                        marker
                    }
                    CompileParamKind::Access => ACCESS_SHARED_MARKER.to_owned(),
                    CompileParamKind::Passing => PASSING_AUTO_MARKER.to_owned(),
                    // Abstract validation uses the maximal currently supported row. Every
                    // concrete instance is lowered again after substituting its selected row.
                    CompileParamKind::Effect => EFFECT_UNSAFE_MARKER.to_owned(),
                    CompileParamKind::Region => continue,
                    CompileParamKind::TypeConstructor { .. }
                    | CompileParamKind::EffectConstructor { .. } => unreachable!(
                        "constructor parameters are validated through concrete instances"
                    ),
                };
                if substitutions
                    .insert(parameter.name.clone(), Type::Named(marker, Vec::new()))
                    .is_some()
                {
                    self.error(format!(
                        "duplicate compile-time parameter `{}` in generic function `{template_name}`",
                        parameter.name
                    ));
                }
            }

            let functions_before = self.functions.clone();
            let function_origins_before = self.function_origins.clone();
            let function_accesses_before = self.function_accesses.clone();
            let function_order_before = self.function_order.clone();
            let signatures_before = self.signatures.clone();
            let function_states_before = self.function_states.clone();
            let hir_functions_before = self.hir_functions.clone();
            let global_states_before = self.global_states.clone();
            let hir_globals_before = self.hir_globals.clone();
            let nominals_before = self.snapshot_nominals();
            let instance_names_before = self.function_instance_names.clone();
            let instances_before = self.function_instances.clone();
            let type_substitutions_before = self.function_type_substitutions.clone();
            let lifted_functions_before = self.lifted_functions.clone();
            let handler_frame_parameter_modes_before = self.handler_frame_parameter_modes.clone();
            let continuation_adapters_before = self.continuation_adapters.clone();
            let effect_callable_adapters_before = self.effect_callable_adapters.clone();
            let next_closure = self.next_closure;
            let inherent_members_before = self.inherent_members.clone();
            let copy_nominals_before = self.copy_nominals.clone();
            let trait_impl_headers_before = self.trait_impl_headers.clone();
            let trait_impls_before = self.trait_impls.clone();
            let trait_methods_before = self.trait_methods_by_receiver.clone();

            let mut function = template;
            substitute_function_types(&mut function, &substitutions);
            for predicate in &function.where_predicates {
                let subject = self.lower_source_type(&predicate.subject);
                if let Type::Named(_, arguments) = &predicate.trait_ref {
                    for argument in arguments {
                        self.lower_source_type(argument);
                    }
                }
                for binding in &predicate.associated_types {
                    self.lower_source_type(&binding.ty);
                }
                if matches!(&predicate.trait_ref, Type::Named(name, arguments)
                    if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty())
                    && subject != Ty::Error
                {
                    self.copy_nominals.insert(subject);
                }
            }
            self.install_assumed_where_predicates(&template_name, &function.where_predicates);
            let validation_name = generic_validation_name(&template_name);
            function.name = validation_name.clone();
            function.compile_groups.clear();
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
            let unsafe_effect = self.function_effects_unsafe(&function.effects);
            let throws_error = function
                .effects
                .throws
                .as_deref()
                .map(|error| self.lower_source_type(error));
            let custom_effects = self.function_effects_custom_identities(&function.effects);
            self.functions.insert(validation_name.clone(), function);
            self.function_origins.insert(
                validation_name.clone(),
                self.function_template_origins[&template_name].clone(),
            );
            self.signatures.insert(
                validation_name.clone(),
                FunctionSig {
                    groups,
                    unsafe_effect,
                    throws_error,
                    custom_effects,
                    result,
                },
            );
            self.function_type_substitutions
                .insert(validation_name.clone(), substitutions);
            self.lower_function(&validation_name);
            self.functions = functions_before;
            self.function_origins = function_origins_before;
            self.function_accesses = function_accesses_before;
            self.function_order = function_order_before;
            self.signatures = signatures_before;
            self.function_states = function_states_before;
            self.hir_functions = hir_functions_before;
            self.global_states = global_states_before;
            self.hir_globals = hir_globals_before;
            self.restore_nominals(nominals_before);
            self.function_instance_names = instance_names_before;
            self.function_instances = instances_before;
            self.function_type_substitutions = type_substitutions_before;
            self.lifted_functions = lifted_functions_before;
            self.handler_frame_parameter_modes = handler_frame_parameter_modes_before;
            self.continuation_adapters = continuation_adapters_before;
            self.effect_callable_adapters = effect_callable_adapters_before;
            self.next_closure = next_closure;
            self.inherent_members = inherent_members_before;
            self.copy_nominals = copy_nominals_before;
            self.trait_impl_headers = trait_impl_headers_before;
            self.trait_impls = trait_impls_before;
            self.trait_methods_by_receiver = trait_methods_before;
        }
    }

    fn install_assumed_where_predicates(
        &mut self,
        function: &str,
        predicates: &[crate::ast::WherePredicate],
    ) {
        for predicate in predicates {
            let Type::Named(trait_name, source_arguments) = &predicate.trait_ref else {
                continue;
            };
            let Some(schema) = self.traits.get(trait_name).cloned() else {
                continue;
            };
            let self_ty = self.lower_source_type(&predicate.subject);
            let arguments = source_arguments
                .iter()
                .map(|argument| self.lower_source_type(argument))
                .collect::<Vec<_>>();
            let associated_types = predicate
                .associated_types
                .iter()
                .map(|binding| (binding.name.clone(), self.lower_source_type(&binding.ty)))
                .collect::<HashMap<_, _>>();
            if self_ty == Ty::Error
                || arguments.contains(&Ty::Error)
                || associated_types.values().any(|ty| *ty == Ty::Error)
            {
                continue;
            }
            let key = TraitImplKey {
                self_ty,
                trait_ref: TraitRefKey {
                    name: trait_name.clone(),
                    arguments,
                },
            };
            self.trait_impl_headers.insert(key.clone());
            if self.trait_impls.contains_key(&key) {
                continue;
            }

            let mut substitutions = HashMap::new();
            substitutions.insert("Self".to_owned(), predicate.subject.clone());
            for (parameter, argument) in schema.compile_parameters.iter().zip(source_arguments) {
                substitutions.insert(parameter.name.clone(), argument.clone());
            }
            for binding in &predicate.associated_types {
                substitutions.insert(binding.name.clone(), binding.ty.clone());
            }
            let mut methods = HashMap::new();
            let associated_types_complete = schema
                .associated_types
                .iter()
                .all(|name| associated_types.contains_key(name));
            for method_id in schema
                .method_order
                .iter()
                .filter(|_| associated_types_complete)
            {
                let declaration = &schema.methods[method_id];
                let mut method = declaration.clone();
                substitute_function_types(&mut method, &substitutions);
                let canonical = assumed_trait_method_name(function, &key, method_id);
                let groups = method
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| ParamSig {
                                name: parameter.name.clone(),
                                ty: self.lower_source_type(&parameter.ty),
                                mode: parameter.mode,
                            })
                            .collect()
                    })
                    .collect();
                let result = method
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result));
                let throws_error = method
                    .effects
                    .throws
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafe_effect: self.function_effects_unsafe(&method.effects),
                        throws_error,
                        custom_effects: self.function_effects_custom_identities(&method.effects),
                        result,
                    },
                );
                methods.insert(method_id.clone(), canonical);
                if schema_function_has_receiver(declaration) {
                    let candidates = self
                        .trait_methods_by_receiver
                        .entry((key.self_ty.clone(), declaration.name.clone()))
                        .or_default();
                    if !candidates.contains(&key) {
                        candidates.push(key.clone());
                    }
                }
            }
            self.trait_impls.insert(
                key.clone(),
                TraitImplInfo {
                    key,
                    associated_types,
                    associated_type_sources: HashMap::new(),
                    methods,
                    access: schema.access,
                },
            );
        }
    }

    fn validate_where_predicate_shapes(
        &mut self,
        owner: &str,
        predicates: &[crate::ast::WherePredicate],
        _compile_parameter_kinds: &HashMap<String, CompileParamKind>,
    ) -> bool {
        let mut valid = true;
        let mut seen = HashSet::new();
        for predicate in predicates {
            if !seen.insert((predicate.subject.clone(), predicate.trait_ref.clone())) {
                self.error(format!("duplicate where predicate in {owner}"));
                valid = false;
                continue;
            }
            let Type::Named(name, arguments) = &predicate.trait_ref else {
                self.error(format!("where predicate in {owner} must reference a trait"));
                valid = false;
                continue;
            };
            let Some(schema) = self.traits.get(name).cloned() else {
                self.error(format!(
                    "unknown trait `{name}` in where predicate of {owner}"
                ));
                valid = false;
                continue;
            };
            let expected_arguments = schema.compile_parameters.len();
            if arguments.len() != expected_arguments {
                self.error(format!(
                    "trait argument count mismatch for `{name}` in where predicate of {owner}: expected {expected_arguments}, found {}",
                    arguments.len()
                ));
                valid = false;
            }
            let mut associated = HashSet::new();
            for binding in &predicate.associated_types {
                if !schema.associated_types.contains(&binding.name) {
                    self.error(format!(
                        "unknown associated type `{name}.{}` in where predicate of {owner}",
                        binding.name
                    ));
                    valid = false;
                } else if schema.associated_type_kinds[&binding.name] != CompileParamKind::Type {
                    self.error(format!(
                        "generic associated type equality `{name}.{}` in where predicate of {owner} is not supported yet",
                        binding.name
                    ));
                    valid = false;
                } else if !associated.insert(binding.name.clone()) {
                    self.error(format!(
                        "duplicate associated type equality `{name}.{}` in where predicate of {owner}",
                        binding.name
                    ));
                    valid = false;
                }
            }
        }
        valid
    }

    fn validate_trait_inheritance_implementations(&mut self) {
        let trait_impl_headers = self.trait_impl_headers.iter().cloned().collect::<Vec<_>>();
        for key in trait_impl_headers {
            let Some(schema) = self.traits.get(&key.trait_ref.name).cloned() else {
                continue;
            };
            let Some(predicates) = self.substituted_trait_where_predicates(&schema, &key) else {
                continue;
            };
            for predicate in predicates {
                if let Some(required) = self.constructor_trait_impl_key_from_predicate(&predicate) {
                    if !self.constructor_trait_impl_headers.contains(&required) {
                        let target = self.diagnostic_type_name(&key.self_ty);
                        self.error(format!(
                            "trait implementation of `{}` for `{target}` requires constructor trait `{}` for `{}`",
                            key.trait_ref.name, required.trait_ref.name, required.target.name
                        ));
                    }
                } else if !self.concrete_where_predicate_holds(&predicate) {
                    let target = self.diagnostic_type_name(&key.self_ty);
                    self.error(format!(
                        "trait implementation of `{}` for `{target}` does not satisfy inherited where predicate",
                        key.trait_ref.name
                    ));
                }
            }
        }

        let constructor_headers = self
            .constructor_trait_impl_headers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in constructor_headers {
            let Some(schema) = self.traits.get(&key.trait_ref.name).cloned() else {
                continue;
            };
            let Some(predicates) =
                self.substituted_constructor_trait_where_predicates(&schema, &key)
            else {
                continue;
            };
            for predicate in predicates {
                if let Some(required) = self.constructor_trait_impl_key_from_predicate_for_target(
                    &predicate,
                    Some(&key.target),
                ) {
                    if required == key {
                        continue;
                    }
                    if !self.constructor_trait_impl_headers.contains(&required) {
                        self.error(format!(
                            "constructor trait implementation of `{}` for `{}` requires `{}` for `{}`",
                            key.trait_ref.name,
                            key.target.name,
                            required.trait_ref.name,
                            required.target.name
                        ));
                    }
                } else if !self.concrete_where_predicate_holds(&predicate) {
                    self.error(format!(
                        "constructor trait implementation of `{}` for `{}` does not satisfy inherited where predicate",
                        key.trait_ref.name, key.target.name
                    ));
                }
            }
        }
    }

    fn substituted_trait_where_predicates(
        &self,
        schema: &TraitSchema,
        key: &TraitImplKey,
    ) -> Option<Vec<crate::ast::WherePredicate>> {
        let mut substitutions = HashMap::new();
        substitutions.insert("Self".to_owned(), self.source_type_for_ty(&key.self_ty)?);
        for (parameter, argument) in schema
            .compile_parameters
            .iter()
            .zip(&key.trait_ref.arguments)
        {
            substitutions.insert(parameter.name.clone(), self.source_type_for_ty(argument)?);
        }
        Some(
            schema
                .where_predicates
                .iter()
                .cloned()
                .map(|mut predicate| {
                    substitute_where_predicate(&mut predicate, &substitutions);
                    predicate
                })
                .collect(),
        )
    }

    fn substituted_constructor_trait_where_predicates(
        &self,
        schema: &TraitSchema,
        key: &ConstructorTraitImplKey,
    ) -> Option<Vec<crate::ast::WherePredicate>> {
        let mut substitutions = HashMap::new();
        substitutions.insert(
            "Self".to_owned(),
            Type::Named(key.target.name.clone(), Vec::new()),
        );
        for (parameter, argument) in schema
            .compile_parameters
            .iter()
            .zip(&key.trait_ref.arguments)
        {
            substitutions.insert(parameter.name.clone(), self.source_type_for_ty(argument)?);
        }
        Some(
            schema
                .where_predicates
                .iter()
                .cloned()
                .map(|mut predicate| {
                    substitute_where_predicate(&mut predicate, &substitutions);
                    predicate
                })
                .collect(),
        )
    }

    fn constructor_trait_impl_key_from_predicate(
        &mut self,
        predicate: &crate::ast::WherePredicate,
    ) -> Option<ConstructorTraitImplKey> {
        self.constructor_trait_impl_key_from_predicate_for_target(predicate, None)
    }

    fn constructor_trait_impl_key_from_predicate_for_target(
        &mut self,
        predicate: &crate::ast::WherePredicate,
        target_override: Option<&TypeConstructorImplTarget>,
    ) -> Option<ConstructorTraitImplKey> {
        let target = target_override
            .filter(|target| {
                matches!(
                    &predicate.subject,
                    Type::Named(name, arguments)
                        if arguments.is_empty() && name == &target.name
                )
            })
            .cloned()
            .or_else(|| self.type_constructor_impl_target(&predicate.subject))?;
        if !self.trait_ref_has_constructor_subject(&predicate.trait_ref) {
            return None;
        }
        let Type::Named(trait_name, source_arguments) = &predicate.trait_ref else {
            return None;
        };
        let schema = self.traits.get(trait_name).cloned()?;
        let expected = schema.compile_parameters.len();
        if source_arguments.len() != expected {
            return None;
        }
        let arguments = source_arguments
            .iter()
            .map(|argument| self.lower_source_type(argument))
            .collect::<Vec<_>>();
        if arguments.contains(&Ty::Error) {
            return None;
        }
        Some(ConstructorTraitImplKey {
            target,
            trait_ref: ConstructorTraitRefKey {
                name: trait_name.clone(),
                arguments,
            },
        })
    }

    fn validate_concrete_where_predicates(
        &mut self,
        function: &str,
        predicates: &[crate::ast::WherePredicate],
    ) -> bool {
        let mut valid = true;
        for predicate in predicates {
            if let Some(required) = self.constructor_trait_impl_key_from_predicate(predicate) {
                if !self.constructor_trait_impl_headers.contains(&required) {
                    self.error(format!(
                        "where predicate `{}: {}` is not satisfied while instantiating `{function}`",
                        required.target.name, required.trait_ref.name
                    ));
                    valid = false;
                }
                continue;
            }
            let subject = self.lower_source_type(&predicate.subject);
            let Type::Named(name, source_arguments) = &predicate.trait_ref else {
                valid = false;
                continue;
            };
            let arguments = source_arguments
                .iter()
                .map(|argument| self.lower_source_type(argument))
                .collect::<Vec<_>>();
            let associated_types = predicate
                .associated_types
                .iter()
                .map(|binding| (binding.name.clone(), self.lower_source_type(&binding.ty)))
                .collect::<HashMap<_, _>>();
            if subject == Ty::Error
                || arguments.contains(&Ty::Error)
                || associated_types.values().any(|ty| *ty == Ty::Error)
            {
                valid = false;
                continue;
            }
            let satisfied =
                if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty() {
                    self.is_copy_type(&subject)
                } else if BINARY_OPERATOR_TRAITS
                    .iter()
                    .any(|operator| name == self.lang_item_name(operator.lang_item))
                    && subject.is_integer()
                    && arguments.as_slice() == [subject.clone()]
                {
                    associated_types
                        .get("Output")
                        .is_none_or(|output| output == &subject)
                } else if let Some(output) =
                    self.builtin_unary_operator_output(name, &subject, &arguments)
                {
                    associated_types
                        .get("Output")
                        .is_none_or(|expected| expected == &output)
                } else {
                    self.trait_impls
                        .get(&TraitImplKey {
                            self_ty: subject.clone(),
                            trait_ref: TraitRefKey {
                                name: name.clone(),
                                arguments,
                            },
                        })
                        .is_some_and(|implementation| {
                            associated_types.iter().all(|(name, expected)| {
                                implementation.associated_types.get(name) == Some(expected)
                            })
                        })
                };
            if !satisfied {
                self.error(format!(
                    "where predicate `{}: {}` is not satisfied while instantiating `{function}`",
                    self.diagnostic_type_name(&subject),
                    name
                ));
                valid = false;
            }
        }
        valid
    }

    fn concrete_where_predicate_holds(&mut self, predicate: &crate::ast::WherePredicate) -> bool {
        if let Some(required) = self.constructor_trait_impl_key_from_predicate(predicate) {
            return self.constructor_trait_impl_headers.contains(&required);
        }
        let subject = self.lower_source_type(&predicate.subject);
        let Type::Named(name, source_arguments) = &predicate.trait_ref else {
            return false;
        };
        let arguments = source_arguments
            .iter()
            .map(|argument| self.lower_source_type(argument))
            .collect::<Vec<_>>();
        let associated_types = predicate
            .associated_types
            .iter()
            .map(|binding| (binding.name.clone(), self.lower_source_type(&binding.ty)))
            .collect::<HashMap<_, _>>();
        if subject == Ty::Error
            || arguments.contains(&Ty::Error)
            || associated_types.values().any(|ty| *ty == Ty::Error)
        {
            return false;
        }
        if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty() {
            return self.is_copy_type(&subject);
        }
        if BINARY_OPERATOR_TRAITS
            .iter()
            .any(|operator| name == self.lang_item_name(operator.lang_item))
            && subject.is_integer()
            && arguments.as_slice() == [subject.clone()]
        {
            return associated_types
                .get("Output")
                .is_none_or(|output| output == &subject);
        }
        if let Some(output) = self.builtin_unary_operator_output(name, &subject, &arguments) {
            return associated_types
                .get("Output")
                .is_none_or(|expected| expected == &output);
        }
        self.trait_impls
            .get(&TraitImplKey {
                self_ty: subject,
                trait_ref: TraitRefKey {
                    name: name.clone(),
                    arguments,
                },
            })
            .is_some_and(|implementation| {
                associated_types.iter().all(|(name, expected)| {
                    implementation.associated_types.get(name) == Some(expected)
                })
            })
    }

    fn lower_source_type(&mut self, source: &Type) -> Ty {
        match source {
            Type::I32 => Ty::I32,
            Type::I64 => Ty::I64,
            Type::U32 => Ty::U32,
            Type::U64 => Ty::U64,
            Type::Bool => Ty::Bool,
            Type::Unit => Ty::Unit,
            Type::Function {
                groups,
                effects,
                result,
            } => {
                if !effects.parameters.is_empty() {
                    self.error(format!(
                        "unresolved effect parameter{} `{}` in function type",
                        if effects.parameters.len() == 1 {
                            ""
                        } else {
                            "s"
                        },
                        effects.parameters.join(", ")
                    ));
                    return Ty::Error;
                }
                Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| group.iter().map(|ty| self.lower_source_type(ty)).collect())
                        .collect(),
                    unsafe_effect: self.function_effects_unsafe(effects),
                    throws_error: effects
                        .throws
                        .as_deref()
                        .map(|error| Box::new(self.lower_source_type(error))),
                    custom_effects: self.function_effects_custom_identities(effects),
                    result: Box::new(self.lower_source_type(result)),
                })
            }
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Ty::Reference {
                pointee: Box::new(self.lower_source_type(pointee)),
                mutable: *mutable,
                region: region.clone(),
            },
            Type::Array(element, length) => {
                let element = self.lower_source_type(element);
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} exceeds the first-version limit of {}",
                        i32::MAX
                    ));
                    Ty::Error
                } else if element == Ty::Unit {
                    self.error("array element type `()` is not supported in the first version");
                    Ty::Error
                } else {
                    let array = Ty::Array(Box::new(element), *length);
                    self.array_types.insert(array.clone());
                    array
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Ty::Unit,
            Type::Named(name, _) if effect_row_from_marker(name).is_some() => {
                let Some((unsafe_effect, throws_error, custom_effects)) =
                    effect_row_from_source(source)
                else {
                    self.error("effect row carries more than one thrown error type");
                    return Ty::Error;
                };
                Ty::EffectRow {
                    unsafe_effect,
                    throws_error: throws_error
                        .as_ref()
                        .map(|error| Box::new(self.lower_source_type(error))),
                    custom_effects,
                }
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && is_compile_value_marker(name) =>
            {
                Ty::Struct(name.clone())
            }
            Type::Named(name, arguments) if matches!(name.as_str(), "Ptr" | "MutPtr") => {
                if arguments.len() != 1 {
                    self.error(format!("type `{name}` expects exactly one type argument"));
                    Ty::Error
                } else {
                    Ty::Pointer {
                        pointee: Box::new(self.lower_source_type(&arguments[0])),
                        mutable: name == "MutPtr",
                    }
                }
            }
            Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::Continuation) =>
            {
                if arguments.len() != 2 {
                    self.error("Continuation expects input and output type arguments");
                    Ty::Error
                } else {
                    Ty::Continuation {
                        input: Box::new(self.lower_source_type(&arguments[0])),
                        output: Box::new(self.lower_source_type(&arguments[1])),
                    }
                }
            }
            Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::EffectCallable) =>
            {
                if arguments.len() != 3 {
                    self.error("EffectCallable expects input, output, and answer type arguments");
                    Ty::Error
                } else {
                    Ty::EffectCallable {
                        input: Box::new(self.lower_source_type(&arguments[0])),
                        output: Box::new(self.lower_source_type(&arguments[1])),
                        answer: Box::new(self.lower_source_type(&arguments[2])),
                    }
                }
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) =>
            {
                Ty::Struct(name.clone())
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Ty::Struct(name.clone())
                } else if self.enum_defs.contains_key(name) {
                    Ty::Enum(name.clone())
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` requires explicit type arguments"
                    ));
                    Ty::Error
                } else {
                    self.error(format!("unknown type `{name}`"));
                    Ty::Error
                }
            }
            Type::Named(name, source_arguments) => {
                let kind = if self.struct_templates.contains_key(name) {
                    NominalKind::Struct
                } else if self.enum_templates.contains_key(name) {
                    NominalKind::Enum
                } else if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                    self.error(format!(
                        "non-generic type `{name}` does not accept type arguments"
                    ));
                    return Ty::Error;
                } else {
                    self.error(format!("unknown generic type `{name}`"));
                    return Ty::Error;
                };
                let expected = match kind {
                    NominalKind::Struct => self.struct_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                    NominalKind::Enum => self.enum_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                };
                if source_arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}`: expected {expected}, found {}",
                        source_arguments.len()
                    ));
                    return Ty::Error;
                }
                let mut arguments = Vec::new();
                for argument in source_arguments {
                    let argument = self.lower_source_type(argument);
                    if argument == Ty::Error {
                        return Ty::Error;
                    }
                    arguments.push(argument);
                }
                let Some(canonical) =
                    self.ensure_nominal_instance(kind, name, source_arguments.clone(), arguments)
                else {
                    return Ty::Error;
                };
                match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                }
            }
            Type::NamedArgs(name, _) => {
                self.error(format!(
                    "internal error: labeled type arguments for `{name}` were not normalized"
                ));
                Ty::Error
            }
        }
    }

    fn struct_layout_or_diagnostic(&mut self, name: &str) -> Option<StructLayout> {
        if let Some(layout) = self.struct_layouts.get(name) {
            return Some(layout.clone());
        }
        if let Some(parameter) = self.abstract_type_parameters.get(name).cloned() {
            self.error(format!(
                "generic parameter `{parameter}` has no known fields or struct layout"
            ));
        } else {
            self.error(format!(
                "internal error: struct type `{name}` has no registered layout"
            ));
        }
        None
    }

    fn enum_layout_or_diagnostic(&mut self, name: &str) -> Option<EnumLayout> {
        if let Some(layout) = self.enum_layouts.get(name) {
            return Some(layout.clone());
        }
        if let Some(parameter) = self.abstract_type_parameters.get(name).cloned() {
            self.error(format!(
                "generic parameter `{parameter}` has no known variants or enum layout"
            ));
        } else {
            self.error(format!(
                "internal error: enum type `{name}` has no registered layout"
            ));
        }
        None
    }

    fn type_argument_from_expr(
        &mut self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Type(source) => {
                let mut source = source.clone();
                substitute_type_parameters(&mut source, substitutions);
                Some(source)
            }
            Expr::Unit => Some(Type::Unit),
            Expr::Name(name) => {
                if let Some(replacement) = substitutions.get(name) {
                    return Some(replacement.clone());
                }
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    self.error("generic type arguments require a named type constructor");
                    return None;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    self.error("generic type arguments cannot contain labeled arguments");
                    return None;
                }
                if name == "Array" {
                    if groups.len() != 1 || groups[0].len() != 2 {
                        self.error("`Array` type arguments require an element type and length");
                        return None;
                    }
                    let call_arguments = groups[0];
                    let element =
                        self.type_argument_from_expr(&call_arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = call_arguments[1].value else {
                        self.error("array type argument length must be a non-negative integer");
                        return None;
                    };
                    let Ok(length) = u64::try_from(length) else {
                        self.error("array type argument length must fit in `u64`");
                        return None;
                    };
                    Some(Type::Array(Box::new(element), length))
                } else {
                    let mut arguments = Vec::new();
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        arguments
                            .push(self.type_argument_from_expr(&argument.value, substitutions)?);
                    }
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => {
                self.error(format!(
                    "generic type arguments must be type names or type applications, found `{expression:?}`"
                ));
                None
            }
        }
    }

    fn type_constructor_argument_from_expr(
        &mut self,
        expression: &Expr,
        parameter_count: usize,
        owner: &str,
        parameter: &str,
    ) -> Option<String> {
        let Expr::Name(name) = expression else {
            self.error(format!(
                "invalid type-constructor argument for `{parameter}` in `{owner}`; expected a generic type constructor"
            ));
            return None;
        };
        let Some(target) =
            self.type_constructor_impl_target(&Type::Named(name.clone(), Vec::new()))
        else {
            self.error(format!(
                "invalid type-constructor argument `{name}` for `{parameter}` in `{owner}`; expected a generic type constructor"
            ));
            return None;
        };
        if target.parameter_count != parameter_count {
            self.error(format!(
                "type-constructor argument `{name}` for `{parameter}` in `{owner}` has {} parameter{}, expected {parameter_count}",
                target.parameter_count,
                if target.parameter_count == 1 { "" } else { "s" }
            ));
            return None;
        }
        Some(name.clone())
    }

    fn source_type_for_ty(&self, ty: &Ty) -> Option<Type> {
        match ty {
            Ty::I32 => Some(Type::I32),
            Ty::I64 => Some(Type::I64),
            Ty::U32 => Some(Type::U32),
            Ty::U64 => Some(Type::U64),
            Ty::Bool => Some(Type::Bool),
            Ty::Unit => Some(Type::Unit),
            Ty::Array(element, length) => Some(Type::Array(
                Box::new(self.source_type_for_ty(element)?),
                *length,
            )),
            Ty::Pointer { pointee, mutable } => Some(Type::Named(
                if *mutable { "MutPtr" } else { "Ptr" }.to_owned(),
                vec![self.source_type_for_ty(pointee)?],
            )),
            Ty::Reference {
                pointee,
                mutable,
                region,
            } => Some(Type::Borrow {
                mutable: *mutable,
                access: None,
                region: region.clone(),
                pointee: Box::new(self.source_type_for_ty(pointee)?),
            }),
            Ty::Struct(name) | Ty::Enum(name) => {
                if is_compile_value_marker(name) {
                    if let Some(constructor) = type_constructor_from_marker(name) {
                        return Some(Type::Named(constructor, Vec::new()));
                    }
                    return Some(Type::Named(name.clone(), Vec::new()));
                }
                if let Some(instance) = self.nominal_instances.get(name) {
                    let arguments = instance
                        .key
                        .arguments
                        .iter()
                        .map(|argument| self.source_type_for_ty(argument))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(instance.key.template.clone(), arguments))
                } else if self.abstract_type_parameters.contains_key(name)
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                {
                    Some(Type::Named(name.clone(), Vec::new()))
                } else {
                    None
                }
            }
            Ty::Function(function) => Some(Type::Function {
                groups: function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|ty| self.source_type_for_ty(ty))
                            .collect::<Option<Vec<_>>>()
                    })
                    .collect::<Option<Vec<_>>>()?,
                effects: FunctionEffects {
                    unsafe_effect: function.unsafe_effect,
                    throws: function
                        .throws_error
                        .as_deref()
                        .and_then(|error| self.source_type_for_ty(error))
                        .map(Box::new),
                    custom: effect_identity_sources(&function.custom_effects),
                    parameters: Vec::new(),
                },
                result: Box::new(self.source_type_for_ty(&function.result)?),
            }),
            Ty::Callable(callable) => {
                self.source_type_for_ty(&Ty::Function(callable.signature.clone()))
            }
            Ty::Continuation { input, output } => Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![
                    self.source_type_for_ty(input)?,
                    self.source_type_for_ty(output)?,
                ],
            )),
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => Some(Type::Named(
                self.lang_item_name(LangItemKind::EffectCallable).to_owned(),
                vec![
                    self.source_type_for_ty(input)?,
                    self.source_type_for_ty(output)?,
                    self.source_type_for_ty(answer)?,
                ],
            )),
            Ty::EffectRow {
                unsafe_effect,
                throws_error,
                custom_effects,
            } => Some(effect_row_source(
                *unsafe_effect,
                throws_error
                    .as_deref()
                    .and_then(|error| self.source_type_for_ty(error)),
                custom_effects,
            )),
            Ty::Never | Ty::Error => None,
        }
    }

    /// Render an internal type using source-level names for diagnostics.
    ///
    /// Concrete generic nominals use canonical `$mono$type$...` names in HIR
    /// and layout maps. Those names are intentionally stable for compiler
    /// identity, but they are not part of Salicin's user-facing syntax.
    fn diagnostic_type_name(&self, ty: &Ty) -> String {
        match ty {
            Ty::I32 => "i32".to_owned(),
            Ty::I64 => "i64".to_owned(),
            Ty::U32 => "u32".to_owned(),
            Ty::U64 => "u64".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Array(element, length) => {
                format!("Array({}, {length})", self.diagnostic_type_name(element))
            }
            Ty::Pointer { pointee, mutable } => format!(
                "{}({})",
                if *mutable { "MutPtr" } else { "Ptr" },
                self.diagnostic_type_name(pointee)
            ),
            Ty::Reference {
                pointee,
                mutable,
                region,
            } => {
                let mode = if *mutable { "borrow(mut)" } else { "borrow" };
                let region = region
                    .as_ref()
                    .map_or_else(String::new, |region| format!("('{region})"));
                format!("{mode}{region} {}", self.diagnostic_type_name(pointee))
            }
            Ty::Struct(name) | Ty::Enum(name) => {
                if let Some(parameter) = self.abstract_type_parameters.get(name) {
                    return parameter.clone();
                }
                let Some(instance) = self.nominal_instances.get(name) else {
                    return name.clone();
                };
                if instance.key.arguments.is_empty() {
                    return instance.key.template.clone();
                }
                let arguments = instance
                    .key
                    .arguments
                    .iter()
                    .map(|argument| self.diagnostic_type_name(argument))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({arguments})", instance.key.template)
            }
            Ty::Never => "Never".to_owned(),
            Ty::Error => "<error>".to_owned(),
            Ty::Function(function) => {
                let mut rendered = String::new();
                for group in &function.groups {
                    rendered.push('(');
                    rendered.push_str(
                        &group
                            .iter()
                            .map(|parameter| self.diagnostic_type_name(parameter))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    rendered.push(')');
                }
                rendered.push_str(": ");
                rendered.push_str(&self.diagnostic_type_name(&function.result));
                let mut effects = function.custom_effects.clone();
                if function.unsafe_effect {
                    effects.insert(0, "Unsafe".to_owned());
                }
                if !effects.is_empty() {
                    rendered.push_str(" with(");
                    rendered.push_str(&effects.join(", "));
                    rendered.push(')');
                }
                rendered
            }
            Ty::Callable(callable) => {
                self.diagnostic_type_name(&Ty::Function(callable.signature.clone()))
            }
            Ty::Continuation { input, output } => format!(
                "Continuation({}, {})",
                self.diagnostic_type_name(input),
                self.diagnostic_type_name(output)
            ),
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => format!(
                "EffectCallable({}, {}, {})",
                self.diagnostic_type_name(input),
                self.diagnostic_type_name(output),
                self.diagnostic_type_name(answer)
            ),
            Ty::EffectRow { .. } => ty.to_string(),
        }
    }

    fn probe_type_argument_source(
        &self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Unit => Some(Type::Unit),
            Expr::Name(name) => substitutions.get(name).cloned().or_else(|| {
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }),
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return None;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return None;
                }
                if name == "Array" {
                    if groups.len() != 1 || groups[0].len() != 2 {
                        return None;
                    }
                    let arguments = groups[0];
                    let element =
                        self.probe_type_argument_source(&arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = arguments[1].value else {
                        return None;
                    };
                    Some(Type::Array(Box::new(element), u64::try_from(length).ok()?))
                } else {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .map(|argument| {
                            self.probe_type_argument_source(&argument.value, substitutions)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => None,
        }
    }

    fn probe_compile_argument_source(
        &self,
        parameter: &CompileParam,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match parameter.kind {
            CompileParamKind::Type => self.probe_type_argument_source(expression, substitutions),
            CompileParamKind::Access => match expression {
                Expr::Name(name) if name == "shared" => {
                    Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "mut" => {
                    Some(Type::Named(ACCESS_MUT_MARKER.to_owned(), Vec::new()))
                }
                _ => None,
            },
            CompileParamKind::Passing => match expression {
                Expr::Name(name) if name == "auto" => {
                    Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "copy" => {
                    Some(Type::Named(PASSING_COPY_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "move" => {
                    Some(Type::Named(PASSING_MOVE_MARKER.to_owned(), Vec::new()))
                }
                _ => None,
            },
            CompileParamKind::Effect => match expression {
                Expr::Name(name) if name == "pure" => Some(effect_row_source(false, None, &[])),
                Expr::Name(name) if name == self.lang_item_name(LangItemKind::UnsafeEffect) => {
                    Some(effect_row_source(true, None, &[]))
                }
                Expr::Name(name) if self.effects.contains(name) => {
                    Some(effect_row_source(false, None, std::slice::from_ref(name)))
                }
                Expr::Name(name) if effect_row_from_marker(name).is_some() => {
                    Some(Type::Named(name.clone(), Vec::new()))
                }
                Expr::Call(callee, arguments)
                    if matches!(
                        callee.as_ref(),
                        Expr::Name(name)
                            if name == self.lang_item_name(LangItemKind::UnsafeEffect)
                                && arguments.is_empty()
                    ) =>
                {
                    Some(effect_row_source(true, None, &[]))
                }
                Expr::Call(callee, arguments)
                    if matches!(
                        callee.as_ref(),
                        Expr::Name(name) if self.effects.contains(name)
                    ) =>
                {
                    let Expr::Name(name) = callee.as_ref() else {
                        unreachable!()
                    };
                    let mut source_arguments = Vec::new();
                    for argument in arguments {
                        if argument.label.is_some() {
                            return None;
                        }
                        source_arguments
                            .push(self.probe_type_argument_source(&argument.value, substitutions)?);
                    }
                    let effect = Type::Named(name.clone(), source_arguments);
                    if self.is_standard_unsafe_effect_source(&effect) {
                        Some(effect_row_source(true, None, &[]))
                    } else {
                        Some(effect_row_source(
                            false,
                            None,
                            &[source_effect_identity(&effect)],
                        ))
                    }
                }
                Expr::Call(callee, arguments)
                    if matches!(callee.as_ref(), Expr::Name(name) if effect_row_from_marker(name).is_some())
                        && arguments.len() <= 1
                        && arguments.iter().all(|argument| argument.label.is_none()) =>
                {
                    let Expr::Name(marker) = callee.as_ref() else {
                        unreachable!()
                    };
                    let error = match arguments.first() {
                        Some(argument) => {
                            Some(self.probe_type_argument_source(&argument.value, substitutions)?)
                        }
                        None => None,
                    };
                    Some(Type::Named(marker.clone(), error.into_iter().collect()))
                }
                _ => None,
            },
            CompileParamKind::TypeConstructor { parameter_count } => {
                let Expr::Name(name) = expression else {
                    return None;
                };
                let source = Type::Named(name.clone(), Vec::new());
                self.type_constructor_impl_target(&source)
                    .filter(|target| target.parameter_count == parameter_count)
                    .map(|_| source)
            }
            CompileParamKind::Region | CompileParamKind::EffectConstructor { .. } => None,
        }
    }

    fn probe_compile_argument_ty(&self, parameter: &CompileParam, source: &Type) -> Option<Ty> {
        match parameter.kind {
            CompileParamKind::TypeConstructor { parameter_count } => {
                let Type::Named(name, arguments) = source else {
                    return None;
                };
                if !arguments.is_empty() {
                    return None;
                }
                self.type_constructor_impl_target(source)
                    .filter(|target| target.parameter_count == parameter_count)
                    .map(|_| Ty::Struct(type_constructor_marker(name)))
            }
            CompileParamKind::Type
            | CompileParamKind::Access
            | CompileParamKind::Passing
            | CompileParamKind::Effect => self.probe_source_ty(source),
            CompileParamKind::Region | CompileParamKind::EffectConstructor { .. } => None,
        }
    }

    fn probe_compile_group_sources(
        &self,
        parameters: &[CompileParam],
        supplied: &[CallArg],
        substitutions: &HashMap<String, Type>,
    ) -> Option<Vec<Type>> {
        if parameters.len() != supplied.len() {
            return None;
        }
        let labeled = supplied
            .first()
            .is_some_and(|argument| argument.label.is_some());
        let mut sources = vec![None; parameters.len()];
        for (position, argument) in supplied.iter().enumerate() {
            let parameter_index = if labeled {
                let label = argument.label.as_ref()?;
                parameters
                    .iter()
                    .position(|parameter| parameter.name == *label)?
            } else {
                if argument.label.is_some() {
                    return None;
                }
                position
            };
            if sources[parameter_index].is_some() {
                return None;
            }
            let parameter = &parameters[parameter_index];
            let source =
                self.probe_compile_argument_source(parameter, &argument.value, substitutions)?;
            self.probe_compile_argument_ty(parameter, &source)?;
            sources[parameter_index] = Some(source);
        }
        sources.into_iter().collect()
    }

    fn probe_source_ty(&self, source: &Type) -> Option<Ty> {
        match source {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Unit => Some(Ty::Unit),
            Type::Function {
                groups,
                effects,
                result,
            } => {
                if !effects.parameters.is_empty() {
                    return None;
                }
                Some(Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|ty| self.probe_source_ty(ty))
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>()?,
                    unsafe_effect: self.function_effects_unsafe(effects),
                    throws_error: match effects.throws.as_deref() {
                        Some(error) => Some(Box::new(self.probe_source_ty(error)?)),
                        None => None,
                    },
                    custom_effects: self.function_effects_custom_identities(effects),
                    result: Box::new(self.probe_source_ty(result)?),
                }))
            }
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Some(Ty::Reference {
                pointee: Box::new(self.probe_source_ty(pointee)?),
                mutable: *mutable,
                region: region.clone(),
            }),
            Type::Array(element, length) => {
                Some(Ty::Array(Box::new(self.probe_source_ty(element)?), *length))
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, _) if effect_row_from_marker(name).is_some() => {
                let (unsafe_effect, throws_error, custom_effects) = effect_row_from_source(source)?;
                Some(Ty::EffectRow {
                    unsafe_effect,
                    throws_error: match throws_error.as_ref() {
                        Some(error) => Some(Box::new(self.probe_source_ty(error)?)),
                        None => None,
                    },
                    custom_effects,
                })
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && is_compile_value_marker(name) =>
            {
                Some(Ty::Struct(name.clone()))
            }
            Type::Named(name, arguments)
                if arguments.is_empty()
                    && (self.abstract_type_parameters.contains_key(name)
                        || name.starts_with("$generic$param$")) =>
            {
                Some(Ty::Struct(name.clone()))
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else {
                    None
                }
            }
            Type::Named(name, source_arguments) => {
                let (kind, expected) = if let Some(template) = self.struct_templates.get(name) {
                    (
                        NominalKind::Struct,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else if let Some(template) = self.enum_templates.get(name) {
                    (
                        NominalKind::Enum,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else {
                    return None;
                };
                if source_arguments.len() != expected {
                    return None;
                }
                let arguments = source_arguments
                    .iter()
                    .map(|argument| self.probe_source_ty(argument))
                    .collect::<Option<Vec<_>>>()?;
                let key = NominalInstanceKey {
                    kind,
                    template: name.clone(),
                    arguments,
                };
                let canonical = self
                    .nominal_instance_names
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| nominal_instance_name(&key));
                Some(match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                })
            }
            Type::NamedArgs(_, _) => None,
        }
    }

    fn probe_generic_nominal_type_head(
        &self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let (kind, compile_groups) = if let Some(template) = self.struct_templates.get(name) {
            (NominalKind::Struct, &template.compile_groups)
        } else if let Some(template) = self.enum_templates.get(name) {
            (NominalKind::Enum, &template.compile_groups)
        } else {
            return None;
        };
        if groups.len() != compile_groups.len() {
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (parameters, supplied) in compile_groups.iter().zip(groups) {
            if parameters.len() != supplied.len()
                || supplied.iter().any(|argument| argument.label.is_some())
            {
                return None;
            }
            for argument in *supplied {
                let source =
                    self.probe_type_argument_source(&argument.value, &context.type_substitutions)?;
                let ty = self.probe_source_ty(&source)?;
                source_arguments.push(source);
                arguments.push(ty);
            }
        }
        let key = NominalInstanceKey {
            kind,
            template: name.to_owned(),
            arguments,
        };
        let canonical = self
            .nominal_instance_names
            .get(&key)
            .cloned()
            .unwrap_or_else(|| nominal_instance_name(&key));
        let ty = match kind {
            NominalKind::Struct => Ty::Struct(canonical),
            NominalKind::Enum => Ty::Enum(canonical),
        };
        Some((kind, ty, Type::Named(name.to_owned(), source_arguments)))
    }

    fn probe_nominal_type_head(
        &self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            return None;
        };
        if context.shadows_top_level_name(name) {
            return None;
        }
        if groups.is_empty() {
            if self.struct_defs.contains_key(name) {
                return Some((
                    NominalKind::Struct,
                    Ty::Struct(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
            if self.enum_defs.contains_key(name) {
                return Some((
                    NominalKind::Enum,
                    Ty::Enum(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
        }
        self.probe_generic_nominal_type_head(name, &groups, context)
    }

    fn probe_enum_variant_fields(&self, source: &Type, variant: &str) -> Option<VariantFields> {
        let Type::Named(template, _) = source else {
            return None;
        };
        self.enum_templates
            .get(template)
            .or_else(|| self.enum_defs.get(template))?
            .variants
            .iter()
            .find(|candidate| candidate.name == variant)
            .map(|candidate| candidate.fields.clone())
    }

    fn unify_template_ty(
        &self,
        template: &Type,
        actual: &Ty,
        actual_source: Option<&Type>,
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        origin: &str,
    ) -> Result<bool, String> {
        let mismatch = || {
            format!("type inference constraint from {origin} does not match actual type `{actual}`")
        };
        if let Type::Named(name, arguments) = template {
            if arguments.is_empty() && compile_parameters.contains(name) {
                if let Some(previous) = inferred.get_mut(name) {
                    if previous.ty == *actual {
                        if previous.source.is_none() {
                            previous.source = actual_source
                                .cloned()
                                .or_else(|| self.source_type_for_ty(actual));
                        }
                        return Ok(false);
                    }
                    return Err(format!(
                        "conflicting inference for type parameter `{name}`: `{}` from {} conflicts with `{actual}` from {origin}",
                        previous.ty, previous.origin
                    ));
                }
                if *actual == Ty::Error || self.is_uninhabited_type(actual) {
                    return Err(format!(
                        "cannot infer type parameter `{name}` from `{actual}` in {origin}"
                    ));
                }
                inferred.insert(
                    name.clone(),
                    InferredTypeArgument {
                        ty: actual.clone(),
                        source: actual_source
                            .cloned()
                            .or_else(|| self.source_type_for_ty(actual)),
                        origin: origin.to_owned(),
                    },
                );
                return Ok(true);
            }
            if !arguments.is_empty() && compile_parameters.contains(name) {
                let (actual_template, actual_sources, actual_types) = match actual_source {
                    Some(Type::Named(actual_template, actual_sources))
                        if !actual_sources.is_empty() =>
                    {
                        let actual_types = actual_sources
                            .iter()
                            .map(|source| self.probe_source_ty(source))
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(mismatch)?;
                        (
                            actual_template.clone(),
                            actual_sources.clone(),
                            actual_types,
                        )
                    }
                    _ => {
                        let actual_name = match actual {
                            Ty::Struct(name) | Ty::Enum(name) => name,
                            _ => return Err(mismatch()),
                        };
                        let Some(instance) = self.nominal_instances.get(actual_name) else {
                            return Err(mismatch());
                        };
                        let actual_sources = instance
                            .key
                            .arguments
                            .iter()
                            .map(|argument| self.source_type_for_ty(argument))
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(mismatch)?;
                        (
                            instance.key.template.clone(),
                            actual_sources,
                            instance.key.arguments.clone(),
                        )
                    }
                };
                if actual_sources.len() != arguments.len() {
                    return Err(mismatch());
                }
                let selected = InferredTypeArgument {
                    ty: Ty::Struct(type_constructor_marker(&actual_template)),
                    source: Some(Type::Named(actual_template.clone(), Vec::new())),
                    origin: origin.to_owned(),
                };
                match inferred.get(name) {
                    Some(previous) if previous.ty != selected.ty => {
                        return Err(format!(
                            "conflicting inference for type-constructor parameter `{name}` from {} and {origin}",
                            previous.origin
                        ));
                    }
                    Some(_) => {}
                    None => {
                        inferred.insert(name.clone(), selected);
                    }
                }
                let mut changed = false;
                for ((template_argument, actual_ty), actual_source) in
                    arguments.iter().zip(&actual_types).zip(&actual_sources)
                {
                    changed |= self.unify_template_ty(
                        template_argument,
                        actual_ty,
                        Some(actual_source),
                        compile_parameters,
                        inferred,
                        origin,
                    )?;
                }
                return Ok(changed);
            }
        }

        match template {
            Type::I32 => (*actual == Ty::I32).then_some(false).ok_or_else(mismatch),
            Type::I64 => (*actual == Ty::I64).then_some(false).ok_or_else(mismatch),
            Type::U32 => (*actual == Ty::U32).then_some(false).ok_or_else(mismatch),
            Type::U64 => (*actual == Ty::U64).then_some(false).ok_or_else(mismatch),
            Type::Bool => (*actual == Ty::Bool).then_some(false).ok_or_else(mismatch),
            Type::Unit => (*actual == Ty::Unit).then_some(false).ok_or_else(mismatch),
            Type::Borrow {
                mutable,
                access,
                region,
                pointee,
            } => {
                let Ty::Reference {
                    pointee: actual_pointee,
                    mutable: actual_mutable,
                    region: actual_region,
                } = actual
                else {
                    return Err(mismatch());
                };
                let mut changed = false;
                if let Some(access) = access {
                    let marker = if *actual_mutable {
                        ACCESS_MUT_MARKER
                    } else {
                        ACCESS_SHARED_MARKER
                    };
                    let selected = InferredTypeArgument {
                        ty: Ty::Struct(marker.to_owned()),
                        source: Some(Type::Named(marker.to_owned(), Vec::new())),
                        origin: origin.to_owned(),
                    };
                    match inferred.get(access) {
                        Some(previous)
                            if previous.origin != "default shared access"
                                && previous.ty != selected.ty =>
                        {
                            return Err(format!(
                                "conflicting inference for access parameter `{access}`: `{}` from {} conflicts with `{}` from {origin}",
                                if previous.ty == Ty::Struct(ACCESS_MUT_MARKER.to_owned()) {
                                    "mut"
                                } else {
                                    "shared"
                                },
                                previous.origin,
                                if *actual_mutable { "mut" } else { "shared" }
                            ));
                        }
                        Some(previous) if previous.ty == selected.ty => {}
                        _ => {
                            inferred.insert(access.clone(), selected);
                            changed = true;
                        }
                    }
                } else if mutable != actual_mutable {
                    return Err(mismatch());
                }
                if region != actual_region {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    pointee,
                    actual_pointee,
                    match actual_source {
                        Some(Type::Borrow { pointee, .. }) => Some(pointee),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
                .map(|pointee_changed| changed || pointee_changed)
            }
            Type::Array(element, length) => {
                let Ty::Array(actual_element, actual_length) = actual else {
                    return Err(mismatch());
                };
                if length != actual_length {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    element,
                    actual_element,
                    match actual_source {
                        Some(Type::Array(element, _)) => Some(element),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
            }
            Type::Function {
                groups,
                effects,
                result,
            } => {
                let actual_function = match actual {
                    Ty::Function(function) => function,
                    Ty::Callable(callable) => &callable.signature,
                    _ => return Err(mismatch()),
                };
                if groups.len() != actual_function.groups.len()
                    || groups
                        .iter()
                        .zip(&actual_function.groups)
                        .any(|(left, right)| left.len() != right.len())
                {
                    return Err(mismatch());
                }
                let (throws_changed, selected_throws) = match (
                    effects.throws.as_deref(),
                    actual_function.throws_error.as_deref(),
                ) {
                    (None, None) => (false, None),
                    (None, Some(actual_error)) if !effects.parameters.is_empty() => {
                        (false, Some(actual_error.clone()))
                    }
                    (Some(template_error), Some(actual_error)) => (
                        self.unify_template_ty(
                            template_error,
                            actual_error,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        )?,
                        None,
                    ),
                    _ => return Err(mismatch()),
                };
                let template_unsafe = self.function_effects_unsafe(effects);
                let fixed_custom = self.function_effects_custom_identities(effects);
                if effects.parameters.is_empty()
                    && ((actual_function.unsafe_effect && !template_unsafe)
                        || actual_function
                            .custom_effects
                            .iter()
                            .any(|effect| !fixed_custom.contains(effect)))
                {
                    return Err(mismatch());
                }
                let mut changed = throws_changed;
                let selected_unsafe = actual_function.unsafe_effect && !template_unsafe;
                let selected_custom = actual_function
                    .custom_effects
                    .iter()
                    .filter(|effect| !fixed_custom.contains(*effect))
                    .cloned()
                    .collect::<Vec<_>>();
                for parameter in &effects.parameters {
                    let source_error = selected_throws
                        .as_ref()
                        .and_then(|error| self.source_type_for_ty(error));
                    if selected_throws.is_some() && source_error.is_none() {
                        return Err(format!(
                            "cannot preserve the thrown error type while inferring effect parameter `{parameter}` from {origin}"
                        ));
                    }
                    let source = effect_row_source(selected_unsafe, source_error, &selected_custom);
                    let selected = InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: selected_unsafe,
                            throws_error: selected_throws.clone().map(Box::new),
                            custom_effects: selected_custom.clone(),
                        },
                        source: Some(source),
                        origin: origin.to_owned(),
                    };
                    match inferred.get(parameter) {
                        Some(previous)
                            if previous.origin != "default pure effect"
                                && previous.ty != selected.ty =>
                        {
                            return Err(format!(
                                "conflicting inference for effect parameter `{parameter}` from {} and {origin}",
                                previous.origin
                            ));
                        }
                        Some(previous) if previous.ty == selected.ty => {}
                        _ => {
                            inferred.insert(parameter.clone(), selected);
                            changed = true;
                        }
                    }
                }
                let actual_source_function = match actual_source {
                    Some(Type::Function { groups, result, .. }) => Some((groups, result.as_ref())),
                    _ => None,
                };
                for (group_index, (templates, actuals)) in
                    groups.iter().zip(&actual_function.groups).enumerate()
                {
                    for (parameter_index, (template, actual)) in
                        templates.iter().zip(actuals).enumerate()
                    {
                        let source = actual_source_function
                            .and_then(|(groups, _)| groups.get(group_index))
                            .and_then(|group| group.get(parameter_index));
                        changed |= self.unify_template_ty(
                            template,
                            actual,
                            source,
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                }
                let actual_logical_result = if actual_function.throws_error.is_some() {
                    self.standard_fallible_info_for_ty(&actual_function.result)
                        .map(|info| info.payload)
                        .ok_or_else(mismatch)?
                } else {
                    (*actual_function.result).clone()
                };
                let actual_logical_source = if actual_function.throws_error.is_some() {
                    actual_source_function.and_then(|(_, result)| match result {
                        Type::Named(_, arguments) if arguments.len() == 2 => arguments.first(),
                        _ => None,
                    })
                } else {
                    actual_source_function.map(|(_, result)| result)
                };
                changed |= self.unify_template_ty(
                    result,
                    &actual_logical_result,
                    actual_logical_source,
                    compile_parameters,
                    inferred,
                    origin,
                )?;
                Ok(changed)
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => {
                if *actual == Ty::Unit {
                    Ok(false)
                } else {
                    Err(mismatch())
                }
            }
            Type::Named(name, arguments)
                if matches!(name.as_str(), "Ptr" | "MutPtr") && arguments.len() == 1 =>
            {
                let Ty::Pointer { pointee, mutable } = actual else {
                    return Err(mismatch());
                };
                if *mutable != (name == "MutPtr") {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    &arguments[0],
                    pointee,
                    match actual_source {
                        Some(Type::Named(actual_name, actual_arguments))
                            if actual_name == name && actual_arguments.len() == 1 =>
                        {
                            Some(&actual_arguments[0])
                        }
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
            }
            Type::Named(name, arguments) => {
                let (actual_kind, actual_name) = match actual {
                    Ty::Struct(name) => (NominalKind::Struct, name),
                    Ty::Enum(name) => (NominalKind::Enum, name),
                    _ => return Err(mismatch()),
                };
                if arguments.is_empty() && name == actual_name {
                    return Ok(false);
                }
                if let Some(instance) = self.nominal_instances.get(actual_name) {
                    if instance.key.kind != actual_kind
                        || instance.key.template != *name
                        || instance.key.arguments.len() != arguments.len()
                    {
                        return Err(mismatch());
                    }
                    let actual_arguments = instance.key.arguments.clone();
                    let mut changed = false;
                    for (template, actual) in arguments.iter().zip(&actual_arguments) {
                        changed |= self.unify_template_ty(
                            template,
                            actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else if let Some(Type::Named(actual_template, source_arguments)) = actual_source {
                    if actual_template != name || source_arguments.len() != arguments.len() {
                        return Err(mismatch());
                    }
                    let mut changed = false;
                    for (template, source) in arguments.iter().zip(source_arguments) {
                        let Some(actual) = self.probe_source_ty(source) else {
                            return Err(mismatch());
                        };
                        changed |= self.unify_template_ty(
                            template,
                            &actual,
                            Some(source),
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else {
                    Err(mismatch())
                }
            }
            Type::NamedArgs(name, _) => Err(format!(
                "internal error: labeled type arguments for `{name}` were not normalized before type inference"
            )),
        }
    }

    fn unify_source_template(
        &self,
        template: &Type,
        actual: &Type,
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        origin: &str,
    ) -> Result<bool, String> {
        if let Some(actual_ty) = self.probe_source_ty(actual) {
            return self.unify_template_ty(
                template,
                &actual_ty,
                Some(actual),
                compile_parameters,
                inferred,
                origin,
            );
        }
        let mismatch = || {
            format!(
                "source type inference constraint from {origin} does not match `{}`",
                source_effect_identity(actual)
            )
        };
        match (template, actual) {
            (
                Type::Named(template_name, template_arguments),
                Type::Named(actual_name, actual_arguments),
            ) if template_name == actual_name
                && template_arguments.len() == actual_arguments.len() =>
            {
                let mut changed = false;
                for (template_argument, actual_argument) in
                    template_arguments.iter().zip(actual_arguments)
                {
                    changed |= self.unify_source_template(
                        template_argument,
                        actual_argument,
                        compile_parameters,
                        inferred,
                        origin,
                    )?;
                }
                Ok(changed)
            }
            _ => Err(mismatch()),
        }
    }

    fn resolved_template_ty(
        &self,
        template: &Type,
        compile_parameters: &HashSet<String>,
        inferred: &HashMap<String, InferredTypeArgument>,
    ) -> Option<Ty> {
        match template {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Unit => Some(Ty::Unit),
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Some(Ty::Reference {
                pointee: Box::new(self.resolved_template_ty(
                    pointee,
                    compile_parameters,
                    inferred,
                )?),
                mutable: *mutable,
                region: region.clone(),
            }),
            Type::Array(element, length) => Some(Ty::Array(
                Box::new(self.resolved_template_ty(element, compile_parameters, inferred)?),
                *length,
            )),
            Type::Function {
                groups,
                effects,
                result,
            } => {
                let mut unsafe_effect = self.function_effects_unsafe(effects);
                let mut throws_error = match effects.throws.as_deref() {
                    Some(error) => Some(Box::new(self.resolved_template_ty(
                        error,
                        compile_parameters,
                        inferred,
                    )?)),
                    None => None,
                };
                let mut custom_effects = self.function_effects_custom_identities(effects);
                for parameter in &effects.parameters {
                    let Ty::EffectRow {
                        unsafe_effect: selected_unsafe,
                        throws_error: selected_throws,
                        custom_effects: selected_custom,
                    } = &inferred.get(parameter)?.ty
                    else {
                        return None;
                    };
                    if let Some(selected_throws) = selected_throws {
                        if throws_error
                            .as_ref()
                            .is_some_and(|fixed| **fixed != **selected_throws)
                        {
                            return None;
                        }
                        throws_error = Some(selected_throws.clone());
                    }
                    if custom_effects
                        .iter()
                        .any(|effect| selected_custom.contains(effect))
                    {
                        // Duplicate row members are normalized below.
                    }
                    if selected_custom.iter().any(|effect| effect.is_empty()) {
                        return None;
                    }
                    unsafe_effect |= *selected_unsafe;
                    custom_effects.extend(selected_custom.clone());
                }
                custom_effects.sort();
                custom_effects.dedup();
                Some(Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|ty| {
                                    self.resolved_template_ty(ty, compile_parameters, inferred)
                                })
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>()?,
                    unsafe_effect,
                    throws_error,
                    custom_effects,
                    result: Box::new(self.resolved_template_ty(
                        result,
                        compile_parameters,
                        inferred,
                    )?),
                }))
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && compile_parameters.contains(name) =>
            {
                inferred.get(name).map(|argument| argument.ty.clone())
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, arguments) => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        self.resolved_template_ty(argument, compile_parameters, inferred)
                    })
                    .collect::<Option<Vec<_>>>()?;
                if self.struct_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Struct,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Struct(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if self.enum_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Enum,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Enum(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if arguments.is_empty() && self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if arguments.is_empty() && self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else {
                    None
                }
            }
            Type::NamedArgs(_, _) => None,
        }
    }

    fn probe_expr_ty(&self, expression: &Expr, hint: Option<&Ty>, context: &LowerCtx) -> TypeProbe {
        match expression {
            Expr::Type(_) => TypeProbe::Unsupported,
            Expr::Integer(_) => hint
                .filter(|ty| ty.is_integer())
                .cloned()
                .map_or(TypeProbe::Defaultable(Ty::I32), TypeProbe::Known),
            Expr::Bool(_) => TypeProbe::Known(Ty::Bool),
            Expr::Unit => TypeProbe::Known(Ty::Unit),
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name) {
                    TypeProbe::Known(local.ty.clone())
                } else if context.has_type_parameter(name) {
                    TypeProbe::Unsupported
                } else if let Some(Some(annotation)) = self.global_annotations.get(name) {
                    TypeProbe::Known(annotation.clone())
                } else if let Some(global) = self.hir_globals.get(name) {
                    TypeProbe::Known(global.ty.clone())
                } else if let Some(signature) = self.signatures.get(name) {
                    signature
                        .function_ty()
                        .map_or(TypeProbe::Unsupported, TypeProbe::Known)
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Borrow { mutable, value, .. } => {
                let pointee_hint = match hint {
                    Some(Ty::Reference { pointee, .. }) => Some(pointee.as_ref()),
                    _ => None,
                };
                let pointee = match self.probe_expr_ty(value, pointee_hint, context) {
                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty,
                    TypeProbe::Defaultable(ty) => ty,
                    TypeProbe::Unsupported => return TypeProbe::Unsupported,
                };
                TypeProbe::Known(Ty::Reference {
                    pointee: Box::new(pointee),
                    mutable: *mutable,
                    region: match hint {
                        Some(Ty::Reference { region, .. }) => region.clone(),
                        _ => None,
                    },
                })
            }
            Expr::Unsafe(body) => self.probe_expr_ty(body, hint, context),
            Expr::Unary(operator @ (UnaryOp::Neg | UnaryOp::Not), operand) => {
                self.probe_unary_ty(*operator, operand, hint, context)
            }
            Expr::Unary(UnaryOp::Deref, operand) => {
                match self.probe_expr_ty(operand, None, context) {
                    TypeProbe::Known(Ty::Pointer { pointee, .. })
                    | TypeProbe::KnownSource(Ty::Pointer { pointee, .. }, _) => {
                        TypeProbe::Known(*pointee)
                    }
                    _ => TypeProbe::Unsupported,
                }
            }
            Expr::Binary(left, operator, right) => match operator {
                BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => TypeProbe::Known(Ty::Bool),
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Rem
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::Shl
                | BinaryOp::Shr => self.probe_arithmetic_ty(*operator, left, right, hint, context),
            },
            Expr::Coalesce(left, right) => self.probe_coalesce_ty(left, right, hint, context),
            Expr::HandlerCoalesce {
                success, fallback, ..
            } => {
                let success = self.probe_expr_ty(success, hint, context);
                if matches!(success, TypeProbe::Unsupported) {
                    self.probe_expr_ty(fallback, hint, context)
                } else {
                    success
                }
            }
            Expr::HandlerChainCall(chain) => {
                let success = self.probe_expr_ty(&chain.success, hint, context);
                if matches!(success, TypeProbe::Unsupported) {
                    self.probe_expr_ty(&chain.residual, hint, context)
                } else {
                    success
                }
            }
            Expr::Try(value) => {
                let probe = self.probe_expr_ty(value, None, context);
                let Some(info) = self.standard_fallible_info_for_probe(&probe) else {
                    return TypeProbe::Unsupported;
                };
                match info.payload_source {
                    Some(source) => TypeProbe::KnownSource(info.payload, source),
                    None => TypeProbe::Known(info.payload),
                }
            }
            Expr::Throw(_) => TypeProbe::Unsupported,
            Expr::DoBlock { body } => self.probe_expr_ty(body, hint, context),
            Expr::Array(elements) => {
                if let Some(Ty::Array(element, length)) = hint {
                    if *length != elements.len() as u64 {
                        return TypeProbe::Unsupported;
                    }
                    return TypeProbe::Known(Ty::Array(element.clone(), *length));
                }
                let Some(first) = elements.first() else {
                    return TypeProbe::Unsupported;
                };
                let first = self.probe_expr_ty(first, None, context);
                let mut exact = match &first {
                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => Some(ty.clone()),
                    TypeProbe::Defaultable(_) => None,
                    TypeProbe::Unsupported => return TypeProbe::Unsupported,
                };
                let mut probes = vec![first];
                for item in elements.iter().skip(1) {
                    let probe = self.probe_expr_ty(item, exact.as_ref(), context);
                    match &probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => {
                            if exact.as_ref().is_some_and(|exact| exact != ty) {
                                return TypeProbe::Unsupported;
                            }
                            exact.get_or_insert_with(|| ty.clone());
                        }
                        TypeProbe::Defaultable(_) => {}
                        TypeProbe::Unsupported => return TypeProbe::Unsupported,
                    }
                    probes.push(probe);
                }
                if let Some(element) = exact {
                    if probes.iter().all(|probe| match probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty == &element,
                        TypeProbe::Defaultable(ty) => ty.is_integer() && element.is_integer(),
                        TypeProbe::Unsupported => false,
                    }) {
                        TypeProbe::Known(Ty::Array(Box::new(element), elements.len() as u64))
                    } else {
                        TypeProbe::Unsupported
                    }
                } else if probes
                    .iter()
                    .all(|probe| matches!(probe, TypeProbe::Defaultable(ty) if ty == &Ty::I32))
                {
                    TypeProbe::Defaultable(Ty::Array(Box::new(Ty::I32), elements.len() as u64))
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Index { base, .. } => match self.probe_expr_ty(base, None, context) {
                TypeProbe::Known(Ty::Array(element, _))
                | TypeProbe::KnownSource(Ty::Array(element, _), _) => TypeProbe::Known(*element),
                _ => TypeProbe::Unsupported,
            },
            Expr::Member(base, member) => {
                if let Some((NominalKind::Enum, ty, source)) =
                    self.probe_nominal_type_head(base, context)
                {
                    if matches!(
                        self.probe_enum_variant_fields(&source, member),
                        Some(VariantFields::Unit)
                    ) {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
                match self.probe_expr_ty(base, None, context) {
                    TypeProbe::Known(Ty::Struct(name))
                    | TypeProbe::KnownSource(Ty::Struct(name), _) => self
                        .struct_layouts
                        .get(&name)
                        .and_then(|layout| layout.fields.iter().find(|field| field.name == *member))
                        .filter(|field| {
                            Self::access_boundary_allows(&context.origin, &field.access)
                        })
                        .map(|field| TypeProbe::Known(field.ty.clone()))
                        .unwrap_or(TypeProbe::Unsupported),
                    _ => TypeProbe::Unsupported,
                }
            }
            Expr::ChainMember(base, member) => {
                self.probe_chain_ty(base, member, None, hint, context)
            }
            Expr::Call(_, _) => self.probe_call_ty(expression, hint, context),
            Expr::StructLiteral {
                constructor,
                fields,
            } => self.probe_struct_literal_ty(constructor, fields, hint, context),
            Expr::Block(statements, tail) => {
                let mut block_context = context.clone();
                block_context.push_scope();
                for statement in statements {
                    let Stmt::Let(binding) = statement else {
                        if matches!(
                            statement,
                            Stmt::Expr(
                                Expr::Return(_)
                                    | Expr::Break(_)
                                    | Expr::Throw(_)
                                    | Expr::While { .. }
                                    | Expr::Loop { .. }
                            )
                        ) {
                            return TypeProbe::Unsupported;
                        }
                        continue;
                    };
                    let annotation = binding.annotation.as_ref().and_then(|source| {
                        let mut source = source.clone();
                        substitute_type_parameters(&mut source, &block_context.type_substitutions);
                        self.probe_source_ty(&source)
                    });
                    let value =
                        self.probe_expr_ty(&binding.value, annotation.as_ref(), &block_context);
                    let inferred = match value {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => Some(ty),
                        TypeProbe::Unsupported => None,
                    };
                    let Some(ty) = annotation.or(inferred) else {
                        continue;
                    };
                    let id = block_context.fresh_local();
                    block_context.insert_local(
                        binding.name.clone(),
                        LocalInfo {
                            id,
                            ty,
                            mutable: binding.mutable,
                            capability: LocalCapability::Owned,
                            alias: None,
                            partial: None,
                            closure: None,
                        },
                    );
                }
                tail.as_ref().map_or(TypeProbe::Known(Ty::Unit), |tail| {
                    self.probe_expr_ty(tail, hint, &block_context)
                })
            }
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                let then_ty = self.probe_expr_ty(then_branch, hint, context);
                let else_ty = self.probe_expr_ty(else_branch, hint, context);
                if then_ty == else_ty {
                    then_ty
                } else {
                    match (then_ty, else_ty) {
                        (TypeProbe::Defaultable(default), exact)
                        | (exact, TypeProbe::Defaultable(default)) => match exact {
                            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                                if default.is_integer() && ty.is_integer() =>
                            {
                                TypeProbe::Known(ty)
                            }
                            _ => TypeProbe::Unsupported,
                        },
                        (TypeProbe::Known(left), TypeProbe::KnownSource(right, source))
                        | (TypeProbe::KnownSource(right, source), TypeProbe::Known(left))
                            if left == right =>
                        {
                            TypeProbe::KnownSource(left, source)
                        }
                        _ => TypeProbe::Unsupported,
                    }
                }
            }
            Expr::Assign(_, _)
            | Expr::CompoundAssign(_, _, _)
            | Expr::Closure(_, _)
            | Expr::If { .. }
            | Expr::Return(_)
            | Expr::While { .. }
            | Expr::Loop { .. }
            | Expr::Break(_)
            | Expr::Continue
            | Expr::Match { .. } => TypeProbe::Unsupported,
        }
    }

    fn probe_struct_literal_ty(
        &self,
        constructor: &Expr,
        fields: &[CallArg],
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        if fields.iter().any(|field| field.label.is_none()) {
            return TypeProbe::Unsupported;
        }
        let mut groups = Vec::new();
        let root = flatten_call(constructor, &mut groups);
        let Expr::Name(name) = root else {
            return TypeProbe::Unsupported;
        };
        if context.shadows_top_level_name(name) {
            return TypeProbe::Unsupported;
        }
        if groups.is_empty()
            && self.struct_layouts.get(name).is_some_and(|layout| {
                layout
                    .fields
                    .iter()
                    .all(|field| Self::access_boundary_allows(&context.origin, &field.access))
                    && fields.len() == layout.fields.len()
                    && fields.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            layout.fields.iter().any(|field| field.name == *label)
                        })
                    })
            })
        {
            return TypeProbe::KnownSource(
                Ty::Struct(name.clone()),
                Type::Named(name.clone(), Vec::new()),
            );
        }
        if self.struct_templates.contains_key(name) {
            if let Some((NominalKind::Struct, ty, source)) =
                self.probe_generic_nominal_type_head(name, &groups, context)
            {
                let template = &self.struct_templates[name];
                if self.source_fields_are_accessible(name, &template.fields, &context.origin)
                    && fields.len() == template.fields.len()
                    && fields.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            template.fields.iter().any(|field| field.name == *label)
                        })
                    })
                {
                    return TypeProbe::KnownSource(ty, source);
                }
            }
            if let Some(hint @ Ty::Struct(canonical)) = hint {
                if self
                    .nominal_instances
                    .get(canonical)
                    .is_some_and(|instance| {
                        instance.key.kind == NominalKind::Struct && instance.key.template == *name
                    })
                    && self.struct_layouts.get(canonical).is_some_and(|layout| {
                        layout.fields.iter().all(|field| {
                            Self::access_boundary_allows(&context.origin, &field.access)
                        }) && fields.len() == layout.fields.len()
                            && fields.iter().all(|argument| {
                                argument.label.as_ref().is_some_and(|label| {
                                    layout.fields.iter().any(|field| field.name == *label)
                                })
                            })
                    })
                {
                    if let Some(source) = self.source_type_for_ty(hint) {
                        return TypeProbe::KnownSource(hint.clone(), source);
                    }
                    return TypeProbe::Known(hint.clone());
                }
            }
        }
        TypeProbe::Unsupported
    }

    fn probe_function_candidate_call_ty(
        &self,
        canonical: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> TypeProbe {
        if let Some(signature) = self.signatures.get(canonical) {
            if groups.len() > signature.groups.len()
                || groups
                    .iter()
                    .zip(&signature.groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.throws_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafe_effect: signature.unsafe_effect,
                throws_error: signature.throws_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }

        let Some(template) = self.function_templates.get(canonical) else {
            return TypeProbe::Unsupported;
        };
        let compile_group_count = template.compile_groups.len();
        if groups.len() < compile_group_count
            || groups.len() > compile_group_count + template.groups.len()
        {
            return TypeProbe::Unsupported;
        }
        let mut substitutions = HashMap::new();
        for (parameters, supplied) in template
            .compile_groups
            .iter()
            .zip(groups.iter().take(compile_group_count))
        {
            let Some(sources) =
                self.probe_compile_group_sources(parameters, supplied, &context.type_substitutions)
            else {
                return TypeProbe::Unsupported;
            };
            for (parameter, source) in parameters.iter().zip(sources) {
                substitutions.insert(parameter.name.clone(), source);
            }
        }

        let mut function = template.clone();
        substitute_function_types(&mut function, &substitutions);
        let runtime_groups = &groups[compile_group_count..];
        if runtime_groups
            .iter()
            .zip(&function.groups)
            .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            return TypeProbe::Unsupported;
        }
        let Some(result_source) = function.return_type.clone() else {
            return TypeProbe::Unsupported;
        };
        let Some(result) = self.probe_source_ty(&result_source) else {
            return TypeProbe::Unsupported;
        };
        if runtime_groups.len() == function.groups.len() {
            if function.effects.throws.is_some() {
                let Some(info) = self.standard_fallible_info_for_ty(&result) else {
                    return TypeProbe::Unsupported;
                };
                return TypeProbe::Known(info.payload);
            }
            return TypeProbe::KnownSource(result, result_source);
        }

        let remaining = function.groups[runtime_groups.len()..]
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|parameter| self.probe_source_ty(&parameter.ty))
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>();
        if let Some(groups) = remaining {
            let throws_error = match function.effects.throws.as_deref() {
                Some(error) => {
                    let Some(error) = self.probe_source_ty(error) else {
                        return TypeProbe::Unsupported;
                    };
                    Some(Box::new(error))
                }
                None => None,
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups,
                unsafe_effect: self.function_effects_unsafe(&function.effects),
                throws_error,
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result: Box::new(result),
            }));
        }
        TypeProbe::Unsupported
    }

    fn probe_call_ty(
        &self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::ChainMember(base, member) = root {
            return self.probe_chain_ty(base, member, Some(&groups), expected, context);
        }
        if let Expr::Member(base, variant) = root {
            if let Expr::Name(target) = base.as_ref() {
                if !context.shadows_top_level_name(target)
                    && (self.struct_templates.contains_key(target)
                        || self.enum_templates.contains_key(target))
                {
                    let candidates = self.constructor_trait_associated_function_candidates(
                        target,
                        variant,
                        &context.origin,
                    );
                    let canonical = match candidates.as_slice() {
                        [canonical] => Some(canonical.clone()),
                        [_, _, ..]
                            if groups
                                .iter()
                                .flat_map(|group| group.iter())
                                .any(|argument| argument.label.is_some()) =>
                        {
                            let matches = self.matching_function_overloads(&candidates, &groups, 0);
                            match matches.as_slice() {
                                [selected] => Some(selected.clone()),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(canonical) = canonical {
                        return self.probe_function_candidate_call_ty(&canonical, &groups, context);
                    }
                }
            }
            let Some((NominalKind::Enum, ty, source)) = self.probe_nominal_type_head(base, context)
            else {
                return TypeProbe::Unsupported;
            };
            let Some(fields) = self.probe_enum_variant_fields(&source, variant) else {
                return TypeProbe::Unsupported;
            };
            if !self.source_variant_fields_are_accessible(&source, &fields, &context.origin) {
                return TypeProbe::Unsupported;
            }
            let valid = match fields {
                VariantFields::Unit => false,
                VariantFields::Positional(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| argument.label.is_none())
                }
                VariantFields::Named(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                fields.iter().any(|field| field.name == *label)
                            })
                        })
                }
            };
            return if valid {
                TypeProbe::KnownSource(ty, source)
            } else {
                TypeProbe::Unsupported
            };
        }
        let Expr::Name(name) = root else {
            return TypeProbe::Unsupported;
        };
        if let Some(local) = context.lookup(name) {
            let function = match &local.ty {
                Ty::Function(function) => function,
                Ty::Callable(callable) => &callable.signature,
                _ => return TypeProbe::Unsupported,
            };
            if groups.len() > function.groups.len()
                || groups
                    .iter()
                    .zip(&function.groups)
                    .any(|(arguments, parameters)| {
                        arguments.len() != parameters.len()
                            || arguments.iter().any(|argument| argument.label.is_some())
                    })
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == function.groups.len() {
                if function.throws_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&function.result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known((*function.result).clone());
            }
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: function.groups[groups.len()..].to_vec(),
                unsafe_effect: function.unsafe_effect,
                throws_error: function.throws_error.clone(),
                custom_effects: function.custom_effects.clone(),
                result: function.result.clone(),
            }));
        }
        if let Some(candidates) = self.function_overloads.get(name) {
            if !groups
                .iter()
                .flat_map(|group| group.iter())
                .any(|argument| argument.label.is_some())
            {
                return TypeProbe::Unsupported;
            }
            let matches = self.matching_function_overloads(candidates, &groups, 0);
            let [selected] = matches.as_slice() else {
                return TypeProbe::Unsupported;
            };
            let Some(signature) = self.signatures.get(selected) else {
                return TypeProbe::Unsupported;
            };
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.throws_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafe_effect: signature.unsafe_effect,
                throws_error: signature.throws_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }
        if context.shadows_top_level_name(name) {
            return TypeProbe::Unsupported;
        }
        if let Some(template) = self.struct_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() == compile_group_count + 1
                && self.source_fields_are_accessible(name, &template.fields, &context.origin)
            {
                let value_arguments = groups[compile_group_count];
                let labeled = value_arguments
                    .iter()
                    .filter(|argument| argument.label.is_some())
                    .count();
                let valid_fields = if labeled == 0 {
                    value_arguments.len() == template.fields.len()
                } else if labeled == value_arguments.len() {
                    value_arguments.len() == template.fields.len()
                        && value_arguments.iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                template.fields.iter().any(|field| field.name == *label)
                            })
                        })
                } else {
                    false
                };
                if valid_fields {
                    if let Some((NominalKind::Struct, ty, source)) = self
                        .probe_generic_nominal_type_head(
                            name,
                            &groups[..compile_group_count],
                            context,
                        )
                    {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
            }
        }
        if let Some(template) = self.function_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() >= compile_group_count
                && groups.len() <= compile_group_count + template.groups.len()
            {
                let mut substitutions = HashMap::new();
                let mut valid = true;
                for (parameters, supplied) in template
                    .compile_groups
                    .iter()
                    .zip(groups.iter().take(compile_group_count))
                {
                    let Some(sources) = self.probe_compile_group_sources(
                        parameters,
                        supplied,
                        &context.type_substitutions,
                    ) else {
                        valid = false;
                        break;
                    };
                    for (parameter, source) in parameters.iter().zip(sources) {
                        substitutions.insert(parameter.name.clone(), source);
                    }
                }
                let runtime_groups = &groups[compile_group_count..];
                valid &= runtime_groups
                    .iter()
                    .zip(&template.groups)
                    .all(|(arguments, parameters)| arguments.len() == parameters.len());
                if valid {
                    let Some(mut result_source) = template.return_type.clone() else {
                        return TypeProbe::Unsupported;
                    };
                    substitute_type_parameters(&mut result_source, &substitutions);
                    let Some(result) = self.probe_source_ty(&result_source) else {
                        return TypeProbe::Unsupported;
                    };
                    if runtime_groups.len() == template.groups.len() {
                        if template.effects.throws.is_some() {
                            let Some(info) = self.standard_fallible_info_for_ty(&result) else {
                                return TypeProbe::Unsupported;
                            };
                            return TypeProbe::Known(info.payload);
                        }
                        return TypeProbe::KnownSource(result, result_source);
                    }
                    let remaining = template.groups[runtime_groups.len()..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| {
                                    let mut source = parameter.ty.clone();
                                    substitute_type_parameters(&mut source, &substitutions);
                                    self.probe_source_ty(&source)
                                })
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>();
                    if let Some(groups) = remaining {
                        let throws_error = match template.effects.throws.as_deref() {
                            Some(error) => {
                                let Some(error) = self.probe_source_ty(error) else {
                                    return TypeProbe::Unsupported;
                                };
                                Some(Box::new(error))
                            }
                            None => None,
                        };
                        return TypeProbe::Known(Ty::Function(FunctionTy {
                            groups,
                            unsafe_effect: self.function_effects_unsafe(&template.effects),
                            throws_error,
                            custom_effects: self
                                .function_effects_custom_identities(&template.effects),
                            result: Box::new(result),
                        }));
                    }
                }
            }
        }
        if let Some(signature) = self.signatures.get(name) {
            if groups.len() > signature.groups.len()
                || groups
                    .iter()
                    .zip(&signature.groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.throws_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafe_effect: signature.unsafe_effect,
                throws_error: signature.throws_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }
        if self.struct_layouts.get(name).is_some_and(|layout| {
            layout
                .fields
                .iter()
                .all(|field| Self::access_boundary_allows(&context.origin, &field.access))
        }) && groups.len() == 1
        {
            return TypeProbe::Known(Ty::Struct(name.clone()));
        }
        TypeProbe::Unsupported
    }

    fn seed_type_argument_inference(
        &mut self,
        owner: &str,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> Option<(
        HashSet<String>,
        HashMap<String, InferredTypeArgument>,
        usize,
    )> {
        let compile_parameters: HashSet<_> = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::Type | CompileParamKind::TypeConstructor { .. }
                )
            })
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut inferred = HashMap::new();
        let mut compile_index = 0;
        let mut source_index = 0;
        while compile_index < compile_groups.len() && source_index < groups.len() {
            let arguments = groups[source_index];
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            let target = if labeled {
                (compile_index..compile_groups.len()).find(|index| {
                    arguments.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            compile_groups[*index]
                                .iter()
                                .any(|parameter| parameter.name == *label)
                        })
                    })
                })
            } else if !arguments.is_empty()
                && self.group_is_explicit_compile_application(
                    &compile_groups[compile_index],
                    arguments,
                    context,
                    unit_is_type,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            let parameters = &compile_groups[target];
            if !labeled && arguments.len() != parameters.len() {
                self.error(format!(
                    "type argument count mismatch in group {} of `{owner}`: expected {}, found {}",
                    target + 1,
                    parameters.len(),
                    arguments.len()
                ));
                return None;
            }
            let mut seen = HashSet::new();
            for (position, argument) in arguments.iter().enumerate() {
                let parameter = if let Some(label) = argument.label.as_deref() {
                    if !seen.insert(label) {
                        self.error(format!(
                            "duplicate compile-time argument `{label}` in `{owner}`"
                        ));
                        return None;
                    }
                    parameters
                        .iter()
                        .find(|parameter| parameter.name == label)
                        .expect("target compile group contains every argument label")
                } else {
                    &parameters[position]
                };
                let source = match parameter.kind {
                    CompileParamKind::Type => {
                        self.type_argument_from_expr(&argument.value, &context.type_substitutions)?
                    }
                    CompileParamKind::Access => match &argument.value {
                        Expr::Name(name) if name == "shared" => {
                            Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "mut" => {
                            Type::Named(ACCESS_MUT_MARKER.to_owned(), Vec::new())
                        }
                        _ => {
                            self.error(format!(
                                "invalid access argument for `{}` in `{owner}`; expected `shared` or `mut`",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Passing => match &argument.value {
                        Expr::Name(name) if name == "auto" => {
                            Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "copy" => {
                            Type::Named(PASSING_COPY_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "move" => {
                            Type::Named(PASSING_MOVE_MARKER.to_owned(), Vec::new())
                        }
                        _ => {
                            self.error(format!(
                                "invalid passing argument for `{}` in `{owner}`; expected `auto`, `copy`, or `move`",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Effect => match &argument.value {
                        Expr::Name(name) if name == "pure" => effect_row_source(false, None, &[]),
                        Expr::Name(name)
                            if name == self.lang_item_name(LangItemKind::UnsafeEffect) =>
                        {
                            effect_row_source(true, None, &[])
                        }
                        Expr::Name(name) if self.effects.contains(name) => {
                            effect_row_source(false, None, std::slice::from_ref(name))
                        }
                        Expr::Name(name) if effect_row_from_marker(name).is_some() => {
                            Type::Named(name.clone(), Vec::new())
                        }
                        Expr::Call(callee, arguments)
                            if matches!(
                                callee.as_ref(),
                                Expr::Name(name)
                                    if name == self.lang_item_name(LangItemKind::UnsafeEffect)
                                        && arguments.is_empty()
                            ) =>
                        {
                            effect_row_source(true, None, &[])
                        }
                        Expr::Call(callee, arguments)
                            if matches!(
                                callee.as_ref(),
                                Expr::Name(name) if self.effects.contains(name)
                            ) =>
                        {
                            let Expr::Name(name) = callee.as_ref() else {
                                unreachable!()
                            };
                            let mut source_arguments = Vec::new();
                            for argument in arguments {
                                if argument.label.is_some() {
                                    self.error(format!(
                                        "effect argument `{}` in `{owner}` does not support labeled constructor arguments yet",
                                        parameter.name
                                    ));
                                    return None;
                                }
                                source_arguments.push(self.type_argument_from_expr(
                                    &argument.value,
                                    &context.type_substitutions,
                                )?);
                            }
                            let effect = Type::Named(name.clone(), source_arguments);
                            if self.is_standard_unsafe_effect_source(&effect) {
                                effect_row_source(true, None, &[])
                            } else {
                                effect_row_source(false, None, &[source_effect_identity(&effect)])
                            }
                        }
                        Expr::Call(callee, arguments)
                            if matches!(callee.as_ref(), Expr::Name(name) if effect_row_from_marker(name).is_some())
                                && arguments.len() <= 1
                                && arguments.iter().all(|argument| argument.label.is_none()) =>
                        {
                            let Expr::Name(marker) = callee.as_ref() else {
                                unreachable!()
                            };
                            let error = match arguments.first() {
                                Some(argument) => Some(self.type_argument_from_expr(
                                    &argument.value,
                                    &context.type_substitutions,
                                )?),
                                None => None,
                            };
                            Type::Named(marker.clone(), error.into_iter().collect())
                        }
                        _ => {
                            self.error(format!(
                                "invalid effect argument for `{}` in `{owner}`; expected `pure`, `Unsafe`, `Throws(Error)`, or a declared custom effect",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Region => {
                        self.error("region arguments are erased before semantic analysis");
                        return None;
                    }
                    CompileParamKind::TypeConstructor { parameter_count } => {
                        let constructor = self.type_constructor_argument_from_expr(
                            &argument.value,
                            parameter_count,
                            owner,
                            &parameter.name,
                        )?;
                        Type::Named(constructor, Vec::new())
                    }
                    CompileParamKind::EffectConstructor { .. } => {
                        self.error(format!(
                            "constructor compile-time argument `{}` in `{owner}` is parsed but not supported by semantic analysis yet",
                            parameter.name
                        ));
                        return None;
                    }
                };
                let ty = if matches!(parameter.kind, CompileParamKind::TypeConstructor { .. }) {
                    let Type::Named(name, arguments) = &source else {
                        unreachable!("type constructor argument helper returns a named source")
                    };
                    debug_assert!(arguments.is_empty());
                    Ty::Struct(type_constructor_marker(name))
                } else {
                    let Some(ty) = self.probe_source_ty(&source) else {
                        self.error(format!(
                            "invalid explicit type argument for `{}` in `{owner}`",
                            parameter.name
                        ));
                        return None;
                    };
                    ty
                };
                inferred.insert(
                    parameter.name.clone(),
                    InferredTypeArgument {
                        ty,
                        source: Some(source),
                        origin: "explicit type argument".to_owned(),
                    },
                );
            }
            source_index += 1;
            compile_index = target + 1;
        }
        for parameter in compile_groups.iter().flatten() {
            if parameter.kind == CompileParamKind::Access {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Passing {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(PASSING_AUTO_MARKER.to_owned()),
                        source: Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())),
                        origin: "default automatic passing".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Effect {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: false,
                            throws_error: None,
                            custom_effects: Vec::new(),
                        },
                        source: Some(effect_row_source(false, None, &[])),
                        origin: "default pure effect".to_owned(),
                    });
            }
        }
        Some((compile_parameters, inferred, source_index))
    }

    fn probe_type_argument_inference_seed(
        &self,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> Option<(
        HashSet<String>,
        HashMap<String, InferredTypeArgument>,
        usize,
    )> {
        let compile_parameters: HashSet<_> = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::Type | CompileParamKind::TypeConstructor { .. }
                )
            })
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut inferred = HashMap::new();
        let mut compile_index = 0;
        let mut source_index = 0;
        while compile_index < compile_groups.len() && source_index < groups.len() {
            let arguments = groups[source_index];
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            let target = if labeled {
                (compile_index..compile_groups.len()).find(|index| {
                    arguments.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            compile_groups[*index]
                                .iter()
                                .any(|parameter| parameter.name == *label)
                        })
                    })
                })
            } else if !arguments.is_empty()
                && self.group_is_explicit_compile_application(
                    &compile_groups[compile_index],
                    arguments,
                    context,
                    unit_is_type,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            let parameters = &compile_groups[target];
            let sources = self.probe_compile_group_sources(
                parameters,
                arguments,
                &context.type_substitutions,
            )?;
            for (parameter, source) in parameters.iter().zip(sources) {
                let ty = self.probe_compile_argument_ty(parameter, &source)?;
                inferred.insert(
                    parameter.name.clone(),
                    InferredTypeArgument {
                        ty,
                        source: Some(source),
                        origin: "explicit type argument".to_owned(),
                    },
                );
            }
            source_index += 1;
            compile_index = target + 1;
        }
        for parameter in compile_groups.iter().flatten() {
            if parameter.kind == CompileParamKind::Access {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Passing {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(PASSING_AUTO_MARKER.to_owned()),
                        source: Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())),
                        origin: "default automatic passing".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Effect {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: false,
                            throws_error: None,
                            custom_effects: Vec::new(),
                        },
                        source: Some(effect_row_source(false, None, &[])),
                        origin: "default pure effect".to_owned(),
                    });
            }
        }
        Some((compile_parameters, inferred, source_index))
    }

    fn group_is_explicit_compile_application(
        &self,
        parameters: &[CompileParam],
        arguments: &[CallArg],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> bool {
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Type)
        {
            return arguments.iter().all(|argument| {
                (unit_is_type && matches!(argument.value, Expr::Unit))
                    || self.expression_is_explicit_type_argument(&argument.value, context)
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Access)
        {
            return arguments.iter().all(|argument| {
                matches!(&argument.value, Expr::Name(name) if name == "shared" || name == "mut")
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Passing)
        {
            return arguments.iter().all(|argument| {
                matches!(&argument.value, Expr::Name(name)
                    if matches!(name.as_str(), "auto" | "copy" | "move"))
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Effect)
        {
            return arguments
                .iter()
                .all(|argument| self.expression_is_explicit_effect_argument(&argument.value));
        }
        parameters.len() == arguments.len()
            && parameters
                .iter()
                .zip(arguments)
                .all(|(parameter, argument)| {
                    self.expression_is_explicit_compile_argument(
                        parameter,
                        &argument.value,
                        context,
                        unit_is_type,
                    )
                })
    }

    fn explicit_compile_group_prefix(
        &self,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> usize {
        let mut compile_index = 0;
        let mut source_index = 0;
        while compile_index < compile_groups.len() && source_index < groups.len() {
            let arguments = groups[source_index];
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            let target = if labeled {
                (compile_index..compile_groups.len()).find(|index| {
                    arguments.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            compile_groups[*index]
                                .iter()
                                .any(|parameter| parameter.name == *label)
                        })
                    })
                })
            } else if !arguments.is_empty()
                && self.group_is_explicit_compile_application(
                    &compile_groups[compile_index],
                    arguments,
                    context,
                    false,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            compile_index = target + 1;
            source_index += 1;
        }
        source_index
    }

    fn expression_is_explicit_compile_argument(
        &self,
        parameter: &CompileParam,
        expression: &Expr,
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> bool {
        match parameter.kind {
            CompileParamKind::Type => {
                (unit_is_type && matches!(expression, Expr::Unit))
                    || self.expression_is_explicit_type_argument(expression, context)
            }
            CompileParamKind::Access => {
                matches!(expression, Expr::Name(name) if name == "shared" || name == "mut")
            }
            CompileParamKind::Passing => {
                matches!(expression, Expr::Name(name)
                    if matches!(name.as_str(), "auto" | "copy" | "move"))
            }
            CompileParamKind::Effect => self.expression_is_explicit_effect_argument(expression),
            CompileParamKind::TypeConstructor { parameter_count } => {
                self.expression_is_explicit_type_constructor_argument(expression, parameter_count)
            }
            CompileParamKind::Region => false,
            CompileParamKind::EffectConstructor { .. } => false,
        }
    }

    fn expression_is_explicit_type_constructor_argument(
        &self,
        expression: &Expr,
        parameter_count: usize,
    ) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        let source = Type::Named(name.clone(), Vec::new());
        self.type_constructor_impl_target(&source)
            .is_some_and(|target| target.parameter_count == parameter_count)
    }

    fn expression_is_explicit_effect_argument(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => {
                name == "pure"
                    || name == self.lang_item_name(LangItemKind::UnsafeEffect)
                    || self.effects.contains(name)
                    || effect_row_from_marker(name).is_some()
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    return false;
                };
                if self.effects.contains(name) {
                    return arguments.iter().all(|argument| argument.label.is_none());
                }
                effect_row_from_marker(name).is_some()
                    && arguments.len() == 1
                    && arguments[0].label.is_none()
            }
            _ => false,
        }
    }

    fn expression_is_explicit_type_argument(&self, expression: &Expr, context: &LowerCtx) -> bool {
        match expression {
            Expr::Name(name) => {
                context.type_substitutions.contains_key(name)
                    || context.has_type_parameter(name)
                    || self.abstract_type_parameters.contains_key(name)
                    || matches!(
                        name.as_str(),
                        "i32" | "i64" | "u32" | "u64" | "bool" | "Never"
                    )
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                    || self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return false;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return false;
                }
                if name == "Array" {
                    return groups.len() == 1
                        && groups[0].len() == 2
                        && self.expression_is_explicit_type_argument(&groups[0][0].value, context)
                        && matches!(groups[0][1].value, Expr::Integer(_));
                }
                self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name)
            }
            _ => false,
        }
    }

    fn finish_type_argument_inference(
        &mut self,
        owner: &str,
        ordered_parameters: &[CompileParam],
        inferred: &HashMap<String, InferredTypeArgument>,
        unsupported_argument: bool,
    ) -> Option<(Vec<Type>, Vec<Ty>)> {
        let unresolved: Vec<_> = ordered_parameters
            .iter()
            .filter(|parameter| !inferred.contains_key(&parameter.name))
            .map(|parameter| parameter.name.clone())
            .collect();
        if !unresolved.is_empty() {
            if unsupported_argument {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}` from this argument expression; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}`; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for parameter in ordered_parameters {
            let inferred = &inferred[&parameter.name];
            let Some(source) = inferred
                .source
                .clone()
                .or_else(|| self.source_type_for_ty(&inferred.ty))
            else {
                self.error(format!(
                    "cannot use inferred type `{}` for type parameter `{}` in `{owner}`",
                    inferred.ty, parameter.name
                ));
                return None;
            };
            source_arguments.push(source);
            arguments.push(inferred.ty.clone());
        }
        Some((source_arguments, arguments))
    }

    fn infer_from_expression_constraints(
        &mut self,
        constraints: &[(Type, Expr, String)],
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        context: &LowerCtx,
    ) -> Option<bool> {
        let mut pending: Vec<_> = (0..constraints.len()).collect();
        let unsupported = loop {
            let mut progress = false;
            let mut next = Vec::new();
            let mut defaultable = Vec::new();
            for index in pending {
                let (template, expression, origin) = &constraints[index];
                let hint = self.resolved_template_ty(template, compile_parameters, inferred);
                match self.probe_expr_ty(expression, hint.as_ref(), context) {
                    TypeProbe::Known(actual) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::KnownSource(actual, source) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            Some(&source),
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::Defaultable(actual) => defaultable.push((index, actual)),
                    TypeProbe::Unsupported => next.push(index),
                }
            }
            if progress {
                next.extend(defaultable.into_iter().map(|(index, _)| index));
                pending = next;
                continue;
            }
            let mut default_progress = false;
            for (index, actual) in defaultable {
                let (template, _, origin) = &constraints[index];
                match self.unify_template_ty(
                    template,
                    &actual,
                    None,
                    compile_parameters,
                    inferred,
                    origin,
                ) {
                    Ok(changed) => default_progress |= changed,
                    Err(message) => {
                        self.error(message);
                        return None;
                    }
                }
            }
            if next.is_empty() {
                break false;
            }
            if !default_progress {
                break true;
            }
            pending = next;
        };
        Some(unsupported)
    }

    fn ensure_function_instance(
        &mut self,
        template_name: &str,
        source_arguments: Vec<Type>,
        arguments: Vec<Ty>,
    ) -> Option<String> {
        let key = FunctionInstanceKey {
            template: template_name.to_owned(),
            arguments,
        };
        if let Some(canonical) = self.function_instance_names.get(&key) {
            let info = &self.function_instances[canonical];
            debug_assert_eq!(info.key, key);
            debug_assert_eq!(info.canonical, *canonical);
            return Some(canonical.clone());
        }
        if self.function_instances.len() >= MAX_FUNCTION_INSTANCES {
            self.error(format!(
                "generic function instance limit of {MAX_FUNCTION_INSTANCES} exceeded while instantiating `{template_name}`"
            ));
            return None;
        }

        let template = self.function_templates[template_name].clone();
        let compile_parameters: Vec<_> = template.compile_groups.iter().flatten().collect();
        if compile_parameters.len() != source_arguments.len() {
            self.error(format!(
                "internal error: invalid type argument count while instantiating `{template_name}`"
            ));
            return None;
        }
        let mut substitutions = HashMap::new();
        for (parameter, argument) in compile_parameters.iter().zip(source_arguments) {
            if substitutions
                .insert(parameter.name.clone(), argument)
                .is_some()
            {
                self.error(format!(
                    "duplicate compile-time parameter `{}` in generic function `{template_name}`",
                    parameter.name
                ));
                return None;
            }
        }

        let mut function = template;
        substitute_function_types(&mut function, &substitutions);
        if !self.validate_concrete_where_predicates(template_name, &function.where_predicates) {
            return None;
        }

        let canonical = function_instance_name(&key);
        if let Some(existing) = self.function_instances.get(&canonical) {
            self.error(format!(
                "internal error: generic function instance name collision between `{}` and `{template_name}`",
                existing.key.template
            ));
            return None;
        }
        self.function_instance_names
            .insert(key.clone(), canonical.clone());
        self.function_instances.insert(
            canonical.clone(),
            FunctionInstanceInfo {
                key,
                canonical: canonical.clone(),
            },
        );
        self.function_type_substitutions
            .insert(canonical.clone(), substitutions.clone());

        function.name = canonical.clone();
        function.compile_groups.clear();
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
        let throws_error = function
            .effects
            .throws
            .as_deref()
            .map(|error| self.lower_source_type(error));
        self.signatures.insert(
            canonical.clone(),
            FunctionSig {
                groups,
                unsafe_effect: self.function_effects_unsafe(&function.effects),
                throws_error,
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result,
            },
        );
        self.functions.insert(canonical.clone(), function);
        self.function_origins.insert(
            canonical.clone(),
            self.function_template_origins[template_name].clone(),
        );
        self.function_order.push(canonical.clone());
        Some(canonical)
    }

    fn lower_expr(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let lowered = match expression {
            Expr::Type(_) => {
                self.error("compile-time type expression cannot be used as a runtime value");
                error_expr()
            }
            Expr::Integer(value) => {
                let ty = match expected {
                    Some(ty) if ty.is_integer() => ty.clone(),
                    Some(Ty::Error) => Ty::Error,
                    Some(ty) => {
                        self.error(format!(
                            "integer literal cannot be used where `{ty}` is expected"
                        ));
                        Ty::Error
                    }
                    None => Ty::I32,
                };
                if ty.is_integer() && !integer_fits(*value, &ty) {
                    self.error(format!("integer literal `{value}` does not fit in `{ty}`"));
                }
                HirExpr {
                    ty,
                    kind: HirExprKind::Integer(*value),
                }
            }
            Expr::Bool(value) => HirExpr {
                ty: Ty::Bool,
                kind: HirExprKind::Bool(*value),
            },
            Expr::Unit => HirExpr {
                ty: Ty::Unit,
                kind: HirExprKind::Unit,
            },
            Expr::Array(elements) => self.lower_array_literal(elements, expected, context),
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name).cloned() {
                    if matches!(expected, Some(Ty::Reference { .. }))
                        && matches!(local.ty, Ty::Reference { .. })
                    {
                        let place = HirPlace {
                            local: local.id,
                            root_ty: local.ty.clone(),
                            projections: Vec::new(),
                            ty: local.ty.clone(),
                            capability: LocalCapability::Owned,
                            root_mutable: local.mutable,
                            loan: None,
                            indirect: false,
                        };
                        self.access_place(place, AccessKind::Auto, context)
                    } else if local.partial.is_some() || local.closure.is_some() {
                        if matches!(&local.ty, Ty::Callable(callable) if callable.captures.iter().any(|capture| matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)))
                        {
                            self.error(format!(
                                "local callable `{name}` cannot escape while it captures a borrow"
                            ));
                            error_expr()
                        } else {
                            let place = HirPlace {
                                local: local.id,
                                root_ty: local.ty.clone(),
                                projections: Vec::new(),
                                ty: local.ty.clone(),
                                capability: local.capability,
                                root_mutable: local.mutable,
                                loan: None,
                                indirect: false,
                            };
                            self.access_place(place, AccessKind::Auto, context)
                        }
                    } else {
                        let place = self
                            .lower_place(expression, context)
                            .expect("a resolved local name is a place");
                        self.access_place(place, AccessKind::Auto, context)
                    }
                } else if context.has_type_parameter(name) {
                    self.error(format!("type parameter `{name}` cannot be used as a value"));
                    error_expr()
                } else if name == "Self" {
                    self.error("expression `Self` is only available inside an extend member");
                    error_expr()
                } else if self.globals.contains_key(name) {
                    HirExpr {
                        ty: self.global_type(name),
                        kind: HirExprKind::Global(name.clone()),
                    }
                } else if self.function_overloads.contains_key(name) {
                    self.error(format!(
                        "overloaded function `{name}` must be selected by a call with named arguments"
                    ));
                    error_expr()
                } else if self.functions.contains_key(name) {
                    HirExpr {
                        ty: self.function_type(name),
                        kind: HirExprKind::Function(name.clone()),
                    }
                } else if self.function_templates.contains_key(name) {
                    self.error(format!(
                        "generic function `{name}` requires explicit type argument groups"
                    ));
                    error_expr()
                } else if let Some((enum_name, variant)) =
                    self.resolve_short_variant(name, expected, &context.origin)
                {
                    if self
                        .enum_layouts
                        .get(&enum_name)
                        .and_then(|layout| layout.variants.get(variant))
                        .is_some_and(|variant| variant.fields.is_empty())
                    {
                        HirExpr {
                            ty: Ty::Enum(enum_name.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: enum_name,
                                variant,
                                fields: Vec::new(),
                            },
                        }
                    } else {
                        self.error(format!("variant `{name}` requires constructor arguments"));
                        error_expr()
                    }
                } else {
                    self.error(format!("unknown name `{name}`"));
                    error_expr()
                }
            }
            Expr::Borrow { mutable, value, .. } => {
                let Some(mut place) = self.lower_place(value, context) else {
                    return error_expr();
                };
                let returned_reference = expected.and_then(|expected| match expected {
                    Ty::Reference {
                        pointee,
                        mutable,
                        region,
                    } => Some(((**pointee).clone(), *mutable, region.clone())),
                    _ => None,
                });
                if let Some((pointee, expected_mutable, expected_region)) = &returned_reference {
                    self.require_same_type(&place.ty, pointee, "returned borrow pointee");
                    if *expected_mutable && !*mutable {
                        self.error(if context.reference_value_depth > 0 {
                            "borrow kind mismatch: expected mutable borrow, found shared borrow"
                        } else {
                            "cannot return a shared borrow as a mutable borrow"
                        });
                    }
                    if context.reference_value_depth == 0 {
                        match context.borrowed_parameter_regions.get(&place.local) {
                            Some((source_region, source_mutable)) => {
                                if expected_region.is_some() && source_region != expected_region {
                                    self.error(format!(
                                        "returned borrow region mismatch: expected {}, found {}",
                                        display_region(expected_region.as_deref()),
                                        display_region(source_region.as_deref())
                                    ));
                                }
                                if *expected_mutable && !source_mutable {
                                    self.error(
                                        "cannot return a mutable borrow through a shared borrow parameter",
                                    );
                                }
                            }
                            None => self.error(
                                "cannot return a borrow of a local value; returned borrows must originate from a region-bound borrow parameter",
                            ),
                        }
                    }
                }
                if *mutable {
                    self.ensure_writable(&place);
                }
                let kind = if *mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };
                let loan = self.acquire_loan(&place, kind, true, context);
                place.capability = if *mutable {
                    LocalCapability::MutParam
                } else {
                    LocalCapability::SharedParam
                };
                place.loan = loan;
                HirExpr {
                    ty: returned_reference.map_or_else(
                        || place.ty.clone(),
                        |(pointee, mutable, region)| Ty::Reference {
                            pointee: Box::new(pointee),
                            mutable,
                            region,
                        },
                    ),
                    kind: HirExprKind::Borrow {
                        place,
                        mutable: *mutable,
                    },
                }
            }
            Expr::Unsafe(body) => {
                context.unsafe_depth += 1;
                let result = self.lower_expr(body, expected, context);
                context.unsafe_depth -= 1;
                result
            }
            Expr::Unary(operator, operand) => {
                if *operator == UnaryOp::Deref {
                    let pointer = self.lower_expr(operand, None, context);
                    if context.unsafe_depth == 0 {
                        self.error("raw pointer dereference requires an `unsafe` block");
                        return error_expr();
                    }
                    let Ty::Pointer { pointee, .. } = &pointer.ty else {
                        self.error(format!(
                            "unary `*` requires a raw pointer, found `{}`",
                            pointer.ty
                        ));
                        return error_expr();
                    };
                    let pointee = (**pointee).clone();
                    if !self.is_copy_type(&pointee) {
                        self.error(format!(
                            "raw pointer reads require a Copy pointee in the first version, found `{}`",
                            self.diagnostic_type_name(&pointee)
                        ));
                        return error_expr();
                    }
                    return HirExpr {
                        ty: pointee,
                        kind: HirExprKind::RawLoad(Box::new(pointer)),
                    };
                }
                if let Some(operator_trait) = unary_operator_trait(*operator) {
                    let operand_probe = self.probe_expr_ty(operand, None, context);
                    if let Some(receiver) = Self::nominal_ty_from_probe(&operand_probe) {
                        return self.lower_trait_unary(
                            operator_trait,
                            operand,
                            &receiver,
                            expected,
                            context,
                        );
                    }
                }
                if *operator == UnaryOp::Neg {
                    if let Expr::Integer(value) = operand.as_ref() {
                        let ty = match expected {
                            Some(ty) if ty.is_signed() => ty.clone(),
                            Some(Ty::Error) => Ty::Error,
                            Some(ty) => {
                                self.error(format!(
                                    "negative integer literal cannot be used where `{ty}` is expected"
                                ));
                                Ty::Error
                            }
                            None => Ty::I32,
                        };
                        if ty.is_signed()
                            && value
                                .checked_neg()
                                .is_none_or(|negative| !integer_fits(negative, &ty))
                        {
                            self.error(format!(
                                "negative integer literal `-{value}` does not fit in `{ty}`"
                            ));
                        }
                        return HirExpr {
                            ty: ty.clone(),
                            kind: HirExprKind::Unary(
                                UnaryOp::Neg,
                                Box::new(HirExpr {
                                    ty,
                                    kind: HirExprKind::Integer(*value),
                                }),
                            ),
                        };
                    }
                }
                let operand_expected = match operator {
                    UnaryOp::Not => Some(Ty::Bool),
                    UnaryOp::Neg => expected.filter(|ty| ty.is_integer()).cloned(),
                    UnaryOp::Deref => unreachable!(),
                };
                let operand = self.lower_expr(operand, operand_expected.as_ref(), context);
                let ty = match operator {
                    UnaryOp::Not => {
                        self.require_same_type(&operand.ty, &Ty::Bool, "operand of `!`");
                        Ty::Bool
                    }
                    UnaryOp::Neg => {
                        if !operand.ty.is_integer() || !operand.ty.is_signed() {
                            self.error(format!(
                                "unary `-` requires a signed integer, found `{}`",
                                operand.ty
                            ));
                            Ty::Error
                        } else {
                            operand.ty.clone()
                        }
                    }
                    UnaryOp::Deref => unreachable!(),
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::Unary(*operator, Box::new(operand)),
                }
            }
            Expr::Binary(left, operator, right) => {
                self.lower_binary(left, *operator, right, expected, context)
            }
            Expr::Coalesce(left, right) => self.lower_coalesce(left, right, expected, context),
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => self
                .lower_handler_coalesce(scrutinee, payload, success, fallback, expected, context),
            Expr::HandlerChainCall(chain) => self.lower_handler_chain_call(
                &chain.scrutinee,
                &chain.payload,
                &chain.error,
                &chain.member,
                &chain.groups,
                &chain.success,
                &chain.residual,
                expected,
                context,
            ),
            Expr::Try(value) => self.lower_try(value, expected, context),
            Expr::DoBlock { body } => self.lower_do_block(body, expected, context),
            Expr::Throw(value) => self.lower_throw(value, context),
            Expr::Assign(place, value) => {
                if let Expr::Unary(UnaryOp::Deref, pointer) = place.as_ref() {
                    let pointer = self.lower_expr(pointer, None, context);
                    if context.unsafe_depth == 0 {
                        self.error("raw pointer assignment requires an `unsafe` block");
                        return error_expr();
                    }
                    let Ty::Pointer { pointee, mutable } = &pointer.ty else {
                        self.error(format!(
                            "raw pointer assignment requires `MutPtr(T)`, found `{}`",
                            pointer.ty
                        ));
                        return error_expr();
                    };
                    if !*mutable {
                        self.error("cannot assign through an immutable `Ptr(T)`");
                        return error_expr();
                    }
                    let pointee = (**pointee).clone();
                    if !self.is_copy_type(&pointee) {
                        self.error(format!(
                            "raw pointer writes require a Copy pointee in the first version, found `{}`",
                            self.diagnostic_type_name(&pointee)
                        ));
                        return error_expr();
                    }
                    let value = self.lower_expr(value, Some(&pointee), context);
                    return HirExpr {
                        ty: Ty::Unit,
                        kind: HirExprKind::RawStore {
                            pointer: Box::new(pointer),
                            value: Box::new(value),
                        },
                    };
                }
                let Some(place) = self.lower_place(place, context) else {
                    return error_expr();
                };
                self.ensure_writable(&place);
                self.ensure_no_conflicting_loan(&place, AccessKind::MutBorrow, context);
                // The right-hand side observes the pre-assignment state.  In
                // particular, `x = x` must not resurrect an unavailable `x`.
                let value = self.lower_expr(value, Some(&place.ty), context);
                let assignment = self.mark_initialized(&place, context);
                let mut root = place.clone();
                root.projections.clear();
                root.ty = root.root_ty.clone();
                let root_initialized = context
                    .flow
                    .initialization_status(&self.place_leaf_keys(&root))
                    == InitializationStatus::Initialized;
                if assignment != AssignmentKind::Overwrite
                    && self.projected_place_crosses_custom_drop(&place)
                {
                    self.error(
                        "reinitializing a field through a type with custom Drop is not allowed because its destructor requires a complete value",
                    );
                }
                HirExpr {
                    ty: Ty::Unit,
                    kind: HirExprKind::Assign {
                        place,
                        value: Box::new(value),
                        assignment,
                        root_initialized,
                    },
                }
            }
            Expr::CompoundAssign(place, operator, value) => {
                self.lower_compound_assign(place, *operator, value, context)
            }
            Expr::Call(_, _) => self.lower_call(expression, expected, context),
            Expr::StructLiteral {
                constructor,
                fields,
            } => self.lower_struct_literal(constructor, fields, expected, context),
            Expr::Member(base, field) => self.lower_member(base, field, expected, context),
            Expr::ChainMember(base, field) => {
                self.lower_chain(base, field, None, expected, context)
            }
            Expr::Index { base, index } => {
                if integer_literal_value(index).is_some()
                    && self.lower_place_without_diagnostic(base, context).is_some()
                {
                    let Some(place) = self.lower_place(expression, context) else {
                        return error_expr();
                    };
                    self.access_place(place, AccessKind::Auto, context)
                } else {
                    self.lower_index(base, index, context)
                }
            }
            Expr::Block(statements, tail) => {
                context.push_scope();
                let mut lowered_statements = Vec::new();
                let mut source_statements = statements.clone();
                let mut source_tail = tail.as_deref().cloned();
                let mut statement_index = 0;
                while statement_index < source_statements.len() {
                    let statement = source_statements[statement_index].clone();
                    match &statement {
                        Stmt::Let(binding) => {
                            let specialized = if statement_index + 1 < source_statements.len() {
                                let next = match &mut source_statements[statement_index + 1] {
                                    Stmt::Let(next) => &mut next.value,
                                    Stmt::Expr(next) => next,
                                };
                                self.specialize_capturing_handler_action_binding(
                                    binding, next, context,
                                )
                            } else if let Some(tail) = source_tail.as_mut() {
                                self.specialize_capturing_handler_action_binding(
                                    binding, tail, context,
                                )
                            } else {
                                false
                            };
                            if specialized {
                                statement_index += 1;
                                continue;
                            }
                            let borrow_annotation = binding.annotation.as_ref().and_then(|ty| {
                                matches!(ty, Type::Borrow { .. })
                                    .then(|| self.lower_source_type(ty))
                            });
                            let annotation = binding
                                .annotation
                                .as_ref()
                                .map(|annotation| self.lower_source_type(annotation));
                            let callable_source = match &binding.value {
                                Expr::Name(name) => context.lookup(name).cloned().filter(|local| {
                                    local.partial.is_some() || local.closure.is_some()
                                }),
                                _ => None,
                            };
                            let value = match &binding.value {
                                Expr::Closure(params, body) => {
                                    let annotation_custom_effect_sources = binding
                                        .annotation
                                        .as_ref()
                                        .and_then(|annotation| match annotation {
                                            Type::Function { effects, .. } => Some(
                                                self.function_effects_custom_source_map(effects),
                                            ),
                                            _ => None,
                                        })
                                        .unwrap_or_default();
                                    let (declared_result, mut effects) = match annotation.as_ref() {
                                        Some(Ty::Function(function)) => (
                                            Some((*function.result).clone()),
                                            ClosureEffectContext {
                                                unsafe_depth: usize::from(function.unsafe_effect),
                                                throws_error: function
                                                    .throws_error
                                                    .as_deref()
                                                    .cloned(),
                                                custom_effects: function
                                                    .custom_effects
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                                custom_effect_sources:
                                                    annotation_custom_effect_sources,
                                                lexical_handler_effects: HashSet::new(),
                                                lexical_handler_effect_sources: HashMap::new(),
                                            },
                                        ),
                                        Some(other) => {
                                            self.error(format!(
                                                "closure binding `{}` requires a function type annotation, found `{other}`",
                                                binding.name
                                            ));
                                            (None, ClosureEffectContext::default())
                                        }
                                        None => (None, ClosureEffectContext::default()),
                                    };
                                    if is_internal_handler_closure_binding(&binding.name) {
                                        effects.lexical_handler_effects =
                                            context.lexical_handler_effects.clone();
                                        effects.lexical_handler_effect_sources =
                                            context.lexical_handler_effect_sources.clone();
                                    }
                                    self.lower_local_closure(
                                        params,
                                        body,
                                        declared_result,
                                        effects,
                                        context,
                                    )
                                }
                                Expr::Name(_) if callable_source.is_some() => {
                                    let source = callable_source
                                        .as_ref()
                                        .expect("callable source was resolved");
                                    let place = HirPlace {
                                        local: source.id,
                                        root_ty: source.ty.clone(),
                                        projections: Vec::new(),
                                        ty: source.ty.clone(),
                                        capability: source.capability,
                                        root_mutable: source.mutable,
                                        loan: None,
                                        indirect: false,
                                    };
                                    self.access_place(place, AccessKind::Move, context)
                                }
                                _ if borrow_annotation.is_some() => self
                                    .lower_reference_value_expr(
                                        &binding.value,
                                        borrow_annotation
                                            .as_ref()
                                            .expect("borrow annotation was checked"),
                                        context,
                                    ),
                                _ => self.lower_expr(&binding.value, annotation.as_ref(), context),
                            };
                            if let Some(borrow_ty) = &borrow_annotation {
                                self.require_same_type(
                                    &value.ty,
                                    borrow_ty,
                                    format_args!("borrow value of local `{}`", binding.name),
                                );
                            }
                            let ty = if matches!(value.kind, HirExprKind::LocalClosure(_)) {
                                value.ty.clone()
                            } else {
                                annotation.unwrap_or_else(|| value.ty.clone())
                            };
                            let partial = match &value.kind {
                                HirExprKind::Partial {
                                    function,
                                    consumed_groups,
                                    captures,
                                } => Some(PartialInfo {
                                    function: function.clone(),
                                    consumed_groups: *consumed_groups,
                                    capture_count: captures.len(),
                                    is_fn_once: captures
                                        .iter()
                                        .any(|capture| matches!(capture, HirArgument::Move(_))),
                                }),
                                HirExprKind::Function(function) => Some(PartialInfo {
                                    function: function.clone(),
                                    consumed_groups: 0,
                                    capture_count: 0,
                                    is_fn_once: false,
                                }),
                                HirExprKind::Read { .. } => callable_source
                                    .as_ref()
                                    .and_then(|source| source.partial.clone()),
                                _ => partial_info_for_callable(&ty),
                            };
                            let closure = match &value.kind {
                                HirExprKind::LocalClosure(closure) => Some(closure.clone()),
                                HirExprKind::Read { .. } => callable_source
                                    .as_ref()
                                    .and_then(|source| source.closure.clone()),
                                _ => closure_info_for_callable(&ty),
                            };
                            let (capability, alias) = match &value.kind {
                                HirExprKind::Borrow { mutable, .. }
                                    if matches!(ty, Ty::Reference { .. }) =>
                                {
                                    (
                                        if *mutable {
                                            LocalCapability::MutParam
                                        } else {
                                            LocalCapability::SharedParam
                                        },
                                        None,
                                    )
                                }
                                HirExprKind::Borrow { place, mutable } => (
                                    if *mutable {
                                        LocalCapability::MutParam
                                    } else {
                                        LocalCapability::SharedParam
                                    },
                                    Some(place.clone()),
                                ),
                                _ if matches!(value.ty, Ty::Reference { mutable: true, .. }) => {
                                    (LocalCapability::MutParam, None)
                                }
                                _ if matches!(value.ty, Ty::Reference { mutable: false, .. }) => {
                                    (LocalCapability::SharedParam, None)
                                }
                                _ => (LocalCapability::Owned, None),
                            };
                            let reference_origin =
                                self.reference_origin_for_hir_expr(&value, context);
                            let reference_loans =
                                self.reference_loans_for_hir_expr(&value, context);
                            if matches!(ty, Ty::Function(_))
                                && partial.is_none()
                                && closure.is_none()
                                && !matches!(&ty, Ty::Function(function) if function.custom_effects.iter().any(|effect| {
                                    context.active_custom_effects.contains(effect)
                                        && self.effect_defs.get(effect.split('(').next().unwrap_or(effect)).is_some_and(|definition| !definition.operations.is_empty())
                                }))
                            {
                                self.error(format!(
                                    "function-valued local `{}` must be a direct partial application",
                                    binding.name
                                ));
                            }
                            if partial.is_some() && binding.mutable {
                                self.error(format!(
                                    "local partial application `{}` must be immutable",
                                    binding.name
                                ));
                            }
                            if closure.as_ref().is_some_and(|closure| closure.is_fn_mut)
                                && !binding.mutable
                            {
                                self.error(format!(
                                    "FnMut closure `{}` requires a mutable binding (`let mut`)",
                                    binding.name
                                ));
                            }
                            let duplicate = context
                                .scopes
                                .last()
                                .expect("block scope")
                                .names
                                .contains_key(&binding.name);
                            if duplicate {
                                self.error(format!(
                                    "duplicate binding `{}` in the same scope",
                                    binding.name
                                ));
                            }
                            let id = context.fresh_local();
                            if !duplicate {
                                if let Some(origin) = reference_origin {
                                    context.borrowed_parameter_regions.insert(id, origin);
                                }
                                if !reference_loans.is_empty() {
                                    context.reference_loans.insert(id, reference_loans);
                                }
                                if matches!(binding.value, Expr::Closure(_, _)) && closure.is_some()
                                {
                                    context.source_closures.insert(id, binding.clone());
                                } else if let Some(source) = callable_source
                                    .as_ref()
                                    .and_then(|source| context.source_closures.get(&source.id))
                                    .cloned()
                                {
                                    context.source_closures.insert(id, source);
                                }
                                context.insert_local(
                                    binding.name.clone(),
                                    LocalInfo {
                                        id,
                                        ty: ty.clone(),
                                        mutable: binding.mutable,
                                        capability,
                                        alias,
                                        partial,
                                        closure,
                                    },
                                );
                            }
                            lowered_statements.push(HirStmt::Let(HirBinding {
                                id,
                                name: binding.name.clone(),
                                ty,
                                mutable: binding.mutable,
                                value,
                            }));
                        }
                        Stmt::Expr(expression) => {
                            lowered_statements
                                .push(HirStmt::Expr(self.lower_expr(expression, None, context)));
                        }
                    }
                    statement_index += 1;
                }
                let lowered_tail = source_tail
                    .as_ref()
                    .map(|tail| Box::new(self.lower_expr(tail, expected, context)));
                let ty = lowered_tail
                    .as_ref()
                    .map_or(Ty::Unit, |tail| tail.ty.clone());
                let escaping_loans = lowered_tail.as_ref().map_or_else(Vec::new, |tail| {
                    self.reference_loans_for_hir_expr(tail, context)
                });
                if escaping_loans.is_empty() {
                    context.pop_scope();
                } else {
                    context.pop_scope_preserving_loans(&escaping_loans);
                }
                HirExpr {
                    ty,
                    kind: HirExprKind::Block(lowered_statements, lowered_tail),
                }
            }
            Expr::Closure(_, _) => {
                self.error("closures are not supported in M0");
                error_expr()
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.lower_expr(condition, Some(&Ty::Bool), context);
                let entry_flow = context.flow.clone();
                let (then_branch, else_branch, exit_flows) = if let Some(else_ast) =
                    else_branch.as_ref()
                {
                    let (then_branch, then_flow, else_branch, else_flow) = if expected.is_some() {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, expected, &entry_flow, context);
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, expected, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    } else if is_unconstrained_integer(then_branch)
                        && !is_unconstrained_integer(else_ast)
                    {
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, None, &entry_flow, context);
                        let branch_hint = if else_branch.ty == Ty::Error
                            || self.is_uninhabited_type(&else_branch.ty)
                        {
                            None
                        } else {
                            Some(&else_branch.ty)
                        };
                        let (then_branch, then_flow) = self.lower_expr_from_flow(
                            then_branch,
                            branch_hint,
                            &entry_flow,
                            context,
                        );
                        (then_branch, then_flow, else_branch, else_flow)
                    } else {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, None, &entry_flow, context);
                        let branch_hint = if then_branch.ty == Ty::Error
                            || self.is_uninhabited_type(&then_branch.ty)
                        {
                            None
                        } else {
                            Some(&then_branch.ty)
                        };
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, branch_hint, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    };
                    (
                        then_branch,
                        Some(Box::new(else_branch)),
                        vec![then_flow, else_flow],
                    )
                } else {
                    let (then_branch, then_flow) = self.lower_expr_from_flow(
                        then_branch,
                        Some(&Ty::Unit),
                        &entry_flow,
                        context,
                    );
                    (then_branch, None, vec![then_flow, entry_flow])
                };
                context.flow = FlowState::join(&exit_flows);
                let ty = if let Some(else_branch) = &else_branch {
                    self.unify_types(&then_branch.ty, &else_branch.ty, "branches of `if`")
                } else {
                    self.require_same_type(
                        &then_branch.ty,
                        &Ty::Unit,
                        "then branch of `if` without `else`",
                    );
                    Ty::Unit
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                }
            }
            Expr::Return(value) => {
                if context.function_name.is_none() {
                    self.error("`return` may only appear in a function body");
                }
                let boundary = context.return_boundary.clone();
                let declared_result = context.declared_result.clone();
                let value = if let Some(boundary) = &boundary {
                    Some(Box::new(match value {
                        Some(value) => self.lower_return_value(value, boundary, context),
                        None => self.finish_return_value(
                            HirExpr {
                                ty: Ty::Unit,
                                kind: HirExprKind::Unit,
                            },
                            boundary,
                        ),
                    }))
                } else {
                    value.as_ref().map(|value| {
                        Box::new(self.lower_expr(value, declared_result.as_ref(), context))
                    })
                };
                let returned_ty = value.as_ref().map_or(Ty::Unit, |value| value.ty.clone());
                context.returned_types.push(returned_ty);
                context.flow.reachable = false;
                HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Return(value),
                }
            }
            Expr::While { condition, body } => self.lower_while(condition, body, context),
            Expr::Loop { body } => self.lower_loop(body, expected, context),
            Expr::Break(value) => self.lower_break(value.as_deref(), context),
            Expr::Continue => self.lower_continue(context),
            Expr::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, expected, context),
        };

        if self.is_uninhabited_type(&lowered.ty) {
            context.flow.reachable = false;
        }
        if let Some(expected) = expected {
            if context.reference_value_depth == 0
                || !reference_value_types_compatible(&lowered.ty, expected)
            {
                self.require_same_type(&lowered.ty, expected, "expression");
            }
        }
        lowered
    }

    fn lower_expr_from_flow(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        entry: &FlowState,
        context: &mut LowerCtx,
    ) -> (HirExpr, FlowState) {
        context.flow = entry.clone();
        let expression = self.lower_expr(expression, expected, context);
        (expression, context.flow.clone())
    }

    fn lower_local_closure(
        &mut self,
        source_params: &[crate::ast::Param],
        body: &Expr,
        declared_result: Option<Ty>,
        effects: ClosureEffectContext,
        outer: &mut LowerCtx,
    ) -> HirExpr {
        let mut source_groups = vec![source_params];
        let mut body = body;
        while let Expr::Closure(params, nested_body) = body {
            source_groups.push(params);
            body = nested_body;
        }
        let deferred_handler_continuation = source_params.first().is_some_and(|parameter| {
            parameter.name.starts_with("$handler$resume$value$")
                || parameter
                    .name
                    .starts_with("$handler$call$continuation$value$")
                || parameter
                    .name
                    .starts_with("$handler$closure$continuation$value$")
        });

        let mut bound: HashSet<String> = source_groups
            .iter()
            .flat_map(|group| group.iter().map(|param| param.name.clone()))
            .collect();
        let mut capture_uses = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, outer, &mut capture_uses) {
            return error_expr();
        }
        let mut reconstructed_inspections = Vec::new();
        if deferred_handler_continuation {
            let mut retained_captures = Vec::with_capacity(capture_uses.len());
            for capture in capture_uses {
                let local = outer
                    .lookup(&capture.name)
                    .expect("capture scanner only records outer locals");
                let Some(inspection) = outer.inspection_bindings.get(&local.id).cloned() else {
                    if !retained_captures
                        .iter()
                        .any(|retained: &ClosureCaptureUse| retained.name == capture.name)
                    {
                        retained_captures.push(capture);
                    }
                    continue;
                };
                let root_name = outer
                    .scopes
                    .iter()
                    .rev()
                    .flat_map(|scope| scope.names.iter())
                    .find_map(|(name, local)| (local.id == inspection.root).then_some(name.clone()))
                    .expect("inspection root remains in scope");
                reconstructed_inspections.push((capture.name, root_name.clone(), inspection));
                if !retained_captures
                    .iter()
                    .any(|capture: &ClosureCaptureUse| capture.name == root_name)
                {
                    retained_captures.push(ClosureCaptureUse {
                        name: root_name,
                        mode: ClosureCaptureMode::Move,
                    });
                }
            }
            capture_uses = retained_captures;
        }
        if deferred_handler_continuation {
            for capture in &mut capture_uses {
                if capture.name.starts_with("$handler$match$inspect$input$") {
                    capture.mode = ClosureCaptureMode::Move;
                    continue;
                }
                let Some(closure) = outer
                    .lookup(&capture.name)
                    .and_then(|local| local.closure.as_ref())
                else {
                    continue;
                };
                capture.mode = if closure.is_fn_once {
                    ClosureCaptureMode::Move
                } else if closure.is_fn_mut {
                    ClosureCaptureMode::Mutable
                } else {
                    ClosureCaptureMode::Shared
                };
            }
        }

        let function = format!("__closure.{}", self.next_closure);
        self.next_closure += 1;
        let mut context =
            LowerCtx::for_function(&function, declared_result.clone(), outer.origin.clone());
        context.unsafe_depth = effects.unsafe_depth;
        context.active_throws_error = effects.throws_error.clone();
        context.active_custom_effects = effects.custom_effects.clone();
        context.active_custom_effect_sources = effects.custom_effect_sources.clone();
        context
            .active_custom_effects
            .extend(effects.lexical_handler_effects.iter().cloned());
        context
            .active_custom_effect_sources
            .extend(effects.lexical_handler_effect_sources.clone());
        context.lexical_handler_effects = effects.lexical_handler_effects.clone();
        context.lexical_handler_effect_sources = effects.lexical_handler_effect_sources.clone();
        context.recursive_frame_calls = outer.recursive_frame_calls.clone();
        context.return_boundary = effects.throws_error.as_ref().and_then(|error| {
            declared_result
                .as_ref()
                .and_then(|result| self.throws_boundary_for_ty(result, error))
        });
        context.type_substitutions = outer.type_substitutions.clone();
        let mut hir_params = Vec::new();
        let mut captures = Vec::new();
        let mut capture_names = Vec::new();
        let mut recursive_capture_params = Vec::new();

        let is_fn_once = capture_uses
            .iter()
            .any(|capture| capture.mode == ClosureCaptureMode::Move);
        let is_fn_mut = !is_fn_once
            && capture_uses
                .iter()
                .any(|capture| capture.mode == ClosureCaptureMode::Mutable);
        for capture in capture_uses {
            let name = capture.name;
            let local = outer
                .lookup(&name)
                .cloned()
                .expect("capture scanner only records outer locals");
            if matches!(local.ty, Ty::Function(_))
                && local.partial.as_ref().is_some_and(|partial| {
                    partial.consumed_groups == 0 && partial.capture_count == 0
                })
            {
                let id = context.fresh_local();
                context.scopes[0].locals.push(id);
                context.scopes[0].names.insert(
                    name,
                    LocalInfo {
                        id,
                        ty: local.ty,
                        mutable: false,
                        capability: LocalCapability::Owned,
                        alias: None,
                        partial: local.partial,
                        closure: None,
                    },
                );
                continue;
            }
            let is_captured_partial = local
                .partial
                .as_ref()
                .is_some_and(|partial| partial.consumed_groups != 0 || partial.capture_count != 0);
            let captures_callable =
                local.closure.is_some() && capture.mode == ClosureCaptureMode::Move;
            if is_captured_partial
                || (local.closure.is_some() && !captures_callable && !deferred_handler_continuation)
            {
                self.error(format!("closure cannot capture local callable `{name}`"));
                continue;
            }
            let compatible_reborrow = matches!(
                (local.capability, capture.mode),
                (LocalCapability::SharedParam, ClosureCaptureMode::Shared)
                    | (LocalCapability::MutParam, ClosureCaptureMode::Shared)
                    | (LocalCapability::MutParam, ClosureCaptureMode::Mutable)
            );
            if local.alias.is_some()
                || (local.capability != LocalCapability::Owned && !compatible_reborrow)
            {
                self.error(format!(
                    "closure capture mode is incompatible with borrowed local `{name}`"
                ));
                continue;
            }
            match capture.mode {
                ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable
                    if !(self.is_copy_type(&local.ty)
                        || deferred_handler_continuation && local.closure.is_some()) =>
                {
                    if name.starts_with("$handler$match$input$")
                        || name.starts_with("$handler$match$inspect$input$")
                    {
                        self.error(
                            "an effectful match guard currently requires its match input to implement Copy",
                        );
                    } else {
                        self.error(format!(
                            "closure capture `{name}` must implement Copy for this capture mode"
                        ));
                    }
                    continue;
                }
                ClosureCaptureMode::Move
                    if !matches!(
                        local.ty,
                        Ty::Struct(_) | Ty::Enum(_) | Ty::Callable(_) | Ty::Continuation { .. }
                    ) =>
                {
                    self.error(format!(
                        "FnOnce move capture `{name}` must be a nominal root local for now"
                    ));
                    continue;
                }
                _ => {}
            }

            let mut place = HirPlace {
                local: local.id,
                root_ty: local.ty.clone(),
                projections: Vec::new(),
                ty: local.ty.clone(),
                capability: local.capability,
                root_mutable: local.mutable,
                loan: None,
                indirect: false,
            };
            let (parameter_mode, capability, mutable, value) = match capture.mode {
                ClosureCaptureMode::Shared => {
                    if !deferred_handler_continuation {
                        place.loan = self.acquire_loan(&place, LoanKind::Shared, true, outer);
                    }
                    (PassMode::Borrow, LocalCapability::SharedParam, false, None)
                }
                ClosureCaptureMode::Mutable => {
                    self.ensure_writable(&place);
                    if !deferred_handler_continuation {
                        place.loan = self.acquire_loan(&place, LoanKind::Mutable, true, outer);
                    }
                    (PassMode::MutBorrow, LocalCapability::MutParam, true, None)
                }
                ClosureCaptureMode::Move => {
                    let value = self.access_place(place.clone(), AccessKind::Move, outer);
                    (
                        PassMode::Move,
                        LocalCapability::Owned,
                        false,
                        Some(Box::new(value)),
                    )
                }
            };
            captures.push(ClosureCapture {
                place,
                mode: capture.mode,
                value,
                forwarded: None,
            });
            capture_names.push(name.clone());

            let id = context.fresh_local();
            let captured_closure = local.closure.clone().map(|mut closure| {
                for (index, capture) in closure.captures.iter_mut().enumerate() {
                    capture.forwarded = Some(ForwardedClosureCapture {
                        binding: id,
                        index,
                        callable_ty: local.ty.clone(),
                    });
                }
                closure
            });
            context.scopes[0].locals.push(id);
            context.scopes[0].names.insert(
                name.clone(),
                LocalInfo {
                    id,
                    ty: local.ty.clone(),
                    mutable,
                    capability,
                    alias: None,
                    partial: None,
                    closure: captured_closure,
                },
            );
            hir_params.push(HirParam {
                id,
                name: format!("capture.{name}"),
                ty: local.ty,
                mode: parameter_mode,
            });
            recursive_capture_params.push(ParamSig {
                name,
                ty: hir_params.last().expect("capture parameter").ty.clone(),
                mode: parameter_mode,
            });
        }

        for (name, root_name, inspection) in reconstructed_inspections {
            let root = context
                .lookup(&root_name)
                .cloned()
                .expect("inspection root capture exists");
            let id = context.fresh_local();
            let place = HirPlace {
                local: root.id,
                root_ty: root.ty,
                projections: inspection.path.clone(),
                ty: inspection.ty.clone(),
                capability: LocalCapability::SharedParam,
                root_mutable: false,
                loan: None,
                indirect: false,
            };
            context.scopes[0].names.insert(
                name,
                LocalInfo {
                    id,
                    ty: inspection.ty.clone(),
                    mutable: false,
                    capability: LocalCapability::SharedParam,
                    alias: Some(place),
                    partial: None,
                    closure: None,
                },
            );
            context.inspection_bindings.insert(
                id,
                InspectionBinding {
                    root: root.id,
                    path: inspection.path,
                    ty: inspection.ty,
                },
            );
        }

        let mut groups = Vec::new();
        for source_group in source_groups {
            let mut group = Vec::new();
            for param in source_group {
                let ty = self.lower_source_type(&param.ty);
                if context.scopes[0].names.contains_key(&param.name) {
                    self.error(format!("duplicate closure parameter `{}`", param.name));
                    continue;
                }
                let id = context.fresh_local();
                if matches!(param.mode, PassMode::Borrow | PassMode::MutBorrow) {
                    context.borrowed_parameter_regions.insert(
                        id,
                        (param.region.clone(), param.mode == PassMode::MutBorrow),
                    );
                }
                let capability = match param.mode {
                    PassMode::Borrow => LocalCapability::SharedParam,
                    PassMode::MutBorrow => LocalCapability::MutParam,
                    PassMode::Inferred | PassMode::Copy | PassMode::Move => LocalCapability::Owned,
                };
                context.scopes[0].locals.push(id);
                context.scopes[0].names.insert(
                    param.name.clone(),
                    LocalInfo {
                        id,
                        ty: ty.clone(),
                        mutable: param.mode == PassMode::MutBorrow,
                        capability,
                        alias: None,
                        partial: None,
                        closure: None,
                    },
                );
                group.push(ParamSig {
                    name: param.name.clone(),
                    ty: ty.clone(),
                    mode: param.mode,
                });
                hir_params.push(HirParam {
                    id,
                    name: param.name.clone(),
                    ty,
                    mode: param.mode,
                });
            }
            groups.push(group);
        }

        if let Some(result) = declared_result.clone() {
            let mut tokens = HashSet::new();
            collect_internal_recursion_tokens(body, &mut tokens);
            let parameters = groups.iter().flatten().cloned().collect::<Vec<_>>();
            for token in tokens {
                context
                    .recursive_frame_calls
                    .entry(token)
                    .or_insert_with(|| RecursiveFrameCall {
                        function: function.clone(),
                        captures: recursive_capture_params.clone(),
                        parameters: parameters.clone(),
                        result: result.clone(),
                    });
            }
        }

        let boundary = context.return_boundary.clone();
        let lowered_body = if let Some(boundary) = &boundary {
            self.lower_return_value(body, boundary, &mut context)
        } else {
            self.lower_expr(body, declared_result.as_ref(), &mut context)
        };
        let mut result = if let Some(declared) = declared_result {
            Some(declared)
        } else if self.is_uninhabited_type(&lowered_body.ty) {
            None
        } else {
            Some(lowered_body.ty.clone())
        };
        for returned in &context.returned_types {
            result = Some(match result {
                Some(current) => self.unify_types(
                    &current,
                    returned,
                    format!("return values in closure `{function}`"),
                ),
                None => returned.clone(),
            });
        }
        let result = result.unwrap_or(Ty::Unit);
        self.lifted_functions.push(HirFunction {
            name: function.clone(),
            params: hir_params,
            result: result.clone(),
            body: lowered_body,
        });

        let mut custom_effects = effects.custom_effects.into_iter().collect::<Vec<_>>();
        custom_effects.sort();
        let callable_ty = Ty::Callable(CallableTy {
            signature: FunctionTy {
                groups: groups
                    .iter()
                    .map(|group| group.iter().map(|param| param.ty.clone()).collect())
                    .collect(),
                unsafe_effect: effects.unsafe_depth > 0,
                throws_error: effects.throws_error.clone().map(Box::new),
                custom_effects: custom_effects.clone(),
                result: Box::new(result.clone()),
            },
            captures: captures
                .iter()
                .map(|capture| CallableCaptureTy {
                    ty: capture.place.ty.clone(),
                    mode: match capture.mode {
                        ClosureCaptureMode::Shared => PassMode::Borrow,
                        ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                        ClosureCaptureMode::Move => PassMode::Move,
                    },
                })
                .collect(),
            kind: CallableKind::Closure {
                function: function.clone(),
                is_fn_mut,
                is_fn_once,
            },
        });
        let info = ClosureInfo {
            function,
            groups: groups.clone(),
            unsafe_effect: effects.unsafe_depth > 0,
            throws_error: effects.throws_error,
            custom_effects,
            result: result.clone(),
            captures,
            capture_names,
            is_fn_mut,
            is_fn_once,
        };
        HirExpr {
            ty: callable_ty,
            kind: HirExprKind::LocalClosure(info),
        }
    }

    fn scan_simple_closure_captures(
        &mut self,
        expression: &Expr,
        bound: &mut HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        match expression {
            Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) => true,
            Expr::Name(name) => {
                if !bound.contains(name) && outer.lookup(name).is_some() {
                    record_closure_capture(captures, name, ClosureCaptureMode::Shared);
                }
                true
            }
            Expr::Unary(_, operand)
            | Expr::Try(operand)
            | Expr::Throw(operand)
            | Expr::Unsafe(operand)
            | Expr::DoBlock { body: operand }
            | Expr::Borrow { value: operand, .. } => {
                self.scan_simple_closure_captures(operand, bound, outer, captures)
            }
            Expr::Binary(left, _, right) | Expr::Coalesce(left, right) => {
                self.scan_simple_closure_captures(left, bound, outer, captures)
                    & self.scan_simple_closure_captures(right, bound, outer, captures)
            }
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => {
                let mut valid =
                    self.scan_simple_closure_captures(scrutinee, bound, outer, captures);
                let saved = bound.clone();
                bound.insert(payload.clone());
                valid &= self.scan_simple_closure_captures(success, bound, outer, captures);
                *bound = saved;
                valid & self.scan_simple_closure_captures(fallback, bound, outer, captures)
            }
            Expr::HandlerChainCall(chain) => {
                let mut valid =
                    self.scan_simple_closure_captures(&chain.scrutinee, bound, outer, captures);
                for argument in chain.groups.iter().flatten() {
                    valid &=
                        self.scan_simple_closure_captures(&argument.value, bound, outer, captures);
                }
                let saved = bound.clone();
                bound.insert(chain.payload.clone());
                valid &= self.scan_simple_closure_captures(&chain.success, bound, outer, captures);
                *bound = saved.clone();
                bound.insert(chain.error.clone());
                valid &= self.scan_simple_closure_captures(&chain.residual, bound, outer, captures);
                *bound = saved;
                valid
            }
            Expr::Array(elements) => elements.iter().fold(true, |valid, element| {
                self.scan_simple_closure_captures(element, bound, outer, captures) & valid
            }),
            Expr::StructLiteral { fields, .. } => fields.iter().fold(true, |valid, field| {
                self.scan_simple_closure_captures(&field.value, bound, outer, captures) & valid
            }),
            Expr::Index { base, index } => {
                self.scan_simple_closure_captures(base, bound, outer, captures)
                    & self.scan_simple_closure_captures(index, bound, outer, captures)
            }
            Expr::Assign(place, value) => {
                let mut valid = self.scan_simple_closure_captures(value, bound, outer, captures);
                if let Some(name) = place_root_name(place) {
                    if !bound.contains(name) && outer.lookup(name).is_some() {
                        if !matches!(place.as_ref(), Expr::Name(_)) {
                            self.error(
                                "FnMut closure assignment only supports a captured root local for now",
                            );
                            valid = false;
                        } else {
                            record_closure_capture(captures, name, ClosureCaptureMode::Mutable);
                        }
                    }
                }
                valid
            }
            Expr::CompoundAssign(place, _, value) => {
                let mut valid = self.scan_simple_closure_captures(value, bound, outer, captures);
                if let Some(name) = place_root_name(place) {
                    if !bound.contains(name) && outer.lookup(name).is_some() {
                        if !matches!(place.as_ref(), Expr::Name(_)) {
                            self.error(
                                "FnMut closure compound assignment only supports a captured root local for now",
                            );
                            valid = false;
                        } else {
                            record_closure_capture(captures, name, ClosureCaptureMode::Mutable);
                        }
                    }
                }
                valid
            }
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.scan_simple_closure_captures(base, bound, outer, captures)
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                if matches!(root, Expr::Name(name) if name.starts_with("$handler$tail$")) {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(
                    root,
                    Expr::Name(name)
                        if matches!(
                            name.as_str(),
                            "$handler$chain$wrap$success"
                                | "$handler$chain$wrap$residual"
                        )
                ) {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(root, Expr::Name(name) if name == "$handler$invoke$continuation") {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .collect::<Vec<_>>();
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = arguments.first()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return arguments.iter().skip(1).fold(true, |valid, argument| {
                        self.scan_simple_closure_captures(&argument.value, bound, outer, captures)
                            & valid
                    });
                }
                if matches!(root, Expr::Name(name) if name == "$handler$invoke$effect$callable") {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .collect::<Vec<_>>();
                    for index in [0, 2] {
                        if let Some(CallArg {
                            value: Expr::Name(name),
                            ..
                        }) = arguments.get(index)
                        {
                            if !bound.contains(name) && outer.lookup(name).is_some() {
                                record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            }
                        }
                    }
                    return arguments
                        .iter()
                        .skip(1)
                        .take(1)
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        });
                }
                if matches!(root, Expr::Name(name) if name == "$handler$erase$continuation") {
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = groups.iter().flat_map(|group| group.iter()).next()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return true;
                }
                if matches!(root, Expr::Name(name) if name == "$handler$erase$effect$callable") {
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = groups.iter().flat_map(|group| group.iter()).next()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return true;
                }
                if matches!(root, Expr::Name(name) if name.starts_with("$handler$recursive$")) {
                    if let Expr::Name(name) = root {
                        if let Some(frame) = outer.recursive_frame_calls.get(name) {
                            for capture in &frame.captures {
                                if outer.lookup(&capture.name).is_some()
                                    && !bound.contains(&capture.name)
                                {
                                    let mode = match capture.mode {
                                        PassMode::Borrow => ClosureCaptureMode::Shared,
                                        PassMode::MutBorrow => ClosureCaptureMode::Mutable,
                                        PassMode::Move => ClosureCaptureMode::Move,
                                        PassMode::Inferred | PassMode::Copy => {
                                            ClosureCaptureMode::Shared
                                        }
                                    };
                                    record_closure_capture(captures, &capture.name, mode);
                                }
                            }
                        }
                    }
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if let Expr::Name(name) = root {
                    if !bound.contains(name)
                        && outer
                            .lookup(name)
                            .is_some_and(|local| local.closure.is_some())
                    {
                        record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        return groups.iter().flat_map(|group| group.iter()).fold(
                            true,
                            |valid, argument| {
                                self.scan_simple_closure_captures(
                                    &argument.value,
                                    bound,
                                    outer,
                                    captures,
                                ) & valid
                            },
                        );
                    }
                }
                if matches!(root, Expr::Name(name) if bound.contains(name)) {
                    let modes = match root {
                        Expr::Name(name) => self.handler_frame_parameter_modes.get(name).cloned(),
                        _ => None,
                    };
                    let mut valid = true;
                    for (index, argument) in
                        groups.iter().flat_map(|group| group.iter()).enumerate()
                    {
                        if let Expr::Name(name) = &argument.value {
                            if !bound.contains(name) && outer.lookup(name).is_some() {
                                let mode = modes
                                    .as_ref()
                                    .and_then(|modes| modes.get(index))
                                    .copied()
                                    .unwrap_or(PassMode::Inferred);
                                let capture = match mode {
                                    PassMode::Borrow => ClosureCaptureMode::Shared,
                                    PassMode::MutBorrow => ClosureCaptureMode::Mutable,
                                    PassMode::Move => ClosureCaptureMode::Move,
                                    PassMode::Inferred | PassMode::Copy => {
                                        ClosureCaptureMode::Shared
                                    }
                                };
                                record_closure_capture(captures, name, capture);
                                continue;
                            }
                        }
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    return valid;
                }
                if let Expr::Name(name) = root {
                    if !bound.contains(name)
                        && outer
                            .lookup(name)
                            .is_some_and(|local| local.closure.is_some())
                    {
                        record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        return groups.iter().flat_map(|group| group.iter()).fold(
                            true,
                            |valid, argument| {
                                self.scan_simple_closure_captures(
                                    &argument.value,
                                    bound,
                                    outer,
                                    captures,
                                ) & valid
                            },
                        );
                    }
                }
                let captured_function = match root {
                    Expr::Name(function)
                        if !bound.contains(function)
                            && outer
                                .lookup(function)
                                .is_some_and(|local| matches!(local.ty, Ty::Function(_))) =>
                    {
                        Some(function.as_str())
                    }
                    _ => None,
                };
                if let Some(function) = captured_function {
                    record_closure_capture(captures, function, ClosureCaptureMode::Shared);
                    groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        })
                } else if let Expr::Member(base, _) = root {
                    if let Expr::Name(name) = base.as_ref() {
                        if name.starts_with("$handler$chain$payload$")
                            && !bound.contains(name)
                            && outer.lookup(name).is_some()
                        {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            return groups.iter().flat_map(|group| group.iter()).fold(
                                true,
                                |valid, argument| {
                                    self.scan_simple_closure_captures(
                                        &argument.value,
                                        bound,
                                        outer,
                                        captures,
                                    ) & valid
                                },
                            );
                        }
                    }
                    let mut valid = self.scan_simple_closure_captures(base, bound, outer, captures);
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    valid
                } else if let Expr::ChainMember(base, _) = root {
                    let mut valid = self.scan_simple_closure_captures(base, bound, outer, captures);
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    valid
                } else if matches!(root, Expr::Name(name) if self.struct_layouts.contains_key(name))
                {
                    groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        })
                } else {
                    self.scan_direct_move_closure_call(expression, bound, outer, captures)
                }
            }
            Expr::Block(statements, tail) => {
                let saved = bound.clone();
                let mut valid = true;
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            valid &= self.scan_simple_closure_captures(
                                &binding.value,
                                bound,
                                outer,
                                captures,
                            );
                            bound.insert(binding.name.clone());
                        }
                        Stmt::Expr(expression) => {
                            valid &= self
                                .scan_simple_closure_captures(expression, bound, outer, captures);
                        }
                    }
                }
                if let Some(tail) = tail {
                    valid &= self.scan_simple_closure_captures(tail, bound, outer, captures);
                }
                *bound = saved;
                valid
            }
            Expr::Closure(parameters, body) => {
                let saved = bound.clone();
                bound.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                let valid = self.scan_simple_closure_captures(body, bound, outer, captures);
                *bound = saved;
                valid
            }
            Expr::While { condition, body } => {
                self.scan_simple_closure_captures(condition, bound, outer, captures)
                    & self.scan_simple_closure_captures(body, bound, outer, captures)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut valid =
                    self.scan_simple_closure_captures(condition, bound, outer, captures);
                valid &= self.scan_simple_closure_captures(then_branch, bound, outer, captures);
                if let Some(else_branch) = else_branch {
                    valid &= self.scan_simple_closure_captures(else_branch, bound, outer, captures);
                }
                valid
            }
            Expr::Loop { body } => self.scan_simple_closure_captures(body, bound, outer, captures),
            Expr::Break(value) => value.as_ref().is_none_or(|value| {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }),
            Expr::Return(value) => value.as_ref().is_none_or(|value| {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }),
            Expr::Match { scrutinee, arms } => {
                let mut valid =
                    self.scan_simple_closure_captures(scrutinee, bound, outer, captures);
                for arm in arms {
                    let saved = bound.clone();
                    collect_pattern_binding_names(&arm.pattern, bound);
                    if let Some(guard) = &arm.guard {
                        valid &= self.scan_simple_closure_captures(guard, bound, outer, captures);
                    }
                    valid &= self.scan_simple_closure_captures(&arm.body, bound, outer, captures);
                    *bound = saved;
                }
                valid
            }
            Expr::Continue => true,
        }
    }

    fn scan_direct_move_closure_call(
        &mut self,
        expression: &Expr,
        bound: &HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(function) = root else {
            self.error("FnOnce capture requires a direct named-function call");
            return false;
        };
        if outer.has_type_parameter(function) {
            self.error(format!(
                "type parameter `{function}` is not a top-level named function"
            ));
            return false;
        }
        let selected_overload = if self.function_overloads.contains_key(function) {
            self.resolve_function_overload(function, &groups)
        } else {
            None
        };
        if self.function_overloads.contains_key(function) && selected_overload.is_none() {
            return false;
        }
        let resolved_function = selected_overload.as_deref().unwrap_or(function);
        let (signature, runtime_groups) = if let Some(signature) =
            self.signatures.get(resolved_function)
        {
            (signature.clone(), groups.as_slice())
        } else if self.function_templates.contains_key(resolved_function) {
            let Some((canonical, runtime_start)) = self.resolve_inferred_generic_function_instance(
                resolved_function,
                &groups,
                None,
                outer,
            ) else {
                return false;
            };
            (
                self.signatures[&canonical].clone(),
                &groups[runtime_start..],
            )
        } else {
            self.error(format!(
                "closure call `{function}` is not a top-level named function"
            ));
            return false;
        };
        if runtime_groups.len() != signature.groups.len() {
            self.error(format!(
                "named function `{function}` must be fully applied inside a closure"
            ));
            return false;
        }

        let mut valid = true;
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&signature.groups).enumerate()
        {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in closure call to `{function}`"
                ));
                valid = false;
            }
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) =
                self.ordered_call_arguments(function, group_index + 1, arguments, &parameter_names)
            else {
                valid = false;
                continue;
            };
            for (argument, parameter) in ordered.into_iter().zip(parameters) {
                match &argument.value {
                    Expr::Name(name) if bound.contains(name) => {}
                    Expr::Name(name) => {
                        if let Some(local) = outer.lookup(name) {
                            let mode = self.effective_pass_mode(parameter.mode, &parameter.ty);
                            if mode == PassMode::Move
                                && matches!(local.ty, Ty::Struct(_) | Ty::Enum(_))
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            } else if mode == PassMode::Copy
                                && self.is_copy_type(&local.ty)
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(captures, name, ClosureCaptureMode::Shared);
                            } else {
                                self.error(format!(
                                    "closure call capture `{name}` must match a Copy parameter or a nominal move parameter"
                                ));
                                valid = false;
                            }
                        }
                    }
                    Expr::Unit | Expr::Integer(_) | Expr::Bool(_) => {}
                    _ => {
                        self.error(
                            "closure call arguments only support literals, closure parameters, or a nominal root move capture",
                        );
                        valid = false;
                    }
                }
            }
        }
        valid
    }

    fn lower_place(&mut self, expression: &Expr, context: &mut LowerCtx) -> Option<HirPlace> {
        match expression {
            Expr::Name(name) => {
                let Some(local) = context.lookup(name).cloned() else {
                    if context.has_type_parameter(name) {
                        self.error(format!("type parameter `{name}` is not a data place"));
                    } else if self.globals.contains_key(name) {
                        self.error(format!(
                            "global constant `{name}` is not a borrowable place"
                        ));
                    } else {
                        self.error(format!("unknown local `{name}` in place expression"));
                    }
                    return None;
                };
                if let Some(alias) = local.alias {
                    return Some(alias);
                }
                if let Ty::Reference {
                    pointee, mutable, ..
                } = &local.ty
                {
                    return Some(HirPlace {
                        local: local.id,
                        root_ty: (**pointee).clone(),
                        projections: Vec::new(),
                        ty: (**pointee).clone(),
                        capability: if *mutable {
                            LocalCapability::MutParam
                        } else {
                            LocalCapability::SharedParam
                        },
                        root_mutable: *mutable,
                        loan: None,
                        indirect: true,
                    });
                }
                Some(HirPlace {
                    local: local.id,
                    root_ty: local.ty.clone(),
                    projections: Vec::new(),
                    ty: local.ty,
                    capability: local.capability,
                    root_mutable: local.mutable,
                    loan: None,
                    indirect: false,
                })
            }
            Expr::Member(base, field_name) => {
                let mut place = self.lower_place(base, context)?;
                let Ty::Struct(struct_name) = &place.ty else {
                    self.error(format!(
                        "field `{field_name}` cannot be selected on value of type `{}`",
                        place.ty
                    ));
                    return None;
                };
                let layout = self.struct_layout_or_diagnostic(struct_name)?;
                let Some((index, field)) = layout
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == *field_name)
                else {
                    self.error(format!(
                        "unknown field `{field_name}` on struct `{struct_name}`"
                    ));
                    return None;
                };
                if !self.require_field_access(struct_name, field, &context.origin) {
                    return None;
                }
                place.projections.push(index);
                place.ty = field.ty.clone();
                Some(place)
            }
            Expr::Index { base, index } => {
                let mut place = self.lower_place(base, context)?;
                let Ty::Array(element, length) = &place.ty else {
                    self.error(format!(
                        "array index place requires an array value, found `{}`",
                        place.ty
                    ));
                    return None;
                };
                let Some(index) = integer_literal_value(index) else {
                    self.error(
                        "array place index must be a compile-time integer literal; dynamic indexes are read-only for now",
                    );
                    return None;
                };
                let Ok(index) = u64::try_from(index) else {
                    self.error(format!(
                        "array index {index} is out of bounds for length {length}"
                    ));
                    return None;
                };
                if index >= *length {
                    self.error(format!(
                        "array index {index} is out of bounds for length {length}"
                    ));
                    return None;
                }
                let Ok(projection) = usize::try_from(index) else {
                    self.error(format!("array index {index} does not fit this target"));
                    return None;
                };
                place.projections.push(projection);
                place.ty = element.as_ref().clone();
                Some(place)
            }
            _ => {
                self.error("expression is not a local place");
                None
            }
        }
    }

    fn access_place(
        &mut self,
        place: HirPlace,
        requested: AccessKind,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let access = if requested == AccessKind::Auto {
            if self.is_copy_type(&place.ty) {
                AccessKind::Copy
            } else {
                AccessKind::Move
            }
        } else {
            requested
        };
        self.ensure_available(&place, context);
        self.ensure_no_conflicting_loan(&place, access, context);
        match access {
            AccessKind::Copy => {
                if !self.is_copy_type(&place.ty) {
                    let ty = self.diagnostic_type_name(&place.ty);
                    self.error(format!(
                        "type `{ty}` does not implement Copy and cannot be copied"
                    ));
                }
                HirExpr {
                    ty: place.ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Copy,
                    },
                }
            }
            AccessKind::Move => {
                if place.capability != LocalCapability::Owned {
                    self.error("cannot move out of a borrowed value");
                } else if self.projected_place_crosses_custom_drop(&place) {
                    self.error(
                        "moving a field out through a type with custom Drop is not allowed because its destructor requires a complete value",
                    );
                } else if context.guard_move_restricted.contains(&place.local)
                    && !self.is_copy_type(&place.ty)
                {
                    self.error("cannot move a non-Copy pattern binding in a match guard");
                } else {
                    self.mark_moved(&place, context);
                }
                HirExpr {
                    ty: place.ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Move,
                    },
                }
            }
            AccessKind::Auto | AccessKind::SharedBorrow | AccessKind::MutBorrow => {
                unreachable!("borrow accesses do not produce values")
            }
        }
    }

    fn ensure_available(&mut self, place: &HirPlace, context: &LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let leaves = self.place_leaf_keys(place);
        match context.flow.initialization_status(&leaves) {
            InitializationStatus::Initialized => {}
            InitializationStatus::Uninitialized => {
                self.error("use of moved or uninitialized value")
            }
            InitializationStatus::MaybeUninitialized => {
                self.error("use of possibly moved or uninitialized value")
            }
        }
    }

    fn ensure_no_conflicting_loan(
        &mut self,
        place: &HirPlace,
        access: AccessKind,
        context: &LowerCtx,
    ) {
        if !context.flow.reachable {
            return;
        }
        let requested = PlaceKey::from(place);
        let conflict = context.flow.loans.iter().any(|(id, loan)| {
            Some(*id) != place.loan
                && places_overlap(&requested, &loan.place)
                && match access {
                    AccessKind::Copy | AccessKind::SharedBorrow => loan.kind == LoanKind::Mutable,
                    AccessKind::Move | AccessKind::MutBorrow => true,
                    AccessKind::Auto => unreachable!("auto access must be resolved"),
                }
        });
        if !conflict {
            return;
        }
        self.error(match access {
            AccessKind::Copy => "cannot read value while it is mutably borrowed",
            AccessKind::Move => "cannot move value because it is borrowed",
            AccessKind::SharedBorrow => "cannot borrow value while it is mutably borrowed",
            AccessKind::MutBorrow => {
                "cannot create mutable borrow because the value is already borrowed"
            }
            AccessKind::Auto => unreachable!("auto access must be resolved"),
        });
    }

    fn ensure_writable(&mut self, place: &HirPlace) {
        match place.capability {
            LocalCapability::MutParam => {}
            LocalCapability::SharedParam => {
                self.error("cannot assign through a shared borrow");
            }
            LocalCapability::Owned if place.root_mutable => {}
            LocalCapability::Owned => {
                self.error("cannot assign to immutable binding");
            }
        }
    }

    fn mark_moved(&mut self, place: &HirPlace, context: &mut LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let leaves = self.place_leaf_keys(place);
        for alternative in &mut context.flow.uninitialized {
            alternative.extend(leaves.iter().cloned());
        }
        context.flow.normalize_uninitialized();
    }

    fn mark_initialized(&mut self, place: &HirPlace, context: &mut LowerCtx) -> AssignmentKind {
        if !context.flow.reachable {
            return AssignmentKind::Overwrite;
        }
        let leaves = self.place_leaf_keys(place);
        let mut saw_overwrite = false;
        let mut saw_initialize = false;
        let mut saw_partial = false;
        for alternative in &context.flow.uninitialized {
            let unavailable = leaves
                .iter()
                .filter(|leaf| alternative.contains(*leaf))
                .count();
            if unavailable == 0 {
                saw_overwrite = true;
            } else if unavailable == leaves.len() {
                saw_initialize = true;
            } else {
                saw_partial = true;
            }
        }
        let assignment = match (saw_overwrite, saw_initialize, saw_partial) {
            (true, false, false) => AssignmentKind::Overwrite,
            (false, true, false) => AssignmentKind::Initialize,
            _ => AssignmentKind::MaybeOverwrite,
        };
        for alternative in &mut context.flow.uninitialized {
            alternative.retain(|leaf| !leaves.contains(leaf));
        }
        context.flow.normalize_uninitialized();
        assignment
    }

    fn place_leaf_keys(&self, place: &HirPlace) -> Vec<PlaceKey> {
        let mut leaves = Vec::new();
        self.append_leaf_keys(
            PlaceKey::from(place),
            &place.ty,
            &mut HashSet::new(),
            &mut leaves,
        );
        leaves
    }

    fn append_leaf_keys(
        &self,
        key: PlaceKey,
        ty: &Ty,
        visiting: &mut HashSet<String>,
        leaves: &mut Vec<PlaceKey>,
    ) {
        if let Ty::Array(element, length) = ty {
            if *length == 0 {
                leaves.push(key);
                return;
            }
            for index in 0..*length {
                let Ok(index) = usize::try_from(index) else {
                    leaves.push(key);
                    return;
                };
                let mut element_key = key.clone();
                element_key.projections.push(index);
                self.append_leaf_keys(element_key, element, visiting, leaves);
            }
            return;
        }
        let Ty::Struct(name) = ty else {
            leaves.push(key);
            return;
        };
        // Recursive value layouts are diagnosed separately. Keep move-path
        // construction finite while that invalid source continues lowering.
        if !visiting.insert(name.clone()) {
            leaves.push(key);
            return;
        }
        let Some(layout) = self.struct_layouts.get(name) else {
            visiting.remove(name);
            leaves.push(key);
            return;
        };
        if layout.fields.is_empty() {
            visiting.remove(name);
            leaves.push(key);
            return;
        }
        for (index, field) in layout.fields.iter().enumerate() {
            let mut field_key = key.clone();
            field_key.projections.push(index);
            self.append_leaf_keys(field_key, &field.ty, visiting, leaves);
        }
        visiting.remove(name);
    }

    fn acquire_loan(
        &mut self,
        place: &HirPlace,
        kind: LoanKind,
        lexical: bool,
        context: &mut LowerCtx,
    ) -> Option<LoanId> {
        let diagnostics_before = self.diagnostics.len();
        self.ensure_available(place, context);
        let access = match kind {
            LoanKind::Shared => AccessKind::SharedBorrow,
            LoanKind::Mutable => AccessKind::MutBorrow,
        };
        self.ensure_no_conflicting_loan(place, access, context);
        if self.diagnostics.len() != diagnostics_before || !context.flow.reachable {
            return None;
        }
        let id = context.next_loan;
        context.next_loan += 1;
        context.flow.loans.insert(
            id,
            Loan {
                place: PlaceKey::from(place),
                kind,
            },
        );
        if lexical {
            context
                .scopes
                .last_mut()
                .expect("borrow expression has a scope")
                .lexical_loans
                .push(id);
        }
        Some(id)
    }

    fn release_loans(&mut self, loans: &[LoanId], context: &mut LowerCtx) {
        for loan in loans {
            context.flow.loans.remove(loan);
        }
    }

    fn lower_member(
        &mut self,
        base: &Expr,
        member: &str,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Some((name, type_groups)) = self.inferred_generic_enum_type_head(base, context) {
            let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                &name,
                &type_groups,
                member,
                &[],
                InferredEnumHints {
                    payload: None,
                    result: expected,
                },
                context,
            ) else {
                return error_expr();
            };
            return self.lower_nominal_type_member_value(&canonical, NominalKind::Enum, member);
        }
        match self.resolve_nominal_type_head(base, context) {
            Ok(Some((target, kind))) => {
                return self.lower_nominal_type_member_value(&target, kind, member);
            }
            Err(()) => return error_expr(),
            Ok(None) => {}
        }
        if let Expr::Name(target) = base {
            if !context.shadows_top_level_name(target)
                && (self.struct_layouts.contains_key(target)
                    || self.enum_layouts.contains_key(target))
            {
                if let Some(canonical) = self
                    .inherent_members
                    .get(target)
                    .and_then(|members| members.constants.get(member))
                    .cloned()
                {
                    return HirExpr {
                        ty: self.global_type(&canonical),
                        kind: HirExprKind::Global(canonical),
                    };
                }
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
            }
            if !context.shadows_top_level_name(target) {
                if let Some(layout) = self.enum_layouts.get(target).cloned() {
                    if let Some((variant, variant_layout)) = layout
                        .variants
                        .iter()
                        .enumerate()
                        .find(|(_, variant)| variant.name == member)
                    {
                        if !variant_layout.fields.is_empty() {
                            self.error(format!(
                                "variant `{target}.{member}` requires constructor arguments"
                            ));
                            return error_expr();
                        }
                        return HirExpr {
                            ty: Ty::Enum(target.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: target.clone(),
                                variant,
                                fields: Vec::new(),
                            },
                        };
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member or variant `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
                if self.struct_layouts.contains_key(target) {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
            }
        }

        if let Some(place) = self.lower_place_without_diagnostic(base, context) {
            if let Ty::Struct(target) | Ty::Enum(target) = &place.ty {
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.methods.contains_key(member))
                {
                    self.error(format!(
                        "inherent method `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
                let has_field = self
                    .struct_layouts
                    .get(target)
                    .is_some_and(|layout| layout.fields.iter().any(|field| field.name == member));
                if !has_field {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.functions.contains_key(member))
                    {
                        self.error(format!(
                            "associated function `{target}.{member}` must be called on the type"
                        ));
                        return error_expr();
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.constants.contains_key(member))
                    {
                        self.error(format!(
                            "associated constant `{target}.{member}` must be accessed on the type"
                        ));
                        return error_expr();
                    }
                }
            }
            let Ty::Struct(struct_name) = &place.ty else {
                self.error(format!(
                    "member access requires a struct value, found `{}`",
                    place.ty
                ));
                return error_expr();
            };
            let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
                return error_expr();
            };
            let Some((index, field)) = layout
                .fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == member)
            else {
                self.error(format!(
                    "unknown field `{member}` on struct `{struct_name}`"
                ));
                return error_expr();
            };
            if !self.require_field_access(struct_name, field, &context.origin) {
                return error_expr();
            }
            let mut field_place = place;
            field_place.projections.push(index);
            field_place.ty = field.ty.clone();
            return self.access_place(field_place, AccessKind::Auto, context);
        }

        let base = self.lower_expr(base, None, context);
        let Ty::Struct(struct_name) = &base.ty else {
            self.error(format!(
                "member access requires a struct value, found `{}`",
                base.ty
            ));
            return error_expr();
        };
        let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
            return error_expr();
        };
        let Some((index, field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == member)
        else {
            self.error(format!(
                "unknown field `{member}` on struct `{struct_name}`"
            ));
            return error_expr();
        };
        if !self.require_field_access(struct_name, field, &context.origin) {
            return error_expr();
        }
        if self.type_needs_drop(&base.ty) {
            self.error(
                "taking a field from a temporary value that needs drop is not supported until partial-drop LLVM lowering is complete",
            );
        }
        HirExpr {
            ty: field.ty.clone(),
            kind: HirExprKind::Field {
                base: Box::new(base),
                index,
            },
        }
    }

    fn lower_nominal_type_member_value(
        &mut self,
        target: &str,
        kind: NominalKind,
        member: &str,
    ) -> HirExpr {
        if let Some(canonical) = self
            .inherent_members
            .get(target)
            .and_then(|members| members.constants.get(member))
            .cloned()
        {
            return HirExpr {
                ty: self.global_type(&canonical),
                kind: HirExprKind::Global(canonical),
            };
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.functions.contains_key(member))
        {
            self.error(format!(
                "associated function `{target}.{member}` must be called"
            ));
            return error_expr();
        }
        if kind == NominalKind::Enum {
            let Some(layout) = self.enum_layout_or_diagnostic(target) else {
                return error_expr();
            };
            if let Some((variant, variant_layout)) = layout
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name == member)
            {
                if !variant_layout.fields.is_empty() {
                    self.error(format!(
                        "variant `{target}.{member}` requires constructor arguments"
                    ));
                    return error_expr();
                }
                return HirExpr {
                    ty: Ty::Enum(target.to_owned()),
                    kind: HirExprKind::ConstructEnum {
                        name: target.to_owned(),
                        variant,
                        fields: Vec::new(),
                    },
                };
            }
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.methods.contains_key(member))
        {
            self.error(format!(
                "inherent method `{target}.{member}` requires an instance receiver and must be called"
            ));
        } else if kind == NominalKind::Enum {
            self.error(format!(
                "unknown associated member or variant `{member}` on `{target}`"
            ));
        } else {
            self.error(format!(
                "unknown associated member `{member}` on `{target}`"
            ));
        }
        error_expr()
    }

    fn lower_place_without_diagnostic(
        &mut self,
        expression: &Expr,
        context: &mut LowerCtx,
    ) -> Option<HirPlace> {
        match expression {
            Expr::Name(name) if context.lookup(name).is_some() => {
                self.lower_place(expression, context)
            }
            Expr::Member(base, _) => {
                self.lower_place_without_diagnostic(base, context)?;
                self.lower_place(expression, context)
            }
            Expr::Index { base, index } if integer_literal_value(index).is_some() => {
                self.lower_place_without_diagnostic(base, context)?;
                self.lower_place(expression, context)
            }
            _ => None,
        }
    }

    fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let scalar_ty = match self.probe_expr_ty(scrutinee, None, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                if ty == Ty::Bool || ty.is_integer() =>
            {
                Some(ty)
            }
            TypeProbe::Defaultable(ty) if ty.is_integer() => Some(ty),
            _ => None,
        };
        if let Some(scalar_ty) = scalar_ty {
            return self.lower_scalar_match(scrutinee, arms, expected, context, &scalar_ty);
        }
        let inspect_handler_input = matches!(scrutinee, Expr::Name(name) if name.starts_with("$handler$match$inspect$input$"));
        let scrutinee = if inspect_handler_input {
            let Some(place) = self.lower_place(scrutinee, context) else {
                return error_expr();
            };
            self.ensure_available(&place, context);
            self.ensure_no_conflicting_loan(&place, AccessKind::Copy, context);
            HirExpr {
                ty: place.ty.clone(),
                kind: HirExprKind::Read {
                    place,
                    kind: HirReadKind::Inspect,
                },
            }
        } else {
            self.lower_expr(scrutinee, None, context)
        };
        self.lower_match_with_scrutinee(scrutinee, arms, expected, context)
    }

    fn lower_scalar_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
        scalar_ty: &Ty,
    ) -> HirExpr {
        let hidden = format!("$match$scalar${}", context.next_local);
        let mut covers_all = false;
        let mut covers_true = false;
        let mut covers_false = false;
        let mut fallback = Expr::Loop {
            body: Box::new(Expr::Block(Vec::new(), None)),
        };

        if arms.is_empty() {
            self.error("match must contain at least one arm");
        }

        for arm in arms.iter().rev() {
            let (pattern_condition, binding) = match &arm.pattern {
                Pattern::Wildcard => (Expr::Bool(true), None),
                Pattern::Binding(name) => (Expr::Bool(true), Some(name.clone())),
                Pattern::Integer(value) if scalar_ty.is_integer() => (
                    Expr::Binary(
                        Box::new(Expr::Name(hidden.clone())),
                        BinaryOp::Eq,
                        Box::new(Expr::Integer(*value)),
                    ),
                    None,
                ),
                Pattern::Bool(value) if *scalar_ty == Ty::Bool => (
                    Expr::Binary(
                        Box::new(Expr::Name(hidden.clone())),
                        BinaryOp::Eq,
                        Box::new(Expr::Bool(*value)),
                    ),
                    None,
                ),
                Pattern::Constructor { path, .. } => {
                    self.error(format!(
                        "constructor pattern `{}` cannot match scalar type `{scalar_ty}`",
                        path.join(".")
                    ));
                    (Expr::Bool(true), None)
                }
                Pattern::Integer(_) | Pattern::Bool(_) => {
                    self.error(format!(
                        "pattern type mismatch: literal pattern cannot match `{scalar_ty}`"
                    ));
                    (Expr::Bool(true), None)
                }
            };

            if arm.guard.is_none() {
                match &arm.pattern {
                    Pattern::Wildcard | Pattern::Binding(_) => covers_all = true,
                    Pattern::Bool(true) if *scalar_ty == Ty::Bool => covers_true = true,
                    Pattern::Bool(false) if *scalar_ty == Ty::Bool => covers_false = true,
                    _ => {}
                }
            }

            let scoped = |value: Expr, binding: Option<&String>| {
                let statements = binding.map_or_else(Vec::new, |name| {
                    vec![Stmt::Let(Binding {
                        mutable: false,
                        name: name.clone(),
                        annotation: None,
                        value: Expr::Name(hidden.clone()),
                    })]
                });
                Expr::Block(statements, Some(Box::new(value)))
            };
            let condition = if let Some(guard) = &arm.guard {
                Expr::Binary(
                    Box::new(pattern_condition),
                    BinaryOp::And,
                    Box::new(scoped(guard.clone(), binding.as_ref())),
                )
            } else {
                pattern_condition
            };
            let body = scoped(arm.body.clone(), binding.as_ref());
            fallback = Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(body),
                else_branch: Some(Box::new(fallback)),
            };
        }

        let exhaustive = covers_all || (*scalar_ty == Ty::Bool && covers_true && covers_false);
        if !exhaustive {
            self.error(format!(
                "match on `{scalar_ty}` is not exhaustive; add a wildcard or binding arm"
            ));
        }

        let lowered = Expr::Block(
            vec![Stmt::Let(Binding {
                mutable: false,
                name: hidden,
                annotation: self.source_type_for_ty(scalar_ty),
                value: scrutinee.clone(),
            })],
            Some(Box::new(fallback)),
        );
        self.lower_expr(&lowered, expected, context)
    }
    fn lower_match_with_scrutinee(
        &mut self,
        scrutinee: HirExpr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Ty::Enum(enum_name) = &scrutinee.ty else {
            self.error(format!(
                "match currently requires an enum value, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
        let mut lowered_arms = Vec::new();
        let mut covered = vec![false; layout.variants.len()];
        let mut result_ty: Option<Ty> = None;
        let entry_flow = context.flow.clone();
        let mut fallthrough_flows = vec![Some(entry_flow.clone()); layout.variants.len()];
        let mut exit_flows = Vec::new();
        let inspected_place = match &scrutinee.kind {
            HirExprKind::Read {
                place,
                kind: HirReadKind::Inspect,
            } => Some(place.clone()),
            _ => None,
        };

        for arm in arms {
            context.push_scope();
            let (matcher, mut bindings, literal_conditions) =
                self.lower_enum_pattern(&arm.pattern, &layout, context);
            if let Some(root) = &inspected_place {
                for binding in bindings.iter().filter(|binding| binding.moves) {
                    let mut alias = root.clone();
                    alias.projections.extend(binding.path.iter().copied());
                    alias.ty = binding.ty.clone();
                    alias.capability = LocalCapability::SharedParam;
                    let local = context
                        .scopes
                        .last_mut()
                        .expect("match arm scope")
                        .names
                        .get_mut(&binding.name)
                        .expect("pattern binding local exists");
                    local.capability = LocalCapability::SharedParam;
                    local.alias = Some(alias);
                    context.inspection_bindings.insert(
                        binding.id,
                        InspectionBinding {
                            root: root.local,
                            path: binding.path.clone(),
                            ty: binding.ty.clone(),
                        },
                    );
                }
                bindings.retain(|binding| !binding.moves);
            }
            if self.type_has_custom_drop(&scrutinee.ty)
                && bindings
                    .iter()
                    .any(|binding| binding.moves && !binding.path.is_empty())
            {
                self.error(format!(
                    "cannot move a pattern binding out of `{}` because it implements `Drop`",
                    scrutinee.ty
                ));
            }
            let matched_variants: Vec<_> = match matcher {
                HirMatcher::Variant(index) => vec![index],
                HirMatcher::All => (0..layout.variants.len()).collect(),
            };
            let incoming_flows: Vec<_> = matched_variants
                .iter()
                .filter_map(|index| fallthrough_flows[*index].clone())
                .collect();
            context.flow = FlowState::join(&incoming_flows);
            let previous_guard_restrictions = context.guard_move_restricted.clone();
            context.guard_move_restricted.extend(
                bindings
                    .iter()
                    .filter(|binding| !self.is_copy_type(&binding.ty))
                    .map(|binding| binding.id),
            );
            let pattern_guard = literal_conditions
                .into_iter()
                .reduce(|left, right| Expr::Binary(Box::new(left), BinaryOp::And, Box::new(right)));
            let combined_guard = match (pattern_guard, arm.guard.as_ref()) {
                (Some(pattern), Some(guard)) => Some(Expr::Binary(
                    Box::new(pattern),
                    BinaryOp::And,
                    Box::new(guard.clone()),
                )),
                (Some(pattern), None) => Some(pattern),
                (None, Some(guard)) => Some(guard.clone()),
                (None, None) => None,
            };
            let guard = combined_guard
                .as_ref()
                .map(|guard| self.lower_expr(guard, Some(&Ty::Bool), context));
            context.guard_move_restricted = previous_guard_restrictions;
            let guard_flow = guard.as_ref().map(|_| context.flow.clone());
            let branch_expected = expected.or_else(|| {
                result_ty
                    .as_ref()
                    .filter(|ty| **ty != Ty::Error && !self.is_uninhabited_type(ty))
            });
            let body = self.lower_expr(&arm.body, branch_expected, context);
            let guard_fallthrough = guard_flow.map(|flow| context.flow_without_current_scope(flow));
            context.pop_scope();
            exit_flows.push(context.flow.clone());

            for variant in &matched_variants {
                if fallthrough_flows[*variant].is_some() {
                    fallthrough_flows[*variant] = guard_fallthrough.clone();
                }
            }

            result_ty = Some(match result_ty {
                Some(current) => self.unify_types(&current, &body.ty, "match arm results"),
                None => body.ty.clone(),
            });
            if guard.is_none() {
                match matcher {
                    HirMatcher::Variant(index) => covered[index] = true,
                    HirMatcher::All => covered.fill(true),
                }
            }
            lowered_arms.push(HirMatchArm {
                matcher,
                bindings,
                guard,
                body,
            });
        }

        exit_flows.extend(fallthrough_flows.into_iter().flatten());
        context.flow = FlowState::join(&exit_flows);

        let missing: Vec<_> = layout
            .variants
            .iter()
            .zip(covered)
            .filter_map(|(variant, covered)| (!covered).then_some(variant.name.as_str()))
            .collect();
        if !missing.is_empty() {
            self.error(format!(
                "match on `{enum_name}` is not exhaustive; missing {}",
                missing.join(", ")
            ));
        }
        if arms.is_empty() && !layout.variants.is_empty() {
            self.error("match must contain at least one arm");
        }
        HirExpr {
            ty: result_ty.unwrap_or({
                if layout.variants.is_empty() {
                    Ty::Never
                } else {
                    Ty::Error
                }
            }),
            kind: HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: lowered_arms,
            },
        }
    }

    fn lower_enum_pattern(
        &mut self,
        pattern: &Pattern,
        layout: &EnumLayout,
        context: &mut LowerCtx,
    ) -> (HirMatcher, Vec<HirPatternBinding>, Vec<Expr>) {
        let mut bindings = Vec::new();
        let mut literal_conditions = Vec::new();
        match pattern {
            Pattern::Wildcard => (HirMatcher::All, bindings, literal_conditions),
            Pattern::Binding(name) => {
                self.bind_pattern(
                    name,
                    Ty::Enum(layout.name.clone()),
                    Vec::new(),
                    context,
                    &mut bindings,
                );
                (HirMatcher::All, bindings, literal_conditions)
            }
            Pattern::Constructor { path, fields } => {
                let Some(variant_name) = path.last() else {
                    self.error("empty constructor path in pattern");
                    return (HirMatcher::All, bindings, literal_conditions);
                };
                let source_name = self
                    .nominal_instances
                    .get(&layout.name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(&layout.name);
                if path.len() > 2
                    || (path.len() == 2 && path[0] != layout.name && path[0] != source_name)
                {
                    self.error(format!(
                        "pattern constructor `{}` does not belong to enum `{}`",
                        path.join("."),
                        layout.name
                    ));
                    return (HirMatcher::All, bindings, literal_conditions);
                }
                let Some(variant_index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == *variant_name)
                else {
                    self.error(format!(
                        "unknown pattern variant `{variant_name}` for enum `{}`",
                        layout.name
                    ));
                    return (HirMatcher::All, bindings, literal_conditions);
                };
                let variant = &layout.variants[variant_index];
                let patterns = self.normalize_pattern_fields(
                    fields,
                    &variant.fields,
                    variant.named,
                    &format!("pattern `{}.{}`", layout.name, variant.name),
                    &format!("{}.{}", layout.name, variant.name),
                    &context.origin,
                );
                for (field_index, field_pattern) in patterns {
                    let field = &variant.fields[field_index];
                    self.lower_irrefutable_pattern(
                        &field_pattern,
                        &field.ty,
                        vec![1 + variant.payload_offset + field_index],
                        context,
                        &mut bindings,
                        &mut literal_conditions,
                    );
                }
                (
                    HirMatcher::Variant(variant_index),
                    bindings,
                    literal_conditions,
                )
            }
            Pattern::Integer(_) | Pattern::Bool(_) => {
                self.error(format!(
                    "pattern type mismatch: enum `{}` cannot be matched by a scalar literal",
                    layout.name
                ));
                (HirMatcher::All, bindings, literal_conditions)
            }
        }
    }

    fn normalize_pattern_fields(
        &mut self,
        patterns: &PatternFields,
        fields: &[FieldLayout],
        named: bool,
        description: &str,
        owner: &str,
        origin: &ItemOrigin,
    ) -> Vec<(usize, Pattern)> {
        for field in fields {
            self.require_field_access(owner, field, origin);
        }
        match patterns {
            PatternFields::Unit => {
                if !fields.is_empty() {
                    self.error(format!(
                        "{description} requires {} field patterns",
                        fields.len()
                    ));
                }
                Vec::new()
            }
            PatternFields::Positional(patterns) => {
                if patterns.len() != fields.len() {
                    self.error(format!(
                        "field count mismatch in {description}: expected {}, found {}",
                        fields.len(),
                        patterns.len()
                    ));
                }
                patterns.iter().cloned().enumerate().collect()
            }
            PatternFields::Named(patterns) => {
                if !named {
                    self.error(format!("{description} has positional fields"));
                    return Vec::new();
                }
                let mut seen = HashSet::new();
                let mut result = Vec::new();
                for pattern in patterns {
                    let Some(index) = fields.iter().position(|field| field.name == pattern.name)
                    else {
                        self.error(format!("unknown field `{}` in {description}", pattern.name));
                        continue;
                    };
                    if !seen.insert(index) {
                        self.error(format!(
                            "duplicate field `{}` in {description}",
                            pattern.name
                        ));
                        continue;
                    }
                    result.push((index, pattern.pattern.clone()));
                }
                for (index, field) in fields.iter().enumerate() {
                    if !seen.contains(&index) {
                        self.error(format!("missing field `{}` in {description}", field.name));
                    }
                }
                result
            }
        }
    }

    fn lower_irrefutable_pattern(
        &mut self,
        pattern: &Pattern,
        ty: &Ty,
        path: Vec<usize>,
        context: &mut LowerCtx,
        bindings: &mut Vec<HirPatternBinding>,
        literal_conditions: &mut Vec<Expr>,
    ) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind_pattern(name, ty.clone(), path, context, bindings),
            Pattern::Integer(value) if ty.is_integer() => {
                let name = format!("$match$literal${}", bindings.len());
                self.bind_pattern(&name, ty.clone(), path, context, bindings);
                literal_conditions.push(Expr::Binary(
                    Box::new(Expr::Name(name)),
                    BinaryOp::Eq,
                    Box::new(Expr::Integer(*value)),
                ));
            }
            Pattern::Bool(value) if *ty == Ty::Bool => {
                let name = format!("$match$literal${}", bindings.len());
                self.bind_pattern(&name, ty.clone(), path, context, bindings);
                literal_conditions.push(Expr::Binary(
                    Box::new(Expr::Name(name)),
                    BinaryOp::Eq,
                    Box::new(Expr::Bool(*value)),
                ));
            }
            Pattern::Constructor {
                path: constructor,
                fields,
            } => {
                let Ty::Struct(struct_name) = ty else {
                    self.error(format!(
                        "pattern type mismatch: constructor `{}` cannot match `{ty}`",
                        constructor.join(".")
                    ));
                    return;
                };
                let source_name = self
                    .nominal_instances
                    .get(struct_name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(struct_name);
                if constructor
                    .last()
                    .is_none_or(|name| name != struct_name && name != source_name)
                {
                    self.error(format!(
                        "pattern type mismatch: expected struct `{struct_name}`, found `{}`",
                        constructor.join(".")
                    ));
                    return;
                }
                let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
                    return;
                };
                let nested = self.normalize_pattern_fields(
                    fields,
                    &layout.fields,
                    true,
                    &format!("pattern `{struct_name}`"),
                    struct_name,
                    &context.origin,
                );
                let binding_start = bindings.len();
                for (index, pattern) in nested {
                    let mut nested_path = path.clone();
                    nested_path.push(index);
                    self.lower_irrefutable_pattern(
                        &pattern,
                        &layout.fields[index].ty,
                        nested_path,
                        context,
                        bindings,
                        literal_conditions,
                    );
                }
                if self.type_has_custom_drop(ty)
                    && bindings[binding_start..]
                        .iter()
                        .any(|binding| binding.moves)
                {
                    self.error(format!(
                        "cannot move a nested pattern binding through `{ty}` because it implements `Drop`"
                    ));
                }
            }
            Pattern::Integer(_) | Pattern::Bool(_) => self.error(format!(
                "pattern type mismatch: literal pattern cannot match `{ty}`"
            )),
        }
    }

    fn bind_pattern(
        &mut self,
        name: &str,
        ty: Ty,
        path: Vec<usize>,
        context: &mut LowerCtx,
        bindings: &mut Vec<HirPatternBinding>,
    ) {
        if context
            .scopes
            .last()
            .expect("match arm scope")
            .names
            .contains_key(name)
        {
            self.error(format!("duplicate pattern binding `{name}`"));
            return;
        }
        let id = context.fresh_local();
        context.insert_local(
            name.to_owned(),
            LocalInfo {
                id,
                ty: ty.clone(),
                mutable: false,
                capability: LocalCapability::Owned,
                alias: None,
                partial: None,
                closure: None,
            },
        );
        bindings.push(HirPatternBinding {
            id,
            name: name.to_owned(),
            moves: !self.is_copy_type(&ty),
            ty,
            path,
        });
    }

    fn lower_call(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::Name(name) = root {
            if name == "$handler$erase$continuation" {
                if groups.len() != 1 || groups[0].len() != 1 {
                    self.error("internal continuation erasure expects one callable argument");
                    return error_expr();
                }
                let Expr::Name(local_name) = &groups[0][0].value else {
                    self.error("internal continuation erasure requires a local closure");
                    return error_expr();
                };
                let Some(local) = context.lookup(local_name).cloned() else {
                    self.error("internal continuation erasure refers to an unknown closure");
                    return error_expr();
                };
                let Some(closure) = local.closure.clone() else {
                    self.error("internal continuation erasure requires closure metadata");
                    return error_expr();
                };
                let [group] = closure.groups.as_slice() else {
                    self.error("an erased continuation must have one parameter group");
                    return error_expr();
                };
                let [parameter] = group.as_slice() else {
                    self.error("an erased continuation must accept exactly one value");
                    return error_expr();
                };
                let callable = HirPlace {
                    local: local.id,
                    root_ty: local.ty.clone(),
                    projections: Vec::new(),
                    ty: local.ty.clone(),
                    capability: LocalCapability::Owned,
                    root_mutable: local.mutable,
                    loan: None,
                    indirect: false,
                };
                self.ensure_available(&callable, context);
                self.mark_moved(&callable, context);
                let continuation_ty = Ty::Continuation {
                    input: Box::new(parameter.ty.clone()),
                    output: Box::new(closure.result.clone()),
                };
                let adapter = format!("$continuation$adapter${}", closure.function);
                if !self
                    .continuation_adapters
                    .iter()
                    .any(|existing| existing.name == adapter)
                {
                    let Ty::Callable(callable_ty) = &local.ty else {
                        self.error("internal continuation closure has no callable type");
                        return error_expr();
                    };
                    self.continuation_adapters.push(ContinuationAdapter {
                        name: adapter.clone(),
                        callable_ty: local.ty.clone(),
                        function: closure.function,
                        captures: callable_ty.captures.clone(),
                        input: parameter.ty.clone(),
                        output: closure.result,
                    });
                }
                return HirExpr {
                    ty: continuation_ty,
                    kind: HirExprKind::EraseContinuation {
                        binding: local.id,
                        callable_ty: local.ty,
                        adapter,
                    },
                };
            }
            if name == "$handler$invoke$continuation" {
                if groups.len() != 1 || groups[0].len() != 2 {
                    self.error("internal continuation invocation expects continuation and value");
                    return error_expr();
                }
                let continuation = self.lower_expr(&groups[0][0].value, None, context);
                let Ty::Continuation { input, output } = continuation.ty.clone() else {
                    self.error("internal continuation invocation requires an erased continuation");
                    return error_expr();
                };
                let argument = self.lower_expr(&groups[0][1].value, Some(&input), context);
                self.require_same_type(&argument.ty, &input, "continuation input");
                return HirExpr {
                    ty: (*output).clone(),
                    kind: HirExprKind::InvokeContinuation {
                        continuation: Box::new(continuation),
                        argument: Box::new(argument),
                    },
                };
            }
            if name == "$handler$erase$effect$callable" {
                if groups.len() != 1 || groups[0].len() != 1 {
                    self.error("internal effect-callable erasure expects one callable argument");
                    return error_expr();
                }
                let Expr::Name(local_name) = &groups[0][0].value else {
                    self.error("internal effect-callable erasure requires a local closure");
                    return error_expr();
                };
                let Some(local) = context.lookup(local_name).cloned() else {
                    self.error("internal effect-callable erasure refers to an unknown closure");
                    return error_expr();
                };
                let Some(closure) = local.closure.clone() else {
                    self.error("internal effect-callable erasure requires closure metadata");
                    return error_expr();
                };
                let [group] = closure.groups.as_slice() else {
                    self.error("an erased effect callable must have one parameter group");
                    return error_expr();
                };
                let [input_parameter, continuation_parameter] = group.as_slice() else {
                    self.error("an erased effect callable must accept an input and a continuation");
                    return error_expr();
                };
                let Ty::Continuation {
                    input: output,
                    output: answer,
                } = &continuation_parameter.ty
                else {
                    self.error(
                        "an erased effect callable's second parameter must be a continuation",
                    );
                    return error_expr();
                };
                self.require_same_type(&closure.result, answer, "erased effect-callable answer");
                let callable = HirPlace {
                    local: local.id,
                    root_ty: local.ty.clone(),
                    projections: Vec::new(),
                    ty: local.ty.clone(),
                    capability: LocalCapability::Owned,
                    root_mutable: local.mutable,
                    loan: None,
                    indirect: false,
                };
                self.ensure_available(&callable, context);
                self.mark_moved(&callable, context);
                let action_ty = Ty::EffectCallable {
                    input: Box::new(input_parameter.ty.clone()),
                    output: output.clone(),
                    answer: answer.clone(),
                };
                let adapter = format!("$effect$callable$adapter${}", closure.function);
                if !self
                    .effect_callable_adapters
                    .iter()
                    .any(|existing| existing.name == adapter)
                {
                    let Ty::Callable(callable_ty) = &local.ty else {
                        self.error("internal effect closure has no callable type");
                        return error_expr();
                    };
                    self.effect_callable_adapters.push(EffectCallableAdapter {
                        name: adapter.clone(),
                        callable_ty: local.ty.clone(),
                        function: closure.function,
                        captures: callable_ty.captures.clone(),
                        input: input_parameter.ty.clone(),
                        output: (**output).clone(),
                        answer: (**answer).clone(),
                    });
                }
                return HirExpr {
                    ty: action_ty,
                    kind: HirExprKind::EraseEffectCallable {
                        binding: local.id,
                        callable_ty: local.ty,
                        adapter,
                    },
                };
            }
            if name == "$handler$invoke$effect$callable" {
                if groups.len() != 1 || groups[0].len() != 3 {
                    self.error(
                        "internal effect-callable invocation expects action, input, and continuation",
                    );
                    return error_expr();
                }
                let action = self.lower_expr(&groups[0][0].value, None, context);
                let Ty::EffectCallable {
                    input,
                    output,
                    answer,
                } = action.ty.clone()
                else {
                    self.error("internal effect-callable invocation requires an erased action");
                    return error_expr();
                };
                let input_value = self.lower_expr(&groups[0][1].value, Some(&input), context);
                self.require_same_type(&input_value.ty, &input, "effect-callable input");
                let expected_continuation = Ty::Continuation {
                    input: output,
                    output: answer.clone(),
                };
                let continuation =
                    self.lower_expr(&groups[0][2].value, Some(&expected_continuation), context);
                self.require_same_type(
                    &continuation.ty,
                    &expected_continuation,
                    "effect-callable continuation",
                );
                return HirExpr {
                    ty: (*answer).clone(),
                    kind: HirExprKind::InvokeEffectCallable {
                        action: Box::new(action),
                        input: Box::new(input_value),
                        continuation: Box::new(continuation),
                    },
                };
            }
            if name.starts_with("$handler$tail$") {
                if groups.len() != 1 || groups[0].len() != 1 {
                    self.error("internal handler tail continuation has invalid arguments");
                    return error_expr();
                }
                if let Some(boundary) = context.return_boundary.clone() {
                    let call =
                        self.lower_expr(&groups[0][0].value, Some(&boundary.success), context);
                    let result = self.finish_return_value(call, &boundary);
                    context.returned_types.push(result.ty.clone());
                    context.flow.reachable = false;
                    return HirExpr {
                        ty: Ty::Never,
                        kind: HirExprKind::Return(Some(Box::new(result))),
                    };
                }
                let declared_result = context.declared_result.clone();
                let call = self.lower_expr(&groups[0][0].value, declared_result.as_ref(), context);
                let result = call.ty.clone();
                if let Some(expected) = context.declared_result.clone() {
                    self.require_same_type(&result, &expected, "handler tail continuation result");
                }
                context.returned_types.push(result.clone());
                context.flow.reachable = false;
                return match call.kind {
                    HirExprKind::Call {
                        function,
                        arguments,
                        consumed_callable,
                        ..
                    } => HirExpr {
                        ty: Ty::Never,
                        kind: HirExprKind::TailCall {
                            function,
                            arguments,
                            consumed_callable,
                            result,
                        },
                    },
                    HirExprKind::InvokeContinuation {
                        continuation,
                        argument,
                    } => HirExpr {
                        ty: Ty::Never,
                        kind: HirExprKind::TailInvokeContinuation {
                            continuation,
                            argument,
                            result,
                        },
                    },
                    _ => {
                        self.error(
                            "handler tail continuation must resolve to a direct or erased continuation call",
                        );
                        error_expr()
                    }
                };
            }
            if let Some(frame) = context.recursive_frame_calls.get(name).cloned() {
                return self.lower_recursive_frame_call(&frame, &groups, context);
            }
        }
        if let Expr::ChainMember(base, member) = root {
            return self.lower_chain(base, member, Some(&groups), expected, context);
        }
        if let Expr::Name(name) = root {
            if matches!(name.as_str(), "size_of" | "align_of") {
                return self.lower_layout_query(name, &groups, context);
            }
            if name == "raw_alloc" {
                return self.lower_raw_alloc(&groups, expected, context);
            }
            if name == "raw_dealloc" {
                return self.lower_raw_dealloc(&groups, context);
            }
            if name == "raw_init" {
                return self.lower_raw_init(&groups, context);
            }
            if name == "raw_take" {
                return self.lower_raw_take(&groups, context);
            }
            if name == "raw_offset" {
                return self.lower_raw_offset(&groups, context);
            }
            if name == "raw_borrow" {
                return self.lower_raw_borrow(name, &groups, expected, context);
            }
            if name == "raw_trap" {
                return self.lower_raw_trap(&groups, context);
            }
            if name == "forget" {
                return self.lower_forget(&groups, context);
            }
            if matches!(name.as_str(), "Ptr" | "MutPtr") {
                return self.lower_raw_pointer_constructor(name, &groups, context);
            }
            if let Some(local) = context.lookup(name).cloned() {
                if local.closure.is_some() {
                    return self.lower_local_closure_call(name, &local, &groups, expected, context);
                }
                if local.partial.is_some() {
                    return self.lower_local_partial_call(name, &local, &groups, expected, context);
                }
                if matches!(local.ty, Ty::Function(_)) {
                    return self
                        .lower_indirect_function_call(name, &local, &groups, expected, context);
                }
                self.error(format!("local value `{name}` is not callable"));
                return error_expr();
            }
            if context.has_type_parameter(name) {
                self.error(format!("type parameter `{name}` is not callable"));
                return error_expr();
            }
            if name == "Self" {
                self.error("expression `Self` is only available inside an extend member");
                return error_expr();
            }
            if self.function_overloads.contains_key(name) {
                let Some(selected) = self.resolve_function_overload(name, &groups) else {
                    return error_expr();
                };
                if self.function_templates.contains_key(&selected) {
                    return self.lower_generic_function_call(&selected, &groups, expected, context);
                }
                return self.lower_named_function_call(&selected, &groups, expected, context);
            }
            if self.function_templates.contains_key(name) {
                return self.lower_generic_function_call(name, &groups, expected, context);
            }
            if self.functions.contains_key(name) {
                return self.lower_named_function_call(name, &groups, expected, context);
            }
            if self.struct_layouts.contains_key(name) {
                self.error(format!(
                    "struct `{name}` is not callable; construct it with `{name} {{ ... }}`"
                ));
                return error_expr();
            }
            if self.struct_templates.contains_key(name) {
                self.error(format!(
                    "generic struct `{name}` is not callable; construct it with `{name}(...) {{ ... }}` or `{name} {{ ... }}`"
                ));
                return error_expr();
            }
            if self.enum_templates.contains_key(name) {
                self.error(format!(
                    "generic enum type `{name}` is not directly callable; select a variant"
                ));
                return error_expr();
            }
            if let Some((enum_name, variant)) =
                self.resolve_short_variant(name, expected, &context.origin)
            {
                return self.lower_enum_constructor(&enum_name, variant, &groups, context);
            }
            self.error(format!("`{name}` is not a function or constructor"));
            return error_expr();
        }
        if let Expr::Member(base, variant_name) = root {
            match self.resolve_effect_application(base, context) {
                Ok(Some((definition, instance))) => {
                    if variant_name == "handle" {
                        return self.lower_effect_handler(
                            &definition,
                            &instance,
                            &groups,
                            expected,
                            context,
                        );
                    }
                    return self.lower_effect_operation_call(
                        &definition,
                        &instance,
                        variant_name,
                        &groups,
                        expected,
                        context,
                    );
                }
                Err(()) => return error_expr(),
                Ok(None) => {}
            }
            if let Some((member, lang_item)) = match variant_name.as_str() {
                "$lang$into_iter" => Some(("into_iter", LangItemKind::IntoIterator)),
                "$lang$next" => Some(("next", LangItemKind::Iterator)),
                "$lang$chain" => Some(("chain", LangItemKind::Chain)),
                "$lang$coalesce" => Some(("coalesce", LangItemKind::Coalesce)),
                _ => None,
            } {
                return self.lower_bound_method_call(
                    base,
                    member,
                    &groups,
                    BoundMethodConstraint::LangItem(lang_item),
                    expected,
                    context,
                );
            }
            if let Some((name, type_groups)) = self.inferred_generic_enum_type_head(base, context) {
                let is_variant = self.enum_templates[&name]
                    .variants
                    .iter()
                    .any(|variant| variant.name == *variant_name);
                if is_variant {
                    let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                        &name,
                        &type_groups,
                        variant_name,
                        &groups,
                        InferredEnumHints {
                            payload: None,
                            result: expected,
                        },
                        context,
                    ) else {
                        return error_expr();
                    };
                    return self.lower_nominal_type_member_call(
                        &canonical,
                        NominalKind::Enum,
                        variant_name,
                        &groups,
                        expected,
                        context,
                    );
                }
            }
            if let Expr::Name(target_template) = base.as_ref() {
                if !context.shadows_top_level_name(target_template) {
                    let explicit_method = groups.first().is_some_and(|group| {
                        matches!(*group, [CallArg { label: Some(label), .. }] if label == "self")
                    });
                    if !explicit_method {
                        let overload_key = (target_template.clone(), variant_name.clone(), false);
                        if (self.struct_templates.contains_key(target_template)
                            || self.enum_templates.contains_key(target_template))
                            && self.inherent_overloads.contains_key(&overload_key)
                        {
                            let Some(canonical) = self.resolve_inherent_overload(
                                target_template,
                                variant_name,
                                false,
                                &groups,
                            ) else {
                                return error_expr();
                            };
                            return self.lower_generic_function_call(
                                &canonical, &groups, expected, context,
                            );
                        }
                        if let Some(canonical) = self
                            .generic_inherent_functions
                            .get(&(target_template.clone(), variant_name.clone()))
                            .cloned()
                        {
                            return self.lower_generic_function_call(
                                &canonical, &groups, expected, context,
                            );
                        }
                        if self.struct_templates.contains_key(target_template)
                            || self.enum_templates.contains_key(target_template)
                        {
                            if let Some(result) = self
                                .lower_constructor_trait_associated_function_call(
                                    target_template,
                                    variant_name,
                                    &groups,
                                    expected,
                                    context,
                                )
                            {
                                return result;
                            }
                        }
                    }
                    if self.struct_templates.contains_key(target_template)
                        || self.enum_templates.contains_key(target_template)
                    {
                        if let Some([receiver]) = groups.first().copied() {
                            let receiver_ty =
                                match self.probe_expr_ty(&receiver.value, None, context) {
                                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => {
                                        Some(ty)
                                    }
                                    TypeProbe::Defaultable(_) | TypeProbe::Unsupported => None,
                                };
                            if let Some((canonical, kind)) = receiver_ty.and_then(|ty| match ty {
                                Ty::Struct(name) => Some((name, NominalKind::Struct)),
                                Ty::Enum(name) => Some((name, NominalKind::Enum)),
                                _ => None,
                            }) {
                                let belongs_to_template = self
                                    .nominal_instances
                                    .get(&canonical)
                                    .is_some_and(|instance| {
                                        instance.key.template == *target_template
                                            && instance.key.kind == kind
                                    });
                                let self_ty = match kind {
                                    NominalKind::Struct => Ty::Struct(canonical.clone()),
                                    NominalKind::Enum => Ty::Enum(canonical.clone()),
                                };
                                let has_method =
                                    self.inherent_members
                                        .get(&canonical)
                                        .is_some_and(|members| {
                                            members.methods.contains_key(variant_name)
                                        })
                                        || !self
                                            .trait_method_candidates(
                                                &self_ty,
                                                variant_name,
                                                &context.origin,
                                            )
                                            .is_empty();
                                if belongs_to_template && has_method {
                                    return self.lower_nominal_type_member_call(
                                        &canonical,
                                        kind,
                                        variant_name,
                                        &groups,
                                        expected,
                                        context,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            match self.resolve_nominal_type_head(base, context) {
                Ok(Some((target, kind))) => {
                    return self.lower_nominal_type_member_call(
                        &target,
                        kind,
                        variant_name,
                        &groups,
                        expected,
                        context,
                    );
                }
                Err(()) => return error_expr(),
                Ok(None) => {}
            }
            if let Expr::Name(enum_name) = base.as_ref() {
                if !context.shadows_top_level_name(enum_name)
                    && (self.struct_layouts.contains_key(enum_name)
                        || self.enum_layouts.contains_key(enum_name))
                {
                    if let Some(canonical) = self
                        .inherent_members
                        .get(enum_name)
                        .and_then(|members| members.functions.get(variant_name))
                        .cloned()
                    {
                        return self
                            .lower_named_function_call(&canonical, &groups, expected, context);
                    }
                    if self
                        .inherent_members
                        .get(enum_name)
                        .is_some_and(|members| members.constants.contains_key(variant_name))
                    {
                        self.error(format!(
                            "associated constant `{enum_name}.{variant_name}` is not callable"
                        ));
                        return error_expr();
                    }
                }
                if !context.shadows_top_level_name(enum_name) {
                    if let Some(layout) = self.enum_layouts.get(enum_name) {
                        if let Some(variant) = layout
                            .variants
                            .iter()
                            .position(|variant| variant.name == *variant_name)
                        {
                            return self
                                .lower_enum_constructor(enum_name, variant, &groups, context);
                        }
                        if self
                            .inherent_members
                            .get(enum_name)
                            .is_some_and(|members| members.methods.contains_key(variant_name))
                        {
                            self.error(format!(
                                "inherent method `{enum_name}.{variant_name}` requires an instance receiver"
                            ));
                            return error_expr();
                        }
                        self.error(format!(
                            "unknown associated member or variant `{variant_name}` on `{enum_name}`"
                        ));
                        return error_expr();
                    }
                    if self.struct_layouts.contains_key(enum_name) {
                        if self
                            .inherent_members
                            .get(enum_name)
                            .is_some_and(|members| members.methods.contains_key(variant_name))
                        {
                            self.error(format!(
                                "inherent method `{enum_name}.{variant_name}` requires an instance receiver"
                            ));
                            return error_expr();
                        }
                        self.error(format!(
                            "unknown associated member `{variant_name}` on `{enum_name}`"
                        ));
                        return error_expr();
                    }
                }
            }
            return self.lower_bound_method_call(
                base,
                variant_name,
                &groups,
                BoundMethodConstraint::None,
                expected,
                context,
            );
        }
        self.error("calls require a named function, constructor, associated function, or method");
        error_expr()
    }

    fn lower_recursive_frame_call(
        &mut self,
        frame: &RecursiveFrameCall,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 || groups[0].len() != frame.parameters.len() {
            self.error(format!(
                "recursive continuation frame expects {} argument(s), found {}",
                frame.parameters.len(),
                groups.first().map_or(0, |group| group.len())
            ));
            return error_expr();
        }
        let mut lowered = Vec::new();
        let mut loans = Vec::new();
        let mut temporaries = Vec::new();
        for capture in &frame.captures {
            lowered.push(self.lower_call_argument(
                &Expr::Name(capture.name.clone()),
                capture,
                context,
                &mut loans,
                &mut temporaries,
            ));
        }
        for (argument, parameter) in groups[0].iter().zip(&frame.parameters) {
            lowered.push(self.lower_call_argument(
                &argument.value,
                parameter,
                context,
                &mut loans,
                &mut temporaries,
            ));
        }
        self.release_loans(&loans, context);
        let call = HirExpr {
            ty: frame.result.clone(),
            kind: HirExprKind::Call {
                function: frame.function.clone(),
                arguments: lowered.clone(),
                consumed_callable: None,
                diverges: self.is_uninhabited_type(&frame.result),
            },
        };
        self.wrap_call_argument_temporaries(call, &mut lowered, temporaries, context)
    }

    fn resolve_effect_application(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Result<Option<(EffectDef, Type)>, ()> {
        let (name, arguments) = match expression {
            Expr::Name(name)
                if !context.shadows_top_level_name(name) && self.effect_defs.contains_key(name) =>
            {
                (name.clone(), Vec::new())
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    return Ok(None);
                };
                if context.shadows_top_level_name(name) || !self.effect_defs.contains_key(name) {
                    return Ok(None);
                }
                if arguments.iter().any(|argument| argument.label.is_some()) {
                    self.error(format!(
                        "effect application `{name}` currently requires positional type arguments"
                    ));
                    return Err(());
                }
                let mut sources = Vec::new();
                for argument in arguments {
                    let Some(source) =
                        self.type_argument_from_expr(&argument.value, &context.type_substitutions)
                    else {
                        return Err(());
                    };
                    sources.push(source);
                }
                (name.clone(), sources)
            }
            _ => return Ok(None),
        };
        let definition = self.effect_defs[&name].clone();
        let parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .collect::<Vec<_>>();
        if arguments.len() != parameters.len() {
            self.error(format!(
                "effect argument count mismatch for `{name}`: expected {}, found {}",
                parameters.len(),
                arguments.len()
            ));
            return Err(());
        }
        Ok(Some((definition, Type::Named(name, arguments))))
    }

    fn probe_handler_action_logical_ty(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<Ty> {
        if let Some((payload, _)) = self.call_throws_info(expression, context) {
            return Some(payload);
        }
        match expression {
            Expr::Block(_, Some(tail)) | Expr::Unsafe(tail) | Expr::DoBlock { body: tail } => {
                self.probe_handler_action_logical_ty(tail, context)
            }
            _ => match self.probe_expr_ty(expression, None, context) {
                TypeProbe::Known(ty)
                | TypeProbe::KnownSource(ty, _)
                | TypeProbe::Defaultable(ty) => Some(ty),
                TypeProbe::Unsupported => None,
            },
        }
    }

    fn lower_effect_handler(
        &mut self,
        definition: &EffectDef,
        instance: &Type,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let diagnostic_count = self.diagnostics.len();
        let handle_protocol = self.lang_item_name(LangItemKind::Handle).to_owned();
        if !self.traits.get(&handle_protocol).is_some_and(|schema| {
            schema.valid && schema.self_parameter.kind == CompileParamKind::Effect
        }) {
            self.error(
                "effect handler lowering requires the validated `core.control.Handle` protocol",
            );
            return error_expr();
        }
        if groups.len() != 2 || groups[1].len() != 1 || groups[1][0].label.is_some() {
            self.error(format!(
                "`{}.handle` expects one labeled clause group followed by one trailing action closure",
                source_effect_identity(instance)
            ));
            return error_expr();
        }
        let Expr::Closure(action_parameters, action_body) = &groups[1][0].value else {
            self.error("an effect handler requires a trailing closure");
            return error_expr();
        };
        if !action_parameters.is_empty() {
            self.error("an effect handler action closure cannot take parameters");
            return error_expr();
        }

        let Type::Named(_, arguments) = instance else {
            unreachable!("resolved effect instances are nominal applications")
        };
        let substitutions = definition
            .compile_groups
            .iter()
            .flatten()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        let mut operations = definition.operations.clone();
        for operation in &mut operations {
            substitute_function_types(operation, &substitutions);
        }

        let mut clauses = HashMap::new();
        let mut handler_operations: HashMap<String, Vec<AlgebraicHandlerOperation>> =
            HashMap::new();
        for operation in &operations {
            let mut residual_effects = operation.effects.clone();
            residual_effects.custom.retain(|effect| {
                source_effect_identity(effect) != source_effect_identity(instance)
            });
            handler_operations
                .entry(operation.name.clone())
                .or_default()
                .push(AlgebraicHandlerOperation {
                    key: effect_operation_key(operation),
                    labels: effect_operation_labels(operation),
                    residual_effects,
                });
        }
        let mut done = None;
        for argument in groups[0] {
            let Some(label) = &argument.label else {
                self.error("effect handler clauses must use operation names as argument labels");
                continue;
            };
            let Expr::Closure(parameters, body) = &argument.value else {
                self.error(format!("handler clause `{label}` must be a closure"));
                continue;
            };
            if label == "done" {
                if done.is_some() {
                    self.error("duplicate handler clause `done`");
                    continue;
                }
                if parameters.len() != 1 {
                    self.error("handler clause `done` expects one result parameter");
                    continue;
                }
                done = Some(AlgebraicHandlerClause {
                    parameters: parameters.clone(),
                    resume: None,
                    body: (**body).clone(),
                    resume_input: None,
                });
                continue;
            }
            let candidates = operations
                .iter()
                .filter(|operation| operation.name == *label)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                self.error(format!(
                    "unknown handler clause `{label}` for effect `{}`",
                    source_effect_identity(instance)
                ));
                continue;
            }
            let operation = if candidates.len() == 1 {
                candidates[0]
            } else {
                let matching = candidates
                    .iter()
                    .copied()
                    .filter(|operation| {
                        let label_count = parameters.len().saturating_sub(usize::from(
                            operation_resume_input_source(operation).is_some(),
                        ));
                        let clause_labels = parameters
                            .iter()
                            .take(label_count)
                            .map(|parameter| parameter.name.as_str())
                            .collect::<Vec<_>>();
                        effect_operation_labels(operation)
                            .iter()
                            .map(String::as_str)
                            .eq(clause_labels.iter().copied())
                    })
                    .collect::<Vec<_>>();
                if matching.len() != 1 {
                    self.error(format!(
                        "overloaded handler clause `{label}` must name the operation parameters in declaration order before `resume`"
                    ));
                    continue;
                }
                matching[0]
            };
            let operation_key = effect_operation_key(operation);
            if clauses.contains_key(&operation_key) {
                self.error(format!("duplicate handler clause `{operation_key}`"));
                continue;
            }
            let operation_parameters = operation.groups.iter().flatten().collect::<Vec<_>>();
            let resume_input = operation_resume_input_source(operation);
            let expected_parameter_count =
                operation_parameters.len() + usize::from(resume_input.is_some());
            if parameters.len() != expected_parameter_count {
                if resume_input.is_some() {
                    self.error(format!(
                        "handler clause `{label}` expects {} operation parameter(s) followed by `resume`, found {} parameter(s)",
                        operation_parameters.len(),
                        parameters.len()
                    ));
                } else {
                    self.error(format!(
                        "handler clause `{label}` handles a `Never`-returning operation and expects {} operation parameter(s) without `resume`, found {} parameter(s)",
                        operation_parameters.len(),
                        parameters.len()
                    ));
                }
                continue;
            }
            let mut parameters = parameters.clone();
            for (parameter, declared) in parameters.iter_mut().zip(operation_parameters.iter()) {
                if parameter.ty == Type::Named("$context$infer".into(), Vec::new()) {
                    parameter.ty = declared.ty.clone();
                }
            }
            let resume = resume_input
                .is_some()
                .then(|| parameters.pop().expect("validated resume parameter").name);
            clauses.insert(
                operation_key,
                AlgebraicHandlerClause {
                    parameters,
                    resume,
                    body: (**body).clone(),
                    resume_input,
                },
            );
        }
        for operation in &operations {
            let key = effect_operation_key(operation);
            if !clauses.contains_key(&key) {
                let display = if handler_operations
                    .get(&operation.name)
                    .is_some_and(|candidates| candidates.len() > 1)
                {
                    key
                } else {
                    operation.name.clone()
                };
                self.error(format!(
                    "missing handler clause `{display}` for effect `{}`",
                    source_effect_identity(instance)
                ));
            }
        }
        if self.diagnostics.len() != diagnostic_count {
            return error_expr();
        }

        let inferred_from_action = if done.is_none() {
            self.probe_handler_action_logical_ty(action_body, context)
                .filter(|ty| !self.is_uninhabited_type(ty))
        } else {
            None
        };
        let inferred_handler_result = inferred_from_action
            .or_else(|| {
                done.is_none()
                    .then(|| {
                        handled_action_result_source(
                            action_body,
                            &source_effect_identity(instance),
                            &operations,
                        )
                        .map(|source| self.lower_source_type(&source))
                    })
                    .flatten()
            })
            .or_else(|| expected.cloned())
            .or_else(|| {
                let bodies = clauses
                    .values()
                    .map(|clause| clause.body.clone())
                    .collect::<Vec<_>>();
                bodies
                    .into_iter()
                    .find_map(|body| match self.probe_expr_ty(&body, None, context) {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => Some(ty),
                        TypeProbe::Unsupported => None,
                    })
            });
        let erased_callables = context
            .function_name
            .as_ref()
            .into_iter()
            .flat_map(|function_name| {
                self.runtime_handler_actions
                    .iter()
                    .filter(move |((candidate, _, _), action)| {
                        candidate == function_name
                            && action.effect == source_effect_identity(instance)
                    })
                    .filter_map(|((_, group_index, parameter_index), action)| {
                        let function = self.functions.get(function_name)?;
                        let parameter = function.groups.get(*group_index)?.get(*parameter_index)?;
                        Some((
                            parameter.name.clone(),
                            SourceErasedCallable {
                                output: self.source_type_for_ty(&action.output)?,
                                answer: self.source_type_for_ty(&action.answer)?,
                                accepts_input: action.accepts_input,
                            },
                        ))
                    })
            })
            .collect::<HashMap<_, _>>();
        let handler = Rc::new(AlgebraicHandler {
            identity: source_effect_identity(instance),
            source: instance.clone(),
            clauses,
            operations: handler_operations,
            lexical_unsafe_depth: Rc::new(Cell::new(context.unsafe_depth)),
            function_aliases: Rc::new(RefCell::new(HashMap::new())),
            resumable_closures: Rc::new(RefCell::new(HashMap::new())),
            dynamic_callables: Rc::new(RefCell::new(HashMap::new())),
            erased_callables,
            done,
            inlining: Rc::new(RefCell::new(HashMap::new())),
            loop_breaks: Rc::new(RefCell::new(HashMap::new())),
            result_source: inferred_handler_result
                .as_ref()
                .and_then(|ty| self.source_type_for_ty(ty)),
            return_continuations: Rc::new(RefCell::new(HashMap::new())),
        });
        let final_continuation: SourceContinuation = if let Some(done) = handler.done.clone() {
            let handler = handler.clone();
            Rc::new(move |analyzer, value| {
                let binding = Binding {
                    mutable: false,
                    name: done.parameters[0].name.clone(),
                    annotation: contextual_annotation(&done.parameters[0]),
                    value,
                };
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let body = analyzer.transform_handler_expr(
                    done.body.clone(),
                    handler.clone(),
                    None,
                    identity,
                )?;
                Ok(Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(body))))
            })
        } else {
            Rc::new(|_, value| Ok(value))
        };
        let handled_identity = handler.identity.clone();
        let transformed = match self.transform_handler_expr(
            (**action_body).clone(),
            handler,
            None,
            final_continuation,
        ) {
            Ok(expression) => expression,
            Err(()) => return error_expr(),
        };
        let newly_active = context
            .active_custom_effects
            .insert(handled_identity.clone());
        let previous_active_source = context
            .active_custom_effect_sources
            .insert(handled_identity.clone(), instance.clone());
        let newly_lexical = context
            .lexical_handler_effects
            .insert(handled_identity.clone());
        let previous_lexical_source = context
            .lexical_handler_effect_sources
            .insert(handled_identity.clone(), instance.clone());
        let lowered = self.lower_expr(&transformed, expected, context);
        if newly_active {
            context.active_custom_effects.remove(&handled_identity);
        }
        match previous_active_source {
            Some(source) => {
                context
                    .active_custom_effect_sources
                    .insert(handled_identity.clone(), source);
            }
            None => {
                context
                    .active_custom_effect_sources
                    .remove(&handled_identity);
            }
        }
        if newly_lexical {
            context.lexical_handler_effects.remove(&handled_identity);
        }
        match previous_lexical_source {
            Some(source) => {
                context
                    .lexical_handler_effect_sources
                    .insert(handled_identity, source);
            }
            None => {
                context
                    .lexical_handler_effect_sources
                    .remove(&handled_identity);
            }
        }
        lowered
    }

    fn transform_handler_expr(
        &mut self,
        expression: Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if let Expr::Name(name) = &expression {
            if handler.function_aliases.borrow().contains_key(name) {
                self.error(format!(
                    "effectful function alias `{name}` cannot escape its handler or be used as a runtime value"
                ));
                return Err(());
            }
            if handler.dynamic_callables.borrow().contains_key(name) {
                self.error(format!(
                    "dynamic effectful callable `{name}` cannot escape its handler as a runtime value"
                ));
                return Err(());
            }
        }
        if let Some((name, argument)) = internal_handler_return_argument(&expression) {
            let returned = handler
                .return_continuations
                .borrow()
                .get(&name)
                .cloned()
                .unwrap_or_else(|| Rc::new(|_, value| Ok(Expr::Return(Some(Box::new(value))))));
            return self.transform_handler_expr(argument, handler, resume, returned);
        }
        if let Some((name, argument)) = internal_handler_loop_break_argument(&expression) {
            let Some(loop_continuation) = handler.loop_breaks.borrow().get(&name).cloned() else {
                self.error("internal handler loop break escaped its continuation frame");
                return Err(());
            };
            return self.transform_handler_expr(argument, handler, resume, loop_continuation);
        }
        if let Some((operation, mut arguments)) =
            handled_operation_call(&expression, &handler.identity)
        {
            let candidates = handler
                .operations
                .get(&operation)
                .cloned()
                .unwrap_or_default();
            let labels = call_argument_labels(&arguments);
            if labels.is_none() && arguments.iter().any(|argument| argument.label.is_some()) {
                self.error(format!(
                    "cannot mix named and positional arguments in effect operation `{operation}`"
                ));
                return Err(());
            }
            let selected = match labels {
                Some(labels) => candidates
                    .iter()
                    .find(|candidate| candidate.labels == labels),
                None if candidates.len() == 1 => candidates.first(),
                None => {
                    self.error(format!(
                        "overloaded effect operation `{operation}` requires named arguments"
                    ));
                    return Err(());
                }
            };
            let Some(selected) = selected else {
                self.error(format!(
                    "no effect operation `{operation}` matches the supplied argument names"
                ));
                return Err(());
            };
            let Some(clause) = handler.clauses.get(&selected.key).cloned() else {
                self.error(format!("missing handler clause `{operation}`"));
                return Err(());
            };
            let residual_effects = selected.residual_effects.clone();
            if arguments.len() != clause.parameters.len() {
                self.error(format!(
                    "effect operation `{operation}` expects {} argument(s), found {}",
                    clause.parameters.len(),
                    arguments.len()
                ));
                return Err(());
            }
            for argument in &mut arguments {
                argument.label = None;
            }
            let handler_for_clause = handler.clone();
            let resume_for_arguments = resume.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut bindings = Vec::new();
                let mut residual_effects = residual_effects.clone();
                if handler_for_clause.lexical_unsafe_depth.get() > 0 {
                    analyzer.strip_authorized_unsafe_effects(&mut residual_effects);
                }
                if residual_effects != FunctionEffects::default() {
                    let gate_id = analyzer.next_closure;
                    analyzer.next_closure += 1;
                    let gate_name = format!("$handler$operation$effects${gate_id}");
                    bindings.push(Stmt::Let(Binding {
                        mutable: false,
                        name: gate_name.clone(),
                        annotation: Some(Type::Function {
                            groups: vec![Vec::new()],
                            effects: residual_effects.clone(),
                            result: Box::new(
                                analyzer.effect_abi_result_source(Type::Unit, &residual_effects),
                            ),
                        }),
                        value: Expr::Closure(Vec::new(), Box::new(Expr::Unit)),
                    }));
                    bindings.push(Stmt::Expr(Expr::Call(
                        Box::new(Expr::Name(gate_name)),
                        Vec::new(),
                    )));
                }
                bindings.extend(clause.parameters.iter().zip(arguments).map(
                    |(parameter, argument)| {
                        Stmt::Let(Binding {
                            mutable: false,
                            name: parameter.name.clone(),
                            annotation: contextual_annotation(parameter),
                            value: argument.value,
                        })
                    },
                ));
                if clause.resume_input.is_none() {
                    if clause.resume.is_some() {
                        analyzer.error(
                            "internal handler clause has a resume name but no continuation input",
                        );
                        return Err(());
                    }
                    let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                    let body = analyzer.transform_handler_expr(
                        clause.body.clone(),
                        handler_for_clause.clone(),
                        None,
                        identity,
                    )?;
                    return Ok(Expr::Block(bindings, Some(Box::new(body))));
                }
                let Some(input) = clause.resume_input.clone() else {
                    analyzer.error("effect operation is missing its continuation input type");
                    return Err(());
                };
                let Some(answer) = handler_for_clause.result_source.clone() else {
                    analyzer.error(
                        "an algebraic continuation requires a contextual handler answer type",
                    );
                    return Err(());
                };
                let continuation_id = analyzer.next_closure;
                analyzer.next_closure += 1;
                let runtime_name = format!("$handler$continuation${continuation_id}");
                let input_name = format!("$handler$resume$value${continuation_id}");
                let continuation_body = continuation(analyzer, Expr::Name(input_name.clone()))?;
                bindings.push(Stmt::Let(Binding {
                    mutable: true,
                    name: runtime_name.clone(),
                    annotation: Some(Type::Function {
                        groups: vec![vec![input.clone()]],
                        effects: FunctionEffects::default(),
                        result: Box::new(answer),
                    }),
                    value: Expr::Closure(
                        vec![Param {
                            mode: PassMode::Inferred,
                            access: None,
                            passing: None,
                            region: None,
                            name: input_name,
                            ty: input,
                        }],
                        Box::new(continuation_body),
                    ),
                }));
                let source_resume = SourceResume {
                    name: clause
                        .resume
                        .clone()
                        .expect("operation clauses have resume"),
                    runtime_name,
                    uses: Rc::new(Cell::new(0)),
                };
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let body = analyzer.transform_handler_expr(
                    clause.body.clone(),
                    handler_for_clause.clone(),
                    Some(source_resume),
                    identity,
                )?;
                Ok(Expr::Block(bindings, Some(Box::new(body))))
            });
            return self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume_for_arguments,
                completed,
            );
        }

        if let Some(source_resume) = &resume {
            if let Some(argument) = resume_call_argument(&expression, &source_resume.name) {
                let uses = source_resume.uses.get() + 1;
                source_resume.uses.set(uses);
                if uses > 1 {
                    self.error(format!(
                        "continuation `{}` is one-shot and cannot be resumed more than once",
                        source_resume.name
                    ));
                    return Err(());
                }
                let runtime_name = source_resume.runtime_name.clone();
                let current = continuation.clone();
                let invoked: SourceContinuation = Rc::new(move |analyzer, value| {
                    current(
                        analyzer,
                        Expr::Call(
                            Box::new(Expr::Name(runtime_name.clone())),
                            vec![CallArg { label: None, value }],
                        ),
                    )
                });
                return self.transform_handler_expr(argument, handler, resume, invoked);
            }
            if matches!(&expression, Expr::Name(name) if name == &source_resume.name) {
                self.error(format!(
                    "continuation `{}` cannot escape its handler clause",
                    source_resume.name
                ));
                return Err(());
            }
        }

        if matches!(&expression, Expr::Call(_, _)) {
            if let Some(result) = self.transform_erased_effect_callable_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_effectful_chain_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_nested_effect_handler(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_resumable_closure_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_dynamic_callable_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_effectful_named_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
        }

        match expression {
            Expr::Block(statements, tail) => self.transform_handler_block(
                statements,
                tail.map(|tail| *tail),
                handler,
                resume,
                continuation,
            ),
            Expr::Unary(operator, operand) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *operand,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(analyzer, Expr::Unary(operator, Box::new(value)))
                    }),
                )
            }
            Expr::Binary(left, operator, right) => {
                if matches!(operator, BinaryOp::And | BinaryOp::Or) {
                    let right = *right;
                    let handler_for_right = handler.clone();
                    let resume_for_right = resume.clone();
                    let next = continuation.clone();
                    return self.transform_handler_expr(
                        *left,
                        handler,
                        resume,
                        Rc::new(move |analyzer, left| {
                            let short_circuit = operator == BinaryOp::Or;
                            let next_for_right = next.clone();
                            let right_branch = analyzer.transform_handler_expr(
                                right.clone(),
                                handler_for_right.clone(),
                                resume_for_right.clone(),
                                next_for_right,
                            )?;
                            let short_value = next(analyzer, Expr::Bool(short_circuit))?;
                            Ok(Expr::If {
                                condition: Box::new(left),
                                then_branch: Box::new(if short_circuit {
                                    short_value.clone()
                                } else {
                                    right_branch.clone()
                                }),
                                else_branch: Some(Box::new(if short_circuit {
                                    right_branch
                                } else {
                                    short_value
                                })),
                            })
                        }),
                    );
                }
                let right = *right;
                let handler_for_right = handler.clone();
                let resume_for_right = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *left,
                    handler,
                    resume,
                    Rc::new(move |analyzer, left| {
                        let left = left.clone();
                        let next = next.clone();
                        analyzer.transform_handler_expr(
                            right.clone(),
                            handler_for_right.clone(),
                            resume_for_right.clone(),
                            Rc::new(move |analyzer, right| {
                                next(
                                    analyzer,
                                    Expr::Binary(Box::new(left.clone()), operator, Box::new(right)),
                                )
                            }),
                        )
                    }),
                )
            }
            Expr::Coalesce(left, right) => {
                let right = *right;
                let handler_for_fallback = handler.clone();
                let resume_for_fallback = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *left,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let payload_id = analyzer.next_closure;
                        analyzer.next_closure += 1;
                        let payload = format!("$handler$coalesce$payload${payload_id}");
                        let success = next(analyzer, Expr::Name(payload.clone()))?;
                        let fallback = analyzer.transform_handler_expr(
                            right.clone(),
                            handler_for_fallback.clone(),
                            resume_for_fallback.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerCoalesce {
                            scrutinee: Box::new(scrutinee),
                            payload,
                            success: Box::new(success),
                            fallback: Box::new(fallback),
                        })
                    }),
                )
            }
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => {
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let success = analyzer.transform_handler_expr(
                            (*success).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let fallback = analyzer.transform_handler_expr(
                            (*fallback).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerCoalesce {
                            scrutinee: Box::new(scrutinee),
                            payload: payload.clone(),
                            success: Box::new(success),
                            fallback: Box::new(fallback),
                        })
                    }),
                )
            }
            Expr::HandlerChainCall(chain) => {
                let HandlerChainCall {
                    scrutinee,
                    payload,
                    error,
                    member,
                    groups,
                    success,
                    residual,
                } = *chain;
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let success = analyzer.transform_handler_expr(
                            (*success).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let residual = analyzer.transform_handler_expr(
                            (*residual).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerChainCall(Box::new(HandlerChainCall {
                            scrutinee: Box::new(scrutinee),
                            payload: payload.clone(),
                            error: error.clone(),
                            member: member.clone(),
                            groups: groups.clone(),
                            success: Box::new(success),
                            residual: Box::new(residual),
                        })))
                    }),
                )
            }
            Expr::Assign(place, value) => {
                let place = *place;
                if let Expr::Name(destination) = &place {
                    let destination_callable =
                        handler.dynamic_callables.borrow().get(destination).cloned();
                    if let Some(destination_callable) = destination_callable {
                        let Expr::Name(source) = value.as_ref() else {
                            self.error(format!(
                                "dynamic effectful callable `{destination}` must be assigned from another compatible dynamic callable"
                            ));
                            return Err(());
                        };
                        let Some(source_callable) =
                            handler.dynamic_callables.borrow().get(source).cloned()
                        else {
                            self.error(format!(
                                "dynamic effectful callable `{destination}` cannot be assigned from `{source}`"
                            ));
                            return Err(());
                        };
                        if destination_callable.group_lengths != source_callable.group_lengths
                            || destination_callable.targets.len() != source_callable.targets.len()
                            || destination_callable.targets.iter().any(|target| {
                                !source_callable
                                    .targets
                                    .iter()
                                    .any(|source| source == target)
                            })
                        {
                            self.error(format!(
                                "dynamic effectful callable assignment from `{source}` to `{destination}` has an incompatible target set"
                            ));
                            return Err(());
                        }
                        let value = remap_dynamic_callable_tag(
                            source,
                            &source_callable.targets,
                            &destination_callable.targets,
                        );
                        return continuation(self, Expr::Assign(Box::new(place), Box::new(value)));
                    }
                }
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(
                            analyzer,
                            Expr::Assign(Box::new(place.clone()), Box::new(value)),
                        )
                    }),
                )
            }
            Expr::CompoundAssign(place, operator, value) => {
                let place = *place;
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(
                            analyzer,
                            Expr::CompoundAssign(
                                Box::new(place.clone()),
                                operator,
                                Box::new(value),
                            ),
                        )
                    }),
                )
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *condition,
                    handler,
                    resume,
                    Rc::new(move |analyzer, condition| {
                        let then_branch = analyzer.transform_handler_expr(
                            (*then_branch).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let else_branch = match &else_branch {
                            Some(branch) => Some(Box::new(analyzer.transform_handler_expr(
                                (**branch).clone(),
                                handler_for_branches.clone(),
                                resume_for_branches.clone(),
                                next.clone(),
                            )?)),
                            None => Some(Box::new(next(analyzer, Expr::Unit)?)),
                        };
                        Ok(Expr::If {
                            condition: Box::new(condition),
                            then_branch: Box::new(then_branch),
                            else_branch,
                        })
                    }),
                )
            }
            Expr::Array(elements) => {
                let arguments = elements
                    .into_iter()
                    .map(|value| CallArg { label: None, value })
                    .collect();
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, values| {
                    continuation(
                        analyzer,
                        Expr::Array(values.into_iter().map(|value| value.value).collect()),
                    )
                });
                self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed)
            }
            Expr::StructLiteral {
                constructor,
                fields,
            } => {
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, fields| {
                    continuation(
                        analyzer,
                        Expr::StructLiteral {
                            constructor: constructor.clone(),
                            fields,
                        },
                    )
                });
                self.transform_handler_arguments(fields, Vec::new(), handler, resume, completed)
            }
            Expr::Index { base, index } => {
                let index = *index;
                let handler_for_index = handler.clone();
                let resume_for_index = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        let base = base.clone();
                        let next = next.clone();
                        analyzer.transform_handler_expr(
                            index.clone(),
                            handler_for_index.clone(),
                            resume_for_index.clone(),
                            Rc::new(move |analyzer, index| {
                                next(
                                    analyzer,
                                    Expr::Index {
                                        base: Box::new(base.clone()),
                                        index: Box::new(index),
                                    },
                                )
                            }),
                        )
                    }),
                )
            }
            Expr::Member(base, member) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        next(analyzer, Expr::Member(Box::new(base), member.clone()))
                    }),
                )
            }
            Expr::ChainMember(base, member) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        next(analyzer, Expr::ChainMember(Box::new(base), member.clone()))
                    }),
                )
            }
            Expr::Try(value) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| next(analyzer, Expr::Try(Box::new(value)))),
                )
            }
            Expr::Throw(value) => {
                if standard_throws_error_source(
                    &handler.source,
                    self.lang_item_name(LangItemKind::ThrowsEffect),
                )
                .is_some()
                {
                    let operation = Expr::Call(
                        Box::new(Expr::Member(
                            Box::new(source_type_expression(&handler.source)),
                            "raise".to_owned(),
                        )),
                        vec![CallArg {
                            label: None,
                            value: *value,
                        }],
                    );
                    return self.transform_handler_expr(operation, handler, resume, continuation);
                }
                let identity: SourceContinuation =
                    Rc::new(|_, value| Ok(Expr::Throw(Box::new(value))));
                self.transform_handler_expr(*value, handler, resume, identity)
            }
            Expr::Unsafe(value) => {
                let next = continuation.clone();
                let depth = handler.lexical_unsafe_depth.get();
                handler.lexical_unsafe_depth.set(depth + 1);
                let transformed = self.transform_handler_expr(
                    *value,
                    handler.clone(),
                    resume,
                    Rc::new(move |analyzer, value| next(analyzer, Expr::Unsafe(Box::new(value)))),
                );
                handler.lexical_unsafe_depth.set(depth);
                transformed
            }
            Expr::DoBlock { body } => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *body,
                    handler,
                    resume,
                    Rc::new(move |analyzer, body| {
                        next(
                            analyzer,
                            Expr::DoBlock {
                                body: Box::new(body),
                            },
                        )
                    }),
                )
            }
            Expr::Match { scrutinee, arms } => {
                let has_effectful_guard = arms.iter().any(|arm| {
                    arm.guard
                        .as_ref()
                        .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler))
                });
                if has_effectful_guard {
                    let can_delay_pattern_transfers =
                        arms.iter().enumerate().all(|(index, arm)| {
                            let guard_is_effectful = arm.guard.as_ref().is_some_and(|guard| {
                                self.handler_expression_may_suspend(guard, &handler)
                            });
                            guard_is_effectful
                                || !pattern_contains_binding(&arm.pattern)
                                || !arms[index + 1..].iter().any(|later| {
                                    later.guard.as_ref().is_some_and(|guard| {
                                        self.handler_expression_may_suspend(guard, &handler)
                                    })
                                })
                        });
                    let handler_for_arms = handler.clone();
                    let resume_for_arms = resume.clone();
                    let next = continuation.clone();
                    return self.transform_handler_expr(
                        *scrutinee,
                        handler,
                        resume,
                        Rc::new(move |analyzer, scrutinee| {
                            let match_id = analyzer.next_closure;
                            analyzer.next_closure += 1;
                            let input = if can_delay_pattern_transfers {
                                format!("$handler$match$inspect$input${match_id}")
                            } else {
                                format!("$handler$match$input${match_id}")
                            };
                            let candidates = analyzer.transform_handler_match_candidates(
                                &input,
                                &arms,
                                handler_for_arms.clone(),
                                resume_for_arms.clone(),
                                next.clone(),
                            )?;
                            Ok(Expr::Block(
                                vec![Stmt::Let(Binding {
                                    mutable: false,
                                    name: input,
                                    annotation: None,
                                    value: scrutinee,
                                })],
                                Some(Box::new(candidates)),
                            ))
                        }),
                    );
                }
                let handler_for_arms = handler.clone();
                let resume_for_arms = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let mut transformed = Vec::with_capacity(arms.len());
                        for arm in &arms {
                            transformed.push(MatchArm {
                                pattern: arm.pattern.clone(),
                                guard: arm.guard.clone(),
                                body: analyzer.transform_handler_expr(
                                    arm.body.clone(),
                                    handler_for_arms.clone(),
                                    resume_for_arms.clone(),
                                    next.clone(),
                                )?,
                            });
                        }
                        Ok(Expr::Match {
                            scrutinee: Box::new(scrutinee),
                            arms: transformed,
                        })
                    }),
                )
            }
            Expr::While { condition, body } => {
                self.transform_handler_loop(Some(*condition), *body, handler, resume, continuation)
            }
            Expr::Loop { body } => {
                self.transform_handler_loop(None, *body, handler, resume, continuation)
            }
            Expr::Call(callee, arguments) => {
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                    continuation(analyzer, Expr::Call(callee.clone(), arguments))
                });
                self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed)
            }
            other => continuation(self, other),
        }
    }

    fn transform_erased_effect_callable_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let action = handler.erased_callables.get(name)?.clone();
        if groups.len() != 1
            || groups[0].len() != usize::from(action.accepts_input)
            || groups[0].iter().any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "erased effect callable `{name}` must be fully applied with {} positional input(s)",
                usize::from(action.accepts_input)
            ));
            return Some(Err(()));
        }
        let arguments = groups[0].to_vec();
        let action_name = name.clone();
        let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
            let specialization = analyzer.next_closure;
            analyzer.next_closure += 1;
            let continuation_name = format!("$handler$erased$action$continuation${specialization}");
            let continuation_value_name = format!("$handler$erased$action$value${specialization}");
            let continuation_body =
                continuation(analyzer, Expr::Name(continuation_value_name.clone()))?;
            let continuation_binding = Binding {
                mutable: true,
                name: continuation_name.clone(),
                annotation: Some(Type::Function {
                    groups: vec![vec![action.output.clone()]],
                    effects: FunctionEffects::default(),
                    result: Box::new(action.answer.clone()),
                }),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: continuation_value_name,
                        ty: action.output.clone(),
                    }],
                    Box::new(continuation_body),
                ),
            };
            let erased_continuation_name =
                format!("$handler$erased$action$continuation$value${specialization}");
            let erased_continuation_binding = Binding {
                mutable: true,
                name: erased_continuation_name.clone(),
                annotation: Some(Type::Named(
                    analyzer
                        .lang_item_name(LangItemKind::Continuation)
                        .to_owned(),
                    vec![action.output.clone(), action.answer.clone()],
                )),
                value: Expr::Call(
                    Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(continuation_name),
                    }],
                ),
            };
            let input = arguments
                .into_iter()
                .next()
                .map(|argument| argument.value)
                .unwrap_or(Expr::Unit);
            let invoke = Expr::Call(
                Box::new(Expr::Name("$handler$invoke$effect$callable".to_owned())),
                vec![
                    CallArg {
                        label: None,
                        value: Expr::Name(action_name.clone()),
                    },
                    CallArg {
                        label: None,
                        value: input,
                    },
                    CallArg {
                        label: None,
                        value: Expr::Name(erased_continuation_name),
                    },
                ],
            );
            Ok(Expr::Block(
                vec![
                    Stmt::Let(continuation_binding),
                    Stmt::Let(erased_continuation_binding),
                ],
                Some(Box::new(invoke)),
            ))
        });
        Some(self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed))
    }

    fn transform_effectful_chain_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut source_groups = Vec::new();
        let Expr::ChainMember(base, member) = flatten_call(expression, &mut source_groups) else {
            return None;
        };
        let base_may_suspend = self.handler_expression_may_suspend(base, &handler);
        let arguments_may_suspend = source_groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler));
        if !base_may_suspend && !arguments_may_suspend {
            return None;
        }
        let groups = source_groups
            .iter()
            .map(|group| group.to_vec())
            .collect::<Vec<_>>();
        let member = member.clone();
        let handler_for_call = handler.clone();
        let resume_for_call = resume.clone();
        Some(self.transform_handler_expr(
            (**base).clone(),
            handler,
            resume,
            Rc::new(move |analyzer, scrutinee| {
                let id = analyzer.next_closure;
                analyzer.next_closure += 1;
                let payload = format!("$handler$chain$payload${id}");
                let error = format!("$handler$chain$error${id}");
                let success_wrap = |value| {
                    Expr::Call(
                        Box::new(Expr::Name("$handler$chain$wrap$success".to_owned())),
                        vec![CallArg { label: None, value }],
                    )
                };
                let residual_wrap = Expr::Call(
                    Box::new(Expr::Name("$handler$chain$wrap$residual".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(error.clone()),
                    }],
                );
                let residual = continuation(analyzer, residual_wrap)?;
                let completed = {
                    let continuation = continuation.clone();
                    Rc::new(move |analyzer: &mut Analyzer, value: Expr| {
                        continuation(analyzer, success_wrap(value))
                    }) as SourceContinuation
                };
                let callee = Expr::Member(Box::new(Expr::Name(payload.clone())), member.clone());
                let success = analyzer.transform_handler_call_groups(
                    callee,
                    groups.clone(),
                    handler_for_call.clone(),
                    resume_for_call.clone(),
                    completed,
                )?;
                Ok(Expr::HandlerChainCall(Box::new(HandlerChainCall {
                    scrutinee: Box::new(scrutinee),
                    payload,
                    error,
                    member: member.clone(),
                    groups: groups.clone(),
                    success: Box::new(success),
                    residual: Box::new(residual),
                })))
            }),
        ))
    }

    fn transform_handler_call_groups(
        &mut self,
        callee: Expr,
        mut groups: Vec<Vec<CallArg>>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if groups.is_empty() {
            return continuation(self, callee);
        }
        let arguments = groups.remove(0);
        let next_handler = handler.clone();
        let next_resume = resume.clone();
        self.transform_handler_arguments(
            arguments,
            Vec::new(),
            handler,
            resume,
            Rc::new(move |analyzer, arguments| {
                analyzer.transform_handler_call_groups(
                    Expr::Call(Box::new(callee.clone()), arguments),
                    groups.clone(),
                    next_handler.clone(),
                    next_resume.clone(),
                    continuation.clone(),
                )
            }),
        )
    }

    fn transform_handler_match_candidates(
        &mut self,
        scrutinee: &str,
        arms: &[MatchArm],
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if !arms.iter().any(|arm| {
            arm.guard
                .as_ref()
                .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler))
        }) {
            let mut transformed = Vec::with_capacity(arms.len());
            for arm in arms {
                transformed.push(MatchArm {
                    pattern: arm.pattern.clone(),
                    guard: arm.guard.clone(),
                    body: self.transform_handler_expr(
                        arm.body.clone(),
                        handler.clone(),
                        resume.clone(),
                        continuation.clone(),
                    )?,
                });
            }
            return Ok(handler_match_commit(scrutinee, transformed));
        }
        let Some((arm, remaining)) = arms.split_first() else {
            return Ok(Expr::Loop {
                body: Box::new(Expr::Unit),
            });
        };
        let body = self.transform_handler_expr(
            arm.body.clone(),
            handler.clone(),
            resume.clone(),
            continuation.clone(),
        )?;
        let covers_all = matches!(arm.pattern, Pattern::Wildcard | Pattern::Binding(_));
        let guard_is_effectful = arm
            .guard
            .as_ref()
            .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler));
        let delays_pattern_transfer = guard_is_effectful;

        let committed_body = if delays_pattern_transfer {
            handler_match_commit(
                scrutinee,
                vec![
                    MatchArm {
                        pattern: arm.pattern.clone(),
                        guard: None,
                        body: body.clone(),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: Expr::Loop {
                            body: Box::new(Expr::Unit),
                        },
                    },
                ],
            )
        } else {
            body.clone()
        };

        let candidate = if let Some(guard) = &arm.guard {
            if guard_is_effectful {
                let false_branch = self.transform_handler_match_candidates(
                    scrutinee,
                    remaining,
                    handler.clone(),
                    resume.clone(),
                    continuation.clone(),
                )?;
                let true_branch = committed_body;
                self.transform_handler_expr(
                    guard.clone(),
                    handler.clone(),
                    resume.clone(),
                    Rc::new(move |_, condition| {
                        Ok(Expr::If {
                            condition: Box::new(condition),
                            then_branch: Box::new(true_branch.clone()),
                            else_branch: Some(Box::new(false_branch.clone())),
                        })
                    }),
                )?
            } else {
                body
            }
        } else {
            body
        };

        let mut candidates = vec![MatchArm {
            pattern: if delays_pattern_transfer {
                pattern_for_suspended_guard(
                    &arm.pattern,
                    arm.guard.as_ref().expect("effectful guard exists"),
                )
            } else {
                arm.pattern.clone()
            },
            guard: if guard_is_effectful {
                None
            } else {
                arm.guard.clone()
            },
            body: candidate,
        }];
        if !covers_all || arm.guard.is_some() {
            let fallback = self.transform_handler_match_candidates(
                scrutinee,
                remaining,
                handler,
                resume,
                continuation,
            )?;
            candidates.push(MatchArm {
                pattern: Pattern::Wildcard,
                guard: None,
                body: fallback,
            });
        }
        Ok(Expr::Match {
            scrutinee: Box::new(Expr::Name(scrutinee.to_owned())),
            arms: candidates,
        })
    }

    fn handler_expression_may_suspend(
        &self,
        expression: &Expr,
        handler: &AlgebraicHandler,
    ) -> bool {
        if handled_operation_call(expression, &handler.identity).is_some() {
            return true;
        }
        if matches!(expression, Expr::Call(_, _)) {
            let mut groups = Vec::new();
            if let Expr::Name(name) = flatten_call(expression, &mut groups) {
                if handler.resumable_closures.borrow().contains_key(name)
                    || handler.dynamic_callables.borrow().contains_key(name)
                    || handler.erased_callables.contains_key(name)
                {
                    return true;
                }
                if self.functions.get(name).is_some_and(|function| {
                    function
                        .effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                }) {
                    return true;
                }
            }
        }
        handler_expression_children(expression)
            .into_iter()
            .any(|child| self.handler_expression_may_suspend(child, handler))
    }

    fn transform_nested_effect_handler(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let Expr::Call(inner_callee, action_arguments) = expression else {
            return None;
        };
        let [CallArg {
            label: None,
            value: Expr::Closure(action_parameters, action_body),
        }] = action_arguments.as_slice()
        else {
            return None;
        };
        if !action_parameters.is_empty() {
            return None;
        }
        let mut groups = Vec::new();
        let Expr::Member(effect, member) = flatten_call(inner_callee, &mut groups) else {
            return None;
        };
        if member != "handle" || groups.len() != 1 {
            return None;
        }
        let effect_name = source_type_expression_name(effect)?;
        let root_name = effect_name.split('(').next().unwrap_or(&effect_name);
        if !self.effect_defs.contains_key(root_name) || effect_name == handler.identity {
            return None;
        }

        let Expr::Call(handler_head, clause_arguments) = inner_callee.as_ref() else {
            return None;
        };
        let mut transformed_clauses = Vec::with_capacity(clause_arguments.len());
        for argument in clause_arguments {
            let value = if let Expr::Closure(parameters, body) = &argument.value {
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let transformed = match self.transform_handler_expr(
                    (**body).clone(),
                    handler.clone(),
                    None,
                    identity,
                ) {
                    Ok(transformed) => transformed,
                    Err(()) => return Some(Err(())),
                };
                Expr::Closure(parameters.clone(), Box::new(transformed))
            } else {
                argument.value.clone()
            };
            transformed_clauses.push(CallArg {
                label: argument.label.clone(),
                value,
            });
        }
        let transformed_inner_callee =
            Expr::Call(Box::new((**handler_head).clone()), transformed_clauses);

        let wrap_in_unsafe = handler.lexical_unsafe_depth.get() > 0;
        let transformed_action = match self.transform_handler_expr(
            (**action_body).clone(),
            handler,
            resume,
            continuation,
        ) {
            Ok(transformed) => transformed,
            Err(()) => return Some(Err(())),
        };
        let call = Expr::Call(
            Box::new(transformed_inner_callee),
            vec![CallArg {
                label: None,
                value: Expr::Closure(Vec::new(), Box::new(transformed_action)),
            }],
        );
        Some(Ok(if wrap_in_unsafe {
            Expr::Unsafe(Box::new(call))
        } else {
            call
        }))
    }

    fn transform_handler_arguments(
        &mut self,
        mut remaining: Vec<CallArg>,
        completed_arguments: Vec<CallArg>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        completed: SourceArgumentsContinuation,
    ) -> Result<Expr, ()> {
        if remaining.is_empty() {
            return completed(self, completed_arguments);
        }
        let argument = remaining.remove(0);
        let label = argument.label.clone();
        let next_handler = handler.clone();
        let next_resume = resume.clone();
        self.transform_handler_expr(
            argument.value,
            handler,
            resume,
            Rc::new(move |analyzer, value| {
                let mut arguments = completed_arguments.clone();
                arguments.push(CallArg {
                    label: label.clone(),
                    value,
                });
                analyzer.transform_handler_arguments(
                    remaining.clone(),
                    arguments,
                    next_handler.clone(),
                    next_resume.clone(),
                    completed.clone(),
                )
            }),
        )
    }

    fn transform_handler_loop(
        &mut self,
        condition: Option<Expr>,
        mut body: Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        let Some(result_source) = handler.result_source.clone() else {
            self.error("a handler containing a resumable loop requires a contextual result type");
            return Err(());
        };
        let specialization = self.next_closure;
        self.next_closure += 1;
        let recursive_name = format!("$handler$recursive$loop${specialization}");
        let break_name = format!("$handler$loop$break${specialization}");
        rewrite_handler_loop_control(&mut body, &recursive_name, &break_name, 0);
        let recursive_call =
            || Expr::Call(Box::new(Expr::Name(recursive_name.clone())), Vec::new());
        let break_call = |value| {
            Expr::Call(
                Box::new(Expr::Name(break_name.clone())),
                vec![CallArg { label: None, value }],
            )
        };
        let iteration = if let Some(condition) = condition {
            Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(Expr::Block(
                    vec![Stmt::Expr(body)],
                    Some(Box::new(recursive_call())),
                )),
                else_branch: Some(Box::new(break_call(Expr::Unit))),
            }
        } else {
            Expr::Block(vec![Stmt::Expr(body)], Some(Box::new(recursive_call())))
        };
        handler
            .loop_breaks
            .borrow_mut()
            .insert(break_name, continuation);
        let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
        let transformed = self.transform_handler_expr(iteration, handler.clone(), resume, identity);
        handler
            .loop_breaks
            .borrow_mut()
            .remove(&format!("$handler$loop$break${specialization}"));
        let transformed = transformed?;
        let frame_name = format!("$handler$loop$frame${specialization}");
        let frame = Binding {
            mutable: true,
            name: frame_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![Vec::new()],
                effects: FunctionEffects::default(),
                result: Box::new(result_source),
            }),
            value: Expr::Closure(Vec::new(), Box::new(transformed)),
        };
        Ok(Expr::Block(
            vec![Stmt::Let(frame)],
            Some(Box::new(Expr::Call(
                Box::new(Expr::Name(frame_name)),
                Vec::new(),
            ))),
        ))
    }

    fn transform_dynamic_callable_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let callable = handler.dynamic_callables.borrow().get(name).cloned()?;
        if groups.len() != callable.group_lengths.len()
            || groups
                .iter()
                .zip(&callable.group_lengths)
                .any(|(arguments, expected)| arguments.len() != *expected)
        {
            self.error(format!(
                "dynamic effectful callable `{name}` must be fully applied under its handler"
            ));
            return Some(Err(()));
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let group_lengths = callable.group_lengths.clone();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = name.clone();
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_dynamic_callable_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler lost its dynamic callable after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }
        let rebuild = |target: &str| {
            let mut call = Expr::Name(target.to_owned());
            for group in &groups {
                call = Expr::Call(Box::new(call), group.to_vec());
            }
            call
        };
        let mut branches = Vec::with_capacity(callable.targets.len());
        for target in &callable.targets {
            match self.transform_handler_expr(
                rebuild(target),
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                Ok(branch) => branches.push(branch),
                Err(()) => return Some(Err(())),
            }
        }
        let Some(mut dispatch) = branches.pop() else {
            self.error("internal dynamic callable has no dispatch targets");
            return Some(Err(()));
        };
        for (index, branch) in branches.into_iter().enumerate().rev() {
            dispatch = Expr::If {
                condition: Box::new(Expr::Binary(
                    Box::new(Expr::Name(name.clone())),
                    BinaryOp::Eq,
                    Box::new(Expr::Integer(index as i128)),
                )),
                then_branch: Box::new(branch),
                else_branch: Some(Box::new(dispatch)),
            };
        }
        Some(Ok(dispatch))
    }

    fn transform_resumable_closure_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let closure = handler.resumable_closures.borrow().get(name).cloned()?;
        if groups.len() != closure.group_lengths.len()
            || groups
                .iter()
                .zip(&closure.group_lengths)
                .any(|(arguments, expected)| arguments.len() != *expected)
        {
            self.error(format!(
                "resumable closure `{name}` must be fully applied before it can run under a handler"
            ));
            return Some(Err(()));
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let group_lengths = closure.group_lengths.clone();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = name.clone();
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_resumable_closure_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler lost its resumable closure after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }

        let specialization = self.next_closure;
        self.next_closure += 1;
        let value_name = format!("$handler$closure$continuation$value${specialization}");
        let continuation_name = format!("$handler$closure$continuation${specialization}");
        let continuation_body = match continuation(self, Expr::Name(value_name.clone())) {
            Ok(body) => body,
            Err(()) => return Some(Err(())),
        };
        let continuation_binding = Binding {
            mutable: true,
            name: continuation_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![vec![closure.input.clone()]],
                effects: FunctionEffects::default(),
                result: Box::new(closure.answer.clone()),
            }),
            value: Expr::Closure(
                vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    passing: None,
                    region: None,
                    name: value_name,
                    ty: closure.input.clone(),
                }],
                Box::new(continuation_body),
            ),
        };
        let erased_name = format!("$handler$erased$closure$continuation${specialization}");
        let erased_binding = Binding {
            mutable: true,
            name: erased_name.clone(),
            annotation: Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![closure.input, closure.answer],
            )),
            value: Expr::Call(
                Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                vec![CallArg {
                    label: None,
                    value: Expr::Name(continuation_name),
                }],
            ),
        };
        let mut call = Expr::Name(name.clone());
        for (index, group) in groups.iter().enumerate() {
            let mut arguments = group.to_vec();
            if index + 1 == groups.len() {
                arguments.push(CallArg {
                    label: None,
                    value: Expr::Name(erased_name.clone()),
                });
            }
            call = Expr::Call(Box::new(call), arguments);
        }
        Some(Ok(Expr::Block(
            vec![Stmt::Let(continuation_binding), Stmt::Let(erased_binding)],
            Some(Box::new(call)),
        )))
    }

    fn transform_resumable_closure_binding(
        &mut self,
        binding: &Binding,
        handler: Rc<AlgebraicHandler>,
    ) -> Option<Result<(Binding, SourceResumableClosure), ()>> {
        let Some(Type::Function {
            groups,
            effects,
            result,
        }) = binding.annotation.as_ref()
        else {
            return None;
        };
        if !effects
            .custom
            .iter()
            .any(|effect| source_effect_identity(effect) == handler.identity)
        {
            return None;
        }
        if !matches!(binding.value, Expr::Closure(_, _)) {
            return None;
        }
        let Some(answer) = handler.result_source.clone() else {
            self.error("a resumable closure requires a contextual handler answer type");
            return Some(Err(()));
        };
        let input = logical_effect_result_source(result, effects);
        let specialization = self.next_closure;
        self.next_closure += 1;
        let continuation_name = format!("$handler$closure$frame$continuation${specialization}");
        let continuation_ty = Type::Named(
            self.lang_item_name(LangItemKind::Continuation).to_owned(),
            vec![input.clone(), answer.clone()],
        );
        let mut value = binding.value.clone();
        let Some(body) = append_innermost_closure_parameter(
            &mut value,
            Param {
                mode: PassMode::Move,
                access: None,
                passing: None,
                region: None,
                name: continuation_name.clone(),
                ty: continuation_ty.clone(),
            },
        ) else {
            self.error("internal resumable closure binding lost its closure value");
            return Some(Err(()));
        };
        let tail_continuation_name = continuation_name.clone();
        let tail: SourceContinuation = Rc::new(move |_, value| {
            Ok(Expr::Call(
                Box::new(Expr::Name("$handler$invoke$continuation".to_owned())),
                vec![
                    CallArg {
                        label: None,
                        value: Expr::Name(tail_continuation_name.clone()),
                    },
                    CallArg { label: None, value },
                ],
            ))
        });
        let return_name = format!("$handler$closure$return${specialization}");
        rewrite_handler_returns(body, &return_name);
        handler
            .return_continuations
            .borrow_mut()
            .insert(return_name.clone(), tail.clone());
        let transformed = self.transform_handler_expr(body.clone(), handler.clone(), None, tail);
        handler
            .return_continuations
            .borrow_mut()
            .remove(&return_name);
        let transformed = match transformed {
            Ok(transformed) => transformed,
            Err(()) => return Some(Err(())),
        };
        *body = transformed;

        let mut rewritten_groups = groups.clone();
        let Some(last_group) = rewritten_groups.last_mut() else {
            self.error("a resumable closure type requires a runtime parameter group");
            return Some(Err(()));
        };
        last_group.push(continuation_ty);
        let mut rewritten_effects = effects.clone();
        rewritten_effects
            .custom
            .retain(|effect| source_effect_identity(effect) != handler.identity);
        if handler.lexical_unsafe_depth.get() > 0 {
            self.strip_authorized_unsafe_effects(&mut rewritten_effects);
        }
        let rewritten_result = self.effect_abi_result_source(answer.clone(), &rewritten_effects);
        let rewritten = Binding {
            mutable: binding.mutable,
            name: binding.name.clone(),
            annotation: Some(Type::Function {
                groups: rewritten_groups,
                effects: rewritten_effects,
                result: Box::new(rewritten_result),
            }),
            value,
        };
        Some(Ok((
            rewritten,
            SourceResumableClosure {
                input,
                answer,
                group_lengths: groups.iter().map(Vec::len).collect(),
            },
        )))
    }

    fn explicit_generic_handler_function(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        handled_effect: &Type,
    ) -> Option<Result<(String, Function, usize), ()>> {
        let template = self.function_templates.get(name)?.clone();
        if groups.len() > template.compile_groups.len() + template.groups.len() {
            return None;
        }
        let inference_context = LowerCtx::for_global(ItemOrigin::default());
        let (compile_parameters, mut inferred, runtime_group_start) = match self
            .seed_type_argument_inference(
                name,
                &template.compile_groups,
                groups,
                &inference_context,
                false,
            ) {
            Some(inferred) => inferred,
            None => return Some(Err(())),
        };
        let mut matched_effect = false;
        for effect in &template.effects.custom {
            let mut effect = effect.clone();
            let substitutions = inferred
                .iter()
                .filter_map(|(name, inferred)| {
                    inferred
                        .source
                        .clone()
                        .or_else(|| self.source_type_for_ty(&inferred.ty))
                        .map(|source| (name.clone(), source))
                })
                .collect::<HashMap<_, _>>();
            substitute_type_parameters(&mut effect, &substitutions);
            let mut candidate = inferred.clone();
            if self
                .unify_source_template(
                    &effect,
                    handled_effect,
                    &compile_parameters,
                    &mut candidate,
                    "handled effect",
                )
                .is_ok()
            {
                inferred = candidate;
                matched_effect = true;
                break;
            }
        }
        if !matched_effect {
            return None;
        }
        let runtime_groups = &groups[runtime_group_start..];
        if runtime_groups.len() > template.groups.len() {
            return None;
        }
        let mut ordered_runtime_groups = Vec::new();
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&template.groups).enumerate()
        {
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) =
                self.ordered_call_arguments(name, group_index + 1, arguments, &parameter_names)
            else {
                return Some(Err(()));
            };
            ordered_runtime_groups.push(ordered);
        }
        let constraints = ordered_runtime_groups
            .iter()
            .zip(&template.groups)
            .enumerate()
            .flat_map(|(group_index, (arguments, parameters))| {
                arguments
                    .iter()
                    .zip(parameters)
                    .map(move |(argument, parameter)| {
                        (
                            parameter.ty.clone(),
                            argument.value.clone(),
                            format!(
                                "argument for parameter `{}` in group {}",
                                parameter.name,
                                group_index + 1
                            ),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let unsupported = match self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            &inference_context,
        ) {
            Some(unsupported) => unsupported,
            None => return Some(Err(())),
        };
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) = match self.finish_type_argument_inference(
            name,
            &ordered_parameters,
            &inferred,
            unsupported,
        ) {
            Some(arguments) => arguments,
            None => return Some(Err(())),
        };
        let canonical = match self.ensure_function_instance(name, source_arguments, arguments) {
            Some(canonical) => canonical,
            None => return Some(Err(())),
        };
        let function = self
            .functions
            .get(&canonical)
            .cloned()
            .expect("created generic function instance is registered");
        Some(Ok((canonical, function, runtime_group_start)))
    }

    fn transform_effectful_named_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(source_name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let original_groups = groups.clone();
        let aliased_name = handler.function_aliases.borrow().get(source_name).cloned();
        let source_name = source_name.clone();
        let selected_name = aliased_name.as_deref().unwrap_or(&source_name);
        let (name, function, runtime_group_start) =
            if let Some(function) = self.functions.get(selected_name).cloned() {
                (selected_name.to_owned(), function, 0)
            } else if let Some(resolved) =
                self.explicit_generic_handler_function(selected_name, &groups, &handler.source)
            {
                match resolved {
                    Ok(resolved) => resolved,
                    Err(()) => return Some(Err(())),
                }
            } else {
                return None;
            };
        groups = groups[runtime_group_start..].to_vec();
        if !function
            .effects
            .custom
            .iter()
            .any(|effect| source_effect_identity(effect) == handler.identity)
        {
            return None;
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let compile_prefix = original_groups[..runtime_group_start]
                .iter()
                .map(|group| group.to_vec())
                .collect::<Vec<_>>();
            let group_lengths = groups.iter().map(|group| group.len()).collect::<Vec<_>>();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = source_name;
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for group in &compile_prefix {
                    call = Expr::Call(Box::new(call), group.clone());
                }
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_effectful_named_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler call lost its effectful target after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }
        if let Some(frame) = handler.inlining.borrow().get(&name).cloned() {
            let specialization = self.next_closure;
            self.next_closure += 1;
            let value_name = format!("$handler$recursive$continuation$value${specialization}");
            let continuation_name = format!("$handler$recursive$continuation${specialization}");
            let erased_name = format!("$handler$erased$recursive$continuation${specialization}");
            let continuation_body = match continuation(self, Expr::Name(value_name.clone())) {
                Ok(body) => body,
                Err(()) => return Some(Err(())),
            };
            let continuation_binding = Binding {
                mutable: true,
                name: continuation_name.clone(),
                annotation: Some(Type::Function {
                    groups: vec![vec![frame.input.clone()]],
                    effects: FunctionEffects::default(),
                    result: Box::new(frame.answer.clone()),
                }),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: value_name,
                        ty: frame.input.clone(),
                    }],
                    Box::new(continuation_body),
                ),
            };
            let erased_binding = Binding {
                mutable: true,
                name: erased_name.clone(),
                annotation: Some(Type::Named(
                    self.lang_item_name(LangItemKind::Continuation).to_owned(),
                    vec![frame.input, frame.answer],
                )),
                value: Expr::Call(
                    Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(continuation_name),
                    }],
                ),
            };
            let mut recursive_arguments = groups
                .iter()
                .flat_map(|group| group.iter())
                .map(|argument| CallArg {
                    label: None,
                    value: argument.value.clone(),
                })
                .collect::<Vec<_>>();
            recursive_arguments.push(CallArg {
                label: None,
                value: Expr::Name(erased_name),
            });
            let recursive_call = Expr::Call(
                Box::new(Expr::Name(frame.recursive_name)),
                recursive_arguments,
            );
            return Some(Ok(Expr::Block(
                vec![Stmt::Let(continuation_binding), Stmt::Let(erased_binding)],
                Some(Box::new(recursive_call)),
            )));
        }
        let Some(_) = function.body else {
            self.error(format!(
                "effectful function `{name}` has no source body available for handler lowering"
            ));
            return Some(Err(()));
        };
        if groups.len() != function.groups.len()
            || groups
                .iter()
                .zip(&function.groups)
                .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            self.error(format!(
                "effectful function `{name}` must be fully applied before it can run under a handler"
            ));
            return Some(Err(()));
        }
        let specialization = self.next_closure;
        let recursive_name = format!("$handler$recursive${specialization}");
        let prefix = format!("$handler$frame${specialization}${name}$");
        self.next_closure += 1;
        let (parameters, mut body) = hygienic_inline_function(&function, &prefix);
        let mut source_arguments = Vec::new();
        for (arguments, declared) in groups.iter().zip(&function.groups) {
            for (argument, declared) in arguments.iter().zip(declared) {
                if argument
                    .label
                    .as_deref()
                    .is_some_and(|label| label != declared.name)
                {
                    self.error(format!(
                        "unknown argument label on effectful call `{name}`: expected `{}`",
                        declared.name
                    ));
                    return Some(Err(()));
                }
                source_arguments.push(CallArg {
                    label: None,
                    value: argument.value.clone(),
                });
            }
        }
        let mut omitted_parameters = HashSet::new();
        let mut static_function_values = HashMap::new();
        for (index, (parameter, argument)) in parameters
            .iter()
            .flatten()
            .zip(&source_arguments)
            .enumerate()
        {
            if !matches!(parameter.ty, Type::Function { .. }) {
                continue;
            }
            let Expr::Name(argument_name) = &argument.value else {
                continue;
            };
            let target = handler
                .function_aliases
                .borrow()
                .get(argument_name)
                .cloned()
                .unwrap_or_else(|| argument_name.clone());
            if self.functions.contains_key(&target)
                || handler.resumable_closures.borrow().contains_key(&target)
                || handler.dynamic_callables.borrow().contains_key(&target)
            {
                omitted_parameters.insert(index);
                static_function_values.insert(parameter.name.clone(), target);
            }
        }
        if !static_function_values.is_empty() {
            rewrite_static_function_values(&mut body, &static_function_values);
        }
        if let Some(parameter_index) =
            parameters
                .iter()
                .flatten()
                .enumerate()
                .find_map(|(index, parameter)| {
                    if omitted_parameters.contains(&index) {
                        return None;
                    }
                    let Type::Function { effects, .. } = &parameter.ty else {
                        return None;
                    };
                    effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                        .then_some(index)
                })
        {
            let parameter = function
                .groups
                .iter()
                .flatten()
                .nth(parameter_index)
                .expect("hygienic and source parameter lists have identical shapes");
            self.error(format!(
                "dynamic effectful callable parameter `{}` requires the handler-aware runtime ABI",
                parameter.name
            ));
            return Some(Err(()));
        }
        if let Some(alias) = source_arguments
            .iter()
            .enumerate()
            .filter(|(index, _)| !omitted_parameters.contains(index))
            .find_map(|(_, argument)| {
                handler_alias_reference(&argument.value, &handler.function_aliases.borrow())
            })
        {
            self.error(format!(
                "effectful function alias `{alias}` cannot escape its handler or be used as a runtime value"
            ));
            return Some(Err(()));
        }
        let Some(input) = logical_function_result_source(&function) else {
            self.error(format!(
                "resumable function `{name}` requires an explicit return type"
            ));
            handler.inlining.borrow_mut().remove(&name);
            return Some(Err(()));
        };
        let Some(answer) = handler.result_source.clone() else {
            self.error("a resumable named call requires a contextual handler answer type");
            handler.inlining.borrow_mut().remove(&name);
            return Some(Err(()));
        };
        let continuation_name = format!("$handler$call$continuation${specialization}");
        let continuation_value_name = format!("$handler$call$continuation$value${specialization}");
        let continuation_body =
            match continuation(self, Expr::Name(continuation_value_name.clone())) {
                Ok(body) => body,
                Err(()) => return Some(Err(())),
            };
        handler.inlining.borrow_mut().insert(
            name.to_owned(),
            SourceInlineFrame {
                recursive_name,
                input: input.clone(),
                answer: answer.clone(),
            },
        );
        let continuation_binding = Binding {
            mutable: true,
            name: continuation_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![vec![input.clone()]],
                effects: FunctionEffects::default(),
                result: Box::new(answer.clone()),
            }),
            value: Expr::Closure(
                vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    passing: None,
                    region: None,
                    name: continuation_value_name,
                    ty: input.clone(),
                }],
                Box::new(continuation_body),
            ),
        };
        let erased_continuation_name =
            format!("$handler$erased$call$continuation${specialization}");
        let erased_continuation_binding = Binding {
            mutable: true,
            name: erased_continuation_name.clone(),
            annotation: Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![input.clone(), answer.clone()],
            )),
            value: Expr::Call(
                Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                vec![CallArg {
                    label: None,
                    value: Expr::Name(continuation_name.clone()),
                }],
            ),
        };
        let frame_continuation_name = format!("$handler$frame$continuation${specialization}");
        let tail_name = format!("$handler$tail${specialization}");
        let continuation_for_return = frame_continuation_name.clone();
        let tail_continuation: SourceContinuation = Rc::new(move |_, value| {
            Ok(Expr::Call(
                Box::new(Expr::Name(tail_name.clone())),
                vec![CallArg {
                    label: None,
                    value: Expr::Call(
                        Box::new(Expr::Name("$handler$invoke$continuation".to_owned())),
                        vec![
                            CallArg {
                                label: None,
                                value: Expr::Name(continuation_for_return.clone()),
                            },
                            CallArg { label: None, value },
                        ],
                    ),
                }],
            ))
        });
        let return_name = format!("$handler$return${specialization}");
        rewrite_handler_returns(&mut body, &return_name);
        handler
            .return_continuations
            .borrow_mut()
            .insert(return_name.clone(), tail_continuation.clone());
        let transformed_body = match self.transform_handler_expr(
            body,
            handler.clone(),
            resume.clone(),
            tail_continuation,
        ) {
            Ok(body) => body,
            Err(()) => {
                handler
                    .return_continuations
                    .borrow_mut()
                    .remove(&return_name);
                handler.inlining.borrow_mut().remove(&name);
                return Some(Err(()));
            }
        };
        handler
            .return_continuations
            .borrow_mut()
            .remove(&return_name);

        let mut flattened_parameters = parameters
            .iter()
            .flatten()
            .cloned()
            .enumerate()
            .filter_map(|(index, parameter)| {
                (!omitted_parameters.contains(&index)).then_some(parameter)
            })
            .collect::<Vec<_>>();
        flattened_parameters.push(Param {
            mode: PassMode::Move,
            access: None,
            passing: None,
            region: None,
            name: frame_continuation_name,
            ty: Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![input.clone(), answer.clone()],
            ),
        });
        let mut flattened_arguments = source_arguments
            .into_iter()
            .enumerate()
            .filter_map(|(index, argument)| {
                (!omitted_parameters.contains(&index)).then_some(argument)
            })
            .collect::<Vec<_>>();
        flattened_arguments.push(CallArg {
            label: None,
            value: Expr::Name(erased_continuation_name),
        });
        let frame_name = format!("$handler$frame${specialization}");
        self.handler_frame_parameter_modes.insert(
            frame_name.clone(),
            flattened_parameters
                .iter()
                .map(|parameter| parameter.mode)
                .collect(),
        );
        let mut frame_effects = function.effects.clone();
        frame_effects
            .custom
            .retain(|effect| source_effect_identity(effect) != handler.identity);
        if handler.lexical_unsafe_depth.get() > 0 {
            self.strip_authorized_unsafe_effects(&mut frame_effects);
        }
        let frame_result = self.effect_abi_result_source(answer, &frame_effects);
        let frame_annotation = Some(Type::Function {
            groups: vec![flattened_parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect()],
            effects: frame_effects,
            result: Box::new(frame_result),
        });
        let frame = Binding {
            mutable: true,
            name: frame_name.clone(),
            annotation: frame_annotation,
            value: Expr::Closure(flattened_parameters, Box::new(transformed_body)),
        };
        let call = Expr::Call(Box::new(Expr::Name(frame_name)), flattened_arguments);
        let result = Ok(Expr::Block(
            vec![
                Stmt::Let(continuation_binding),
                Stmt::Let(erased_continuation_binding),
                Stmt::Let(frame),
            ],
            Some(Box::new(call)),
        ));
        handler.inlining.borrow_mut().remove(&name);
        Some(result)
    }

    fn transform_handler_block(
        &mut self,
        mut statements: Vec<Stmt>,
        tail: Option<Expr>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if statements.is_empty() {
            return self.transform_handler_expr(
                tail.unwrap_or(Expr::Unit),
                handler,
                resume,
                continuation,
            );
        }
        let first = statements.remove(0);
        match first {
            Stmt::Let(mut binding) => {
                if let Some(Type::Function {
                    groups: callable_groups,
                    effects,
                    ..
                }) = binding.annotation.as_ref()
                {
                    if effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                        && matches!(binding.value, Expr::If { .. })
                    {
                        let mut targets = Vec::new();
                        if let Some(selection) =
                            static_callable_selection(&binding.value, &mut targets)
                        {
                            let group_lengths =
                                callable_groups.iter().map(Vec::len).collect::<Vec<_>>();
                            let mut union = Vec::new();
                            let mut sources = Vec::new();
                            let mut tag_bindings = Vec::new();
                            let mut valid = true;
                            for (index, target) in targets.into_iter().enumerate() {
                                if let Some(dynamic) =
                                    handler.dynamic_callables.borrow().get(&target).cloned()
                                {
                                    if dynamic.group_lengths != group_lengths {
                                        valid = false;
                                        break;
                                    }
                                    let hidden = format!(
                                        "$handler$dynamic$tag${}${index}",
                                        self.next_closure
                                    );
                                    tag_bindings.push(Stmt::Let(Binding {
                                        mutable: false,
                                        name: hidden.clone(),
                                        annotation: Some(Type::I32),
                                        value: Expr::Name(target),
                                    }));
                                    for candidate in &dynamic.targets {
                                        if !union.contains(candidate) {
                                            union.push(candidate.clone());
                                        }
                                    }
                                    sources.push((hidden, dynamic.targets));
                                    continue;
                                }
                                let resolved = handler
                                    .function_aliases
                                    .borrow()
                                    .get(&target)
                                    .cloned()
                                    .unwrap_or(target);
                                if !(self.functions.contains_key(&resolved)
                                    || handler.resumable_closures.borrow().contains_key(&resolved))
                                {
                                    valid = false;
                                    break;
                                }
                                if !union.contains(&resolved) {
                                    union.push(resolved.clone());
                                }
                                sources.push((String::new(), vec![resolved]));
                            }
                            if valid && union.len() >= 2 {
                                let selection =
                                    expand_dynamic_callable_selection(selection, &sources, &union);
                                let name = binding.name.clone();
                                let callable = SourceDynamicCallable {
                                    targets: union,
                                    group_lengths,
                                };
                                let next_handler = handler.clone();
                                let next_resume = resume.clone();
                                let next_continuation = continuation.clone();
                                let transformed = self.transform_handler_expr(
                                    selection,
                                    handler,
                                    resume,
                                    Rc::new(move |analyzer, selection| {
                                        let previous = next_handler
                                            .dynamic_callables
                                            .borrow_mut()
                                            .insert(name.clone(), callable.clone());
                                        let rest = analyzer.transform_handler_block(
                                            statements.clone(),
                                            tail.clone(),
                                            next_handler.clone(),
                                            next_resume.clone(),
                                            next_continuation.clone(),
                                        );
                                        let mut callables =
                                            next_handler.dynamic_callables.borrow_mut();
                                        if let Some(previous) = previous {
                                            callables.insert(name.clone(), previous);
                                        } else {
                                            callables.remove(&name);
                                        }
                                        drop(callables);
                                        let rest = rest?;
                                        Ok(Expr::Block(
                                            vec![Stmt::Let(Binding {
                                                mutable: false,
                                                name: name.clone(),
                                                annotation: Some(Type::I32),
                                                value: selection,
                                            })],
                                            Some(Box::new(rest)),
                                        ))
                                    }),
                                );
                                return transformed.map(|transformed| {
                                    Expr::Block(tag_bindings, Some(Box::new(transformed)))
                                });
                            }
                        }
                    }
                }
                if let Some(transformed) =
                    self.transform_resumable_closure_binding(&binding, handler.clone())
                {
                    let (binding, closure) = transformed?;
                    let name = binding.name.clone();
                    let previous = handler
                        .resumable_closures
                        .borrow_mut()
                        .insert(name.clone(), closure);
                    let rest = self.transform_handler_block(
                        statements,
                        tail,
                        handler.clone(),
                        resume,
                        continuation,
                    );
                    let mut closures = handler.resumable_closures.borrow_mut();
                    if let Some(previous) = previous {
                        closures.insert(name, previous);
                    } else {
                        closures.remove(&name);
                    }
                    drop(closures);
                    return rest
                        .map(|rest| Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(rest))));
                }
                if let Expr::Name(target) = &binding.value {
                    let dynamic = handler.dynamic_callables.borrow().get(target).cloned();
                    if let Some(dynamic) = dynamic {
                        let name = binding.name.clone();
                        binding.annotation = Some(Type::I32);
                        let previous = handler
                            .dynamic_callables
                            .borrow_mut()
                            .insert(name.clone(), dynamic);
                        let rest = self.transform_handler_block(
                            statements,
                            tail,
                            handler.clone(),
                            resume,
                            continuation,
                        );
                        let mut callables = handler.dynamic_callables.borrow_mut();
                        if let Some(previous) = previous {
                            callables.insert(name, previous);
                        } else {
                            callables.remove(&name);
                        }
                        drop(callables);
                        return rest.map(|rest| {
                            Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(rest)))
                        });
                    }
                }
                if let Expr::Name(target) = &binding.value {
                    let resolved_target = handler
                        .function_aliases
                        .borrow()
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| target.clone());
                    let aliases_handler_effect =
                        self.functions
                            .get(&resolved_target)
                            .is_some_and(|function| {
                                function.effects.custom.iter().any(|effect| {
                                    source_effect_identity(effect) == handler.identity
                                })
                            });
                    if aliases_handler_effect {
                        if binding.mutable || binding.annotation.is_some() {
                            self.error(format!(
                                "effectful function alias `{}` must be an inferred immutable binding",
                                binding.name
                            ));
                            return Err(());
                        }
                        let alias = binding.name.clone();
                        let previous = handler
                            .function_aliases
                            .borrow_mut()
                            .insert(alias.clone(), resolved_target);
                        let transformed = self.transform_handler_block(
                            statements,
                            tail,
                            handler.clone(),
                            resume,
                            continuation,
                        );
                        let mut aliases = handler.function_aliases.borrow_mut();
                        if let Some(previous) = previous {
                            aliases.insert(alias, previous);
                        } else {
                            aliases.remove(&alias);
                        }
                        return transformed;
                    }
                }
                if binding.name.starts_with("$handler$frame$")
                    || binding.name.starts_with("$handler$continuation$")
                    || binding.name.starts_with("$handler$call$continuation$")
                {
                    if let Expr::Closure(parameters, body) = binding.value {
                        let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                        let transformed = self.transform_handler_expr(
                            *body,
                            handler.clone(),
                            resume.clone(),
                            identity,
                        )?;
                        binding.value = Expr::Closure(parameters, Box::new(transformed));
                    }
                }
                let name = binding.name.clone();
                let annotation = binding.annotation.clone();
                let mutable = binding.mutable;
                let next_handler = handler.clone();
                let next_resume = resume.clone();
                self.transform_handler_expr(
                    binding.value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        let rest = analyzer.transform_handler_block(
                            statements.clone(),
                            tail.clone(),
                            next_handler.clone(),
                            next_resume.clone(),
                            continuation.clone(),
                        )?;
                        Ok(Expr::Block(
                            vec![Stmt::Let(Binding {
                                mutable,
                                name: name.clone(),
                                annotation: annotation.clone(),
                                value,
                            })],
                            Some(Box::new(rest)),
                        ))
                    }),
                )
            }
            Stmt::Expr(statement) => {
                let next_handler = handler.clone();
                let next_resume = resume.clone();
                self.transform_handler_expr(
                    statement,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        let rest = analyzer.transform_handler_block(
                            statements.clone(),
                            tail.clone(),
                            next_handler.clone(),
                            next_resume.clone(),
                            continuation.clone(),
                        )?;
                        Ok(Expr::Block(vec![Stmt::Expr(value)], Some(Box::new(rest))))
                    }),
                )
            }
        }
    }

    fn lower_effect_operation_call(
        &mut self,
        definition: &EffectDef,
        instance: &Type,
        operation: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let candidates = definition
            .operations
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.name == operation)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            self.error(format!(
                "unknown operation `{operation}` on effect `{}`",
                definition.name
            ));
            return error_expr();
        }
        let arguments = groups
            .iter()
            .flat_map(|group| group.iter())
            .cloned()
            .collect::<Vec<_>>();
        let labels = call_argument_labels(&arguments);
        if labels.is_none() && arguments.iter().any(|argument| argument.label.is_some()) {
            self.error(format!(
                "cannot mix named and positional arguments in effect operation `{operation}`"
            ));
            return error_expr();
        }
        let selected = if candidates.len() == 1 {
            Some(candidates[0])
        } else if let Some(labels) = labels {
            candidates
                .iter()
                .copied()
                .find(|(_, candidate)| effect_operation_labels(candidate) == labels)
        } else {
            self.error(format!(
                "overloaded effect operation `{operation}` requires named arguments"
            ));
            return error_expr();
        };
        let Some((operation_index, selected)) = selected else {
            self.error(format!(
                "no effect operation `{operation}` matches the supplied argument names"
            ));
            return error_expr();
        };
        let mut function = selected.clone();
        let Type::Named(_, arguments) = instance else {
            unreachable!("resolved effect instances are nominal applications")
        };
        let parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .collect::<Vec<_>>();
        let substitutions = parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        substitute_function_types(&mut function, &substitutions);
        function.compile_groups.clear();
        if !function.effects.custom.contains(instance) {
            function.effects.custom.push(instance.clone());
        }
        let identity = source_effect_identity(instance);
        let canonical = format!("$effect$operation${identity}${operation}${operation_index}");
        if !self.functions.contains_key(&canonical) {
            function.name = canonical.clone();
            let signature = FunctionSig {
                groups: function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| ParamSig {
                                name: parameter.name.clone(),
                                ty: self.lower_source_type(&parameter.ty),
                                mode: parameter.mode,
                            })
                            .collect()
                    })
                    .collect(),
                unsafe_effect: self.function_effects_unsafe(&function.effects),
                throws_error: function
                    .effects
                    .throws
                    .as_deref()
                    .map(|error| self.lower_source_type(error)),
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result: function
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result)),
            };
            self.functions.insert(canonical.clone(), function);
            self.signatures.insert(canonical.clone(), signature);
        }
        self.lower_named_function_call(&canonical, groups, expected, context)
    }

    fn lower_nominal_type_member_call(
        &mut self,
        target: &str,
        kind: NominalKind,
        member: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let explicitly_qualified_method = groups.first().is_some_and(
            |group| matches!(*group, [CallArg { label: Some(label), .. }] if label == "self"),
        );
        if !explicitly_qualified_method {
            let overload_key = (target.to_owned(), member.to_owned(), false);
            if self.inherent_overloads.contains_key(&overload_key) {
                let Some(canonical) = self.resolve_inherent_overload(target, member, false, groups)
                else {
                    return error_expr();
                };
                if self.function_templates.contains_key(&canonical) {
                    return self.lower_generic_function_call(&canonical, groups, expected, context);
                }
                return self.lower_named_function_call(&canonical, groups, expected, context);
            }
            if let Some(canonical) = self
                .inherent_members
                .get(target)
                .and_then(|members| members.functions.get(member))
                .cloned()
            {
                return self.lower_named_function_call(&canonical, groups, expected, context);
            }
            let self_ty = match kind {
                NominalKind::Struct => Ty::Struct(target.to_owned()),
                NominalKind::Enum => Ty::Enum(target.to_owned()),
            };
            let associated =
                self.trait_associated_function_candidates(&self_ty, member, &context.origin);
            match associated.as_slice() {
                [canonical] => {
                    if self.function_templates.contains_key(canonical) {
                        return self
                            .lower_generic_function_call(canonical, groups, expected, context);
                    }
                    return self.lower_named_function_call(canonical, groups, expected, context);
                }
                [_, _, ..] => {
                    if !groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .any(|argument| argument.label.is_some())
                    {
                        self.error(format!(
                            "ambiguous trait associated function `{target}.{member}`; named arguments are required to select an overload"
                        ));
                        return error_expr();
                    }
                    let matches = self.matching_function_overloads(&associated, groups, 0);
                    match matches.as_slice() {
                        [canonical] => {
                            if self.function_templates.contains_key(canonical) {
                                return self.lower_generic_function_call(
                                    canonical, groups, expected, context,
                                );
                            }
                            return self.lower_named_function_call(
                                canonical, groups, expected, context,
                            );
                        }
                        [] => self.error(format!(
                            "no trait associated function overload `{target}.{member}` matches the supplied named parameter groups"
                        )),
                        _ => self.error(format!(
                            "trait associated function overload `{target}.{member}` remains ambiguous"
                        )),
                    }
                    return error_expr();
                }
                [] => {}
            }
            if self
                .inherent_members
                .get(target)
                .is_some_and(|members| members.constants.contains_key(member))
            {
                self.error(format!(
                    "associated constant `{target}.{member}` is not callable"
                ));
                return error_expr();
            }
            if kind == NominalKind::Enum {
                let Some(layout) = self.enum_layout_or_diagnostic(target) else {
                    return error_expr();
                };
                if let Some(variant) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == member)
                {
                    return self.lower_enum_constructor(target, variant, groups, context);
                }
            }
        }
        let self_ty = match kind {
            NominalKind::Struct => Ty::Struct(target.to_owned()),
            NominalKind::Enum => Ty::Enum(target.to_owned()),
        };
        let has_inherent_method = self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.methods.contains_key(member));
        let has_trait_method = !self
            .trait_method_candidates(&self_ty, member, &context.origin)
            .is_empty()
            || self.has_inaccessible_trait_method(&self_ty, member, &context.origin);
        if has_inherent_method || has_trait_method {
            let Some((receiver_group, remaining_groups)) = groups.split_first() else {
                self.error(format!(
                    "qualified method `{target}.{member}` requires a receiver argument group"
                ));
                return error_expr();
            };
            let [receiver] = *receiver_group else {
                self.error(format!(
                    "receiver group of qualified method `{target}.{member}` expects exactly one argument"
                ));
                return error_expr();
            };
            if receiver
                .label
                .as_deref()
                .is_some_and(|label| label != "self")
            {
                self.error(format!(
                    "receiver argument of qualified method `{target}.{member}` must be unlabeled or named `self`"
                ));
                return error_expr();
            }
            self.lower_bound_method_call(
                &receiver.value,
                member,
                remaining_groups,
                BoundMethodConstraint::Nominal(target),
                expected,
                context,
            )
        } else if kind == NominalKind::Enum {
            self.error(format!(
                "unknown associated member or variant `{member}` on `{target}`"
            ));
            error_expr()
        } else {
            self.error(format!(
                "unknown associated member `{member}` on `{target}`"
            ));
            error_expr()
        }
    }

    fn lower_constructor_trait_associated_function_call(
        &mut self,
        target: &str,
        member: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> Option<HirExpr> {
        let candidates =
            self.constructor_trait_associated_function_candidates(target, member, &context.origin);
        let canonical = match candidates.as_slice() {
            [canonical] => canonical.clone(),
            [_, _, ..] => {
                if !groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    self.error(format!(
                        "ambiguous constructor trait associated function `{target}.{member}`; named arguments are required to select an overload"
                    ));
                    return Some(error_expr());
                }
                let matches = self.matching_function_overloads(&candidates, groups, 0);
                match matches.as_slice() {
                    [canonical] => canonical.clone(),
                    [] => {
                        self.error(format!(
                            "no constructor trait associated function overload `{target}.{member}` matches the supplied named parameter groups"
                        ));
                        return Some(error_expr());
                    }
                    _ => {
                        self.error(format!(
                            "constructor trait associated function overload `{target}.{member}` remains ambiguous"
                        ));
                        return Some(error_expr());
                    }
                }
            }
            [] => {
                if self.has_inaccessible_constructor_trait_associated_function(
                    target,
                    member,
                    &context.origin,
                ) {
                    self.error(format!(
                        "constructor trait associated function `{target}.{member}` is private or package-visible from another package"
                    ));
                    return Some(error_expr());
                }
                return None;
            }
        };
        if self.function_templates.contains_key(&canonical) {
            Some(self.lower_generic_function_call(&canonical, groups, expected, context))
        } else {
            Some(self.lower_named_function_call(&canonical, groups, expected, context))
        }
    }

    fn lower_generic_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some((canonical, runtime_start)) =
            self.resolve_inferred_generic_function_instance(name, groups, expected, context)
        else {
            return error_expr();
        };
        self.lower_named_function_call(&canonical, &groups[runtime_start..], expected, context)
    }

    fn inferred_generic_enum_type_head<'a>(
        &self,
        expression: &'a Expr,
        context: &LowerCtx,
    ) -> Option<(String, Vec<&'a [CallArg]>)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            return None;
        };
        if context.shadows_top_level_name(name) || !self.enum_templates.contains_key(name) {
            return None;
        }
        let compile_group_count = self.enum_templates[name].compile_groups.len();
        if groups.len() > compile_group_count {
            return None;
        }
        Some((name.clone(), groups))
    }

    fn resolve_inferred_generic_enum_instance(
        &mut self,
        name: &str,
        type_groups: &[&[CallArg]],
        variant_name: &str,
        value_groups: &[&[CallArg]],
        hints: InferredEnumHints<'_>,
        context: &LowerCtx,
    ) -> Option<String> {
        let template = self.enum_templates[name].clone();
        let (compile_parameters, mut inferred, consumed_groups) = self
            .seed_type_argument_inference(
                name,
                &template.compile_groups,
                type_groups,
                context,
                true,
            )?;
        if consumed_groups != type_groups.len() {
            self.error(format!("invalid type argument group in `{name}`"));
            return None;
        }
        if let Some(payload_hint) = hints.payload.filter(|hint| hint.ty != Ty::Error) {
            let kind = if name == self.lang_item_name(LangItemKind::Option) {
                StandardFallibleKind::Option
            } else if name == self.lang_item_name(LangItemKind::Result) {
                StandardFallibleKind::Result
            } else {
                self.error(format!(
                    "internal error: coalescing enum `{name}` is not a standard fallible type"
                ));
                return None;
            };
            let Some(payload_parameter) = self.standard_fallible_payload_parameter(kind, name)
            else {
                self.error(format!(
                    "internal error: coalescing enum `{name}` has no payload type parameter"
                ));
                return None;
            };
            let payload_template = Type::Named(payload_parameter.name.clone(), Vec::new());
            if let Err(message) = self.unify_template_ty(
                &payload_template,
                &payload_hint.ty,
                payload_hint.source.as_ref(),
                &compile_parameters,
                &mut inferred,
                "payload type of `??`",
            ) {
                self.error(message);
                return None;
            }
        }
        if let Some(expected) = hints.result.filter(|ty| **ty != Ty::Error) {
            let result_template = Type::Named(
                name.to_owned(),
                template
                    .compile_groups
                    .iter()
                    .flatten()
                    .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
                    .collect(),
            );
            if let Err(message) = self.unify_template_ty(
                &result_template,
                expected,
                None,
                &compile_parameters,
                &mut inferred,
                "expected result type",
            ) {
                self.error(message);
                return None;
            }
        }

        let Some(variant) = template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .cloned()
        else {
            self.error(format!(
                "unknown associated member or variant `{variant_name}` on `{name}`"
            ));
            return None;
        };
        if !self.require_source_variant_fields_access(
            name,
            variant_name,
            &variant.fields,
            &context.origin,
        ) {
            return None;
        }
        let (fields, named) = match variant.fields {
            VariantFields::Unit => (Vec::new(), false),
            VariantFields::Positional(types) => (
                types
                    .into_iter()
                    .enumerate()
                    .map(|(index, ty)| (index.to_string(), ty))
                    .collect::<Vec<_>>(),
                false,
            ),
            VariantFields::Named(fields) => (
                fields
                    .into_iter()
                    .map(|field| (field.name, field.ty))
                    .collect::<Vec<_>>(),
                true,
            ),
        };
        if fields.is_empty() {
            if !value_groups.is_empty() {
                self.error(format!(
                    "unit variant `{name}.{variant_name}` is a value and must not be called"
                ));
                return None;
            }
        } else if value_groups.len() != 1 {
            self.error(format!(
                "enum variant constructor `{name}` expects exactly one argument group"
            ));
            return None;
        }

        let mut constraints = Vec::new();
        if let Some(arguments) = value_groups.first().copied() {
            let labeled = arguments
                .iter()
                .filter(|argument| argument.label.is_some())
                .count();
            if labeled != 0 && labeled != arguments.len() {
                self.error(format!(
                    "cannot mix labeled and positional arguments in variant `{name}.{variant_name}`"
                ));
                return None;
            }
            if labeled == 0 {
                if arguments.len() != fields.len() {
                    self.error(format!(
                        "argument count mismatch for variant `{name}.{variant_name}`: expected {}, found {}",
                        fields.len(),
                        arguments.len()
                    ));
                    return None;
                }
                for (argument, (field_name, field_ty)) in arguments.iter().zip(&fields) {
                    constraints.push((
                        field_ty.clone(),
                        argument.value.clone(),
                        format!("argument for variant field `{field_name}`"),
                    ));
                }
            } else {
                if !named {
                    self.error(format!(
                        "variant `{name}.{variant_name}` does not accept labeled arguments"
                    ));
                    return None;
                }
                let mut initialized = HashSet::new();
                for argument in arguments {
                    let label = argument
                        .label
                        .as_deref()
                        .expect("all arguments are labeled");
                    let Some((index, (_, field_ty))) = fields
                        .iter()
                        .enumerate()
                        .find(|(_, (field_name, _))| field_name == label)
                    else {
                        self.error(format!(
                            "unknown field `{label}` in variant `{name}.{variant_name}`"
                        ));
                        return None;
                    };
                    if !initialized.insert(index) {
                        self.error(format!(
                            "duplicate field `{label}` in variant `{name}.{variant_name}`"
                        ));
                        return None;
                    }
                    constraints.push((
                        field_ty.clone(),
                        argument.value.clone(),
                        format!("argument for variant field `{label}`"),
                    ));
                }
                if initialized.len() != fields.len() {
                    let missing = fields
                        .iter()
                        .enumerate()
                        .find(|(index, _)| !initialized.contains(index))
                        .map(|(_, (name, _))| name.as_str())
                        .unwrap_or("<unknown>");
                    self.error(format!(
                        "missing field `{missing}` in variant `{name}.{variant_name}`"
                    ));
                    return None;
                }
            }
        }

        let unsupported = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) =
            self.finish_type_argument_inference(name, &ordered_parameters, &inferred, unsupported)?;
        self.ensure_nominal_instance(NominalKind::Enum, name, source_arguments, arguments)
    }

    fn resolve_inferred_generic_struct_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Option<(String, usize)> {
        let template = self.struct_templates[name].clone();
        if !self.require_source_fields_access(name, &template.fields, &context.origin) {
            return None;
        }
        let (compile_parameters, mut inferred, runtime_start) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            groups,
            context,
            true,
        )?;
        let value_groups = &groups[runtime_start..];
        if value_groups.len() != 1 {
            self.error(format!(
                "struct constructor `{name}` expects exactly one argument group"
            ));
            return None;
        }

        if let Some(expected) = expected.filter(|ty| **ty != Ty::Error) {
            let result_template = Type::Named(
                name.to_owned(),
                template
                    .compile_groups
                    .iter()
                    .flatten()
                    .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
                    .collect(),
            );
            if let Err(message) = self.unify_template_ty(
                &result_template,
                expected,
                None,
                &compile_parameters,
                &mut inferred,
                "expected result type",
            ) {
                self.error(message);
                return None;
            }
        }

        let arguments = value_groups[0];
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            self.error(format!(
                "cannot mix labeled and positional arguments in struct `{name}`"
            ));
            return None;
        }
        let mut constraints = Vec::new();
        if labeled == 0 {
            if arguments.len() != template.fields.len() {
                self.error(format!(
                    "argument count mismatch for struct `{name}`: expected {}, found {}",
                    template.fields.len(),
                    arguments.len()
                ));
                return None;
            }
            for (argument, field) in arguments.iter().zip(&template.fields) {
                constraints.push((
                    field.ty.clone(),
                    argument.value.clone(),
                    format!("argument for field `{}`", field.name),
                ));
            }
        } else {
            let mut initialized = HashSet::new();
            for argument in arguments {
                let label = argument
                    .label
                    .as_deref()
                    .expect("all arguments are labeled");
                let Some((index, field)) = template
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == label)
                else {
                    self.error(format!("unknown field `{label}` in struct `{name}`"));
                    return None;
                };
                if !initialized.insert(index) {
                    self.error(format!("duplicate field `{label}` in struct `{name}`"));
                    return None;
                }
                constraints.push((
                    field.ty.clone(),
                    argument.value.clone(),
                    format!("argument for field `{label}`"),
                ));
            }
            if initialized.len() != template.fields.len() {
                let missing = template
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(index, _)| !initialized.contains(index))
                    .map(|(_, field)| field.name.as_str())
                    .unwrap_or("<unknown>");
                self.error(format!("missing field `{missing}` in struct `{name}`"));
                return None;
            }
        }
        let unsupported = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) =
            self.finish_type_argument_inference(name, &ordered_parameters, &inferred, unsupported)?;
        let canonical =
            self.ensure_nominal_instance(NominalKind::Struct, name, source_arguments, arguments)?;
        Some((canonical, runtime_start))
    }

    fn resolve_generic_nominal_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<(String, usize, NominalKind)> {
        let (kind, compile_groups) = if let Some(template) = self.struct_templates.get(name) {
            (NominalKind::Struct, template.compile_groups.clone())
        } else if let Some(template) = self.enum_templates.get(name) {
            (NominalKind::Enum, template.compile_groups.clone())
        } else {
            self.error(format!("unknown generic nominal type `{name}`"));
            return None;
        };
        let (compile_parameters, inferred, consumed_groups) =
            self.seed_type_argument_inference(name, &compile_groups, groups, context, true)?;
        if consumed_groups != groups.len() {
            self.error(format!("invalid type argument group in `{name}`"));
            return None;
        }
        let ordered_parameters = compile_groups.into_iter().flatten().collect::<Vec<_>>();
        let (source_arguments, arguments) =
            self.finish_type_argument_inference(name, &ordered_parameters, &inferred, false)?;
        debug_assert!(compile_parameters
            .iter()
            .all(|parameter| inferred.contains_key(parameter)));
        let canonical = self.ensure_nominal_instance(kind, name, source_arguments, arguments)?;
        Some((canonical, consumed_groups, kind))
    }

    fn resolve_nominal_type_head(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Result<Option<(String, NominalKind)>, ()> {
        match expression {
            Expr::Name(name) if context.lookup(name).is_some() => Ok(None),
            Expr::Name(name) if context.has_type_parameter(name) => {
                self.error(format!(
                    "type parameter `{name}` has no statically known associated members"
                ));
                Err(())
            }
            Expr::Name(name) if !context.shadows_top_level_name(name) => {
                if self.struct_layouts.contains_key(name) {
                    Ok(Some((name.clone(), NominalKind::Struct)))
                } else if self.enum_layouts.contains_key(name) {
                    Ok(Some((name.clone(), NominalKind::Enum)))
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` requires explicit type arguments"
                    ));
                    Err(())
                } else {
                    Ok(None)
                }
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return Ok(None);
                };
                if context.lookup(name).is_some() {
                    return Ok(None);
                }
                if context.has_type_parameter(name) {
                    self.error(format!(
                        "type parameter `{name}` cannot be used as a generic type constructor"
                    ));
                    return Err(());
                }
                if !self.struct_templates.contains_key(name)
                    && !self.enum_templates.contains_key(name)
                {
                    return Ok(None);
                }
                let expected_groups = if let Some(template) = self.struct_templates.get(name) {
                    template.compile_groups.clone()
                } else {
                    self.enum_templates[name].compile_groups.clone()
                };
                if groups.len() > expected_groups.len() {
                    return Ok(None);
                }
                let mut next_compile_group = 0;
                for arguments in &groups {
                    let labeled = arguments
                        .first()
                        .is_some_and(|argument| argument.label.is_some());
                    let target = if labeled {
                        (next_compile_group..expected_groups.len()).find(|index| {
                            arguments.iter().all(|argument| {
                                argument.label.as_ref().is_some_and(|label| {
                                    expected_groups[*index]
                                        .iter()
                                        .any(|parameter| parameter.name == *label)
                                })
                            })
                        })
                    } else if next_compile_group < expected_groups.len()
                        && !arguments.is_empty()
                        && self.group_is_explicit_compile_application(
                            &expected_groups[next_compile_group],
                            arguments,
                            context,
                            true,
                        )
                    {
                        Some(next_compile_group)
                    } else {
                        None
                    };
                    let Some(target) = target else {
                        return Ok(None);
                    };
                    next_compile_group = target + 1;
                }
                let Some((canonical, consumed, kind)) =
                    self.resolve_generic_nominal_instance(name, &groups, context)
                else {
                    return Err(());
                };
                if consumed != groups.len() {
                    self.error(format!(
                        "generic type head `{name}` is missing type argument groups"
                    ));
                    return Err(());
                }
                Ok(Some((canonical, kind)))
            }
            _ => Ok(None),
        }
    }

    fn resolve_inferred_generic_function_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Option<(String, usize)> {
        let template = self
            .function_templates
            .get(name)
            .unwrap_or_else(|| panic!("missing generic function template `{name}`"))
            .clone();
        let (compile_parameters, mut inferred, runtime_start) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            groups,
            context,
            false,
        )?;
        let runtime_groups = &groups[runtime_start..];
        if runtime_groups.len() > template.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected at most {}, found {}",
                template.groups.len(),
                runtime_groups.len()
            ));
            return None;
        }
        let mut ordered_runtime_groups = Vec::new();
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&template.groups).enumerate()
        {
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            ordered_runtime_groups.push(self.ordered_call_arguments(
                name,
                group_index + 1,
                arguments,
                &parameter_names,
            )?);
        }

        if runtime_groups.len() == template.groups.len() {
            if let (Some(expected), Some(result)) = (expected, template.return_type.as_ref()) {
                if *expected != Ty::Error {
                    let logical_result = if template.effects.throws.is_some() {
                        match result {
                            Type::Named(_, arguments) if arguments.len() == 2 => &arguments[0],
                            _ => result,
                        }
                    } else {
                        result
                    };
                    if !source_type_is_never(logical_result) {
                        if let Err(message) = self.unify_template_ty(
                            logical_result,
                            expected,
                            None,
                            &compile_parameters,
                            &mut inferred,
                            "expected result type",
                        ) {
                            self.error(message);
                            return None;
                        }
                    }
                }
            }
        }

        let constraints: Vec<_> = ordered_runtime_groups
            .iter()
            .zip(&template.groups)
            .enumerate()
            .flat_map(|(group_index, (arguments, parameters))| {
                arguments
                    .iter()
                    .zip(parameters)
                    .map(move |(argument, parameter)| {
                        (
                            parameter.ty.clone(),
                            argument.value.clone(),
                            format!(
                                "argument for parameter `{}` in group {}",
                                parameter.name,
                                group_index + 1
                            ),
                        )
                    })
            })
            .collect();
        let unsupported_argument = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;

        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) = self.finish_type_argument_inference(
            name,
            &ordered_parameters,
            &inferred,
            unsupported_argument,
        )?;
        let canonical = self.ensure_function_instance(name, source_arguments, arguments)?;
        Some((canonical, runtime_start))
    }

    fn lower_bound_method_call(
        &mut self,
        receiver: &Expr,
        member: &str,
        groups: &[&[CallArg]],
        constraint: BoundMethodConstraint<'_>,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let qualified_target = match constraint {
            BoundMethodConstraint::Nominal(target) => Some(target),
            BoundMethodConstraint::None | BoundMethodConstraint::LangItem(_) => None,
        };
        let forced_trait = match constraint {
            BoundMethodConstraint::LangItem(kind) => Some(kind),
            BoundMethodConstraint::None | BoundMethodConstraint::Nominal(_) => None,
        };
        let (mut receiver_place, mut temporary_binding) =
            if let Some(place) = self.lower_place_without_diagnostic(receiver, context) {
                (place, None)
            } else {
                let value = self.lower_expr(receiver, None, context);
                let id = context.fresh_local();
                let ty = value.ty.clone();
                (
                    HirPlace {
                        local: id,
                        root_ty: ty.clone(),
                        projections: Vec::new(),
                        ty: ty.clone(),
                        capability: LocalCapability::Owned,
                        root_mutable: false,
                        loan: None,
                        indirect: false,
                    },
                    Some(HirBinding {
                        id,
                        name: format!("$temporary receiver for {member}"),
                        ty,
                        mutable: false,
                        value,
                    }),
                )
            };
        let target = match &receiver_place.ty {
            Ty::Struct(name) | Ty::Enum(name) => name.clone(),
            ty => {
                self.error(format!(
                    "method call requires a nominal receiver, found `{ty}`"
                ));
                return error_expr();
            }
        };
        if let Some(qualified_target) = qualified_target {
            if qualified_target != target {
                self.error(format!(
                    "qualified method `{qualified_target}.{member}` requires receiver `{qualified_target}`, found `{target}`"
                ));
                return error_expr();
            }
        }
        let overload_key = (target.clone(), member.to_owned(), true);
        let inherent = if forced_trait.is_some() {
            None
        } else if self.inherent_overloads.contains_key(&overload_key) {
            self.resolve_inherent_overload(&target, member, true, groups)
        } else {
            self.inherent_members
                .get(&target)
                .and_then(|members| members.methods.get(member))
                .cloned()
        };
        if forced_trait.is_none()
            && self.inherent_overloads.contains_key(&overload_key)
            && inherent.is_none()
        {
            return error_expr();
        }
        let mut canonical = if let Some(canonical) = inherent {
            canonical
        } else {
            let mut candidates =
                self.trait_method_function_candidates(&receiver_place.ty, member, &context.origin);
            let mut constructor_candidates = self.constructor_trait_method_function_candidates(
                &receiver_place.ty,
                member,
                &context.origin,
            );
            if let Some(kind) = forced_trait {
                let trait_name = self.lang_item_name(kind);
                candidates.retain(|(key, _)| key.trait_ref.name == trait_name);
                constructor_candidates.retain(|(key, _)| key.trait_ref.name == trait_name);
                if kind.assignment_operator_method().is_some() {
                    if let Some(argument) = match groups {
                        [group] if group.len() == 1 => Some(&group[0]),
                        _ => None,
                    } {
                        let rhs = match self.probe_expr_ty(&argument.value, None, context) {
                            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => Some(ty),
                            TypeProbe::Defaultable(ty) => Some(ty),
                            TypeProbe::Unsupported => None,
                        };
                        if let Some(rhs) = rhs {
                            candidates.retain(|(key, _)| {
                                key.trait_ref.arguments.as_slice() == [rhs.clone()]
                            });
                        }
                    }
                }
            }
            let total_candidates = candidates.len() + constructor_candidates.len();
            if total_candidates == 1 {
                if candidates
                    .first()
                    .is_some_and(|(key, _)| self.is_drop_impl(key))
                {
                    self.error("`Drop.drop` cannot be called directly; destruction is automatic");
                    return error_expr();
                }
                if let Some((key, canonical)) = candidates.first() {
                    let implementation = &self.trait_impls[key];
                    debug_assert_eq!(implementation.key, *key);
                    debug_assert!(implementation
                        .associated_types
                        .values()
                        .all(|ty| *ty != Ty::Error));
                    canonical.clone()
                } else {
                    constructor_candidates[0].1.clone()
                }
            } else if total_candidates > 1 {
                let canonicals = candidates
                    .iter()
                    .map(|(_, canonical)| canonical.clone())
                    .chain(
                        constructor_candidates
                            .iter()
                            .map(|(_, canonical)| canonical.clone()),
                    )
                    .collect::<Vec<_>>();
                if !groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    self.error(format!(
                        "ambiguous trait method `{member}` on `{target}` requires named arguments to select an overload"
                    ));
                    return error_expr();
                }
                let matches = self.matching_function_overloads(&canonicals, groups, 1);
                match matches.as_slice() {
                    [selected] => selected.clone(),
                    [] => {
                        self.error(format!(
                            "no trait method overload `{member}` on `{target}` matches the supplied named parameter groups"
                        ));
                        return error_expr();
                    }
                    _ => {
                        self.error(format!(
                            "trait method overload `{member}` on `{target}` remains ambiguous"
                        ));
                        return error_expr();
                    }
                }
            } else {
                if let Some(kind) = forced_trait {
                    let requirement = match kind {
                        LangItemKind::Iterator | LangItemKind::IntoIterator => "`for`",
                        LangItemKind::Coalesce => "operator `??`",
                        LangItemKind::Chain => "operator `?.`",
                        _ => "language syntax",
                    };
                    self.error(format!(
                        "type `{}` does not implement `{}` required by {requirement}",
                        self.diagnostic_type_name(&receiver_place.ty),
                        kind.source_name(),
                    ));
                    return error_expr();
                }
                if self
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target}.{member}` must be called on the type"
                    ));
                } else if self
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.constants.contains_key(member))
                {
                    self.error(format!(
                        "associated constant `{target}.{member}` must be accessed on the type"
                    ));
                } else if self.has_inaccessible_trait_method(
                    &receiver_place.ty,
                    member,
                    &context.origin,
                ) {
                    self.error(format!(
                        "trait method `{member}` on `{target}` is private or package-visible from another package"
                    ));
                } else {
                    self.error(format!("unknown method `{member}` on `{target}`"));
                }
                return error_expr();
            }
        };

        let mut runtime_groups = groups;
        if let Some(template) = self.function_templates.get(&canonical).cloned() {
            let compile_prefix =
                self.explicit_compile_group_prefix(&template.compile_groups, groups, context);
            let receiver_group = [CallArg {
                label: None,
                value: receiver.clone(),
            }];
            let mut full_groups = Vec::with_capacity(groups.len() + 1);
            full_groups.extend_from_slice(&groups[..compile_prefix]);
            full_groups.push(receiver_group.as_slice());
            full_groups.extend_from_slice(&groups[compile_prefix..]);
            let Some((instance, runtime_start)) = self.resolve_inferred_generic_function_instance(
                &canonical,
                &full_groups,
                expected,
                context,
            ) else {
                return error_expr();
            };
            debug_assert_eq!(runtime_start, compile_prefix);
            canonical = instance;
            runtime_groups = &groups[compile_prefix..];
        }

        let function_ty = self.function_type(&canonical);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[&canonical].clone();
        let Some(receiver_parameter) = signature.groups.first().and_then(|group| group.first())
        else {
            self.error(format!(
                "internal error: method `{target}.{member}` has no receiver parameter"
            ));
            return error_expr();
        };
        let consumed_groups = runtime_groups.len() + 1;
        if consumed_groups > signature.groups.len() {
            self.error(format!(
                "too many parameter groups in method call `{target}.{member}`: expected {}, found {}",
                signature.groups.len() - 1,
                runtime_groups.len()
            ));
            return error_expr();
        }

        let mut temporary_loans = Vec::new();
        let mut argument_temporary_bindings = Vec::new();
        let receiver_argument = if temporary_binding.is_none() {
            self.lower_call_argument(
                receiver,
                receiver_parameter,
                context,
                &mut temporary_loans,
                &mut argument_temporary_bindings,
            )
        } else {
            self.require_same_type(
                &receiver_place.ty,
                &receiver_parameter.ty,
                format!("receiver for method `{target}.{member}`"),
            );
            let receiver_mode =
                self.effective_pass_mode(receiver_parameter.mode, &receiver_parameter.ty);
            if receiver_mode == PassMode::MutBorrow {
                receiver_place.root_mutable = true;
                if let Some(binding) = temporary_binding.as_mut() {
                    binding.mutable = true;
                }
            }
            match receiver_mode {
                PassMode::Copy => {
                    if !self.is_copy_type(&receiver_parameter.ty) {
                        let ty = self.diagnostic_type_name(&receiver_parameter.ty);
                        self.error(format!(
                            "receiver for method `{target}.{member}` requires Copy, but `{ty}` does not implement Copy"
                        ));
                    }
                    HirArgument::Copy(self.access_place(
                        receiver_place.clone(),
                        AccessKind::Copy,
                        context,
                    ))
                }
                PassMode::Move => HirArgument::Move(self.access_place(
                    receiver_place.clone(),
                    AccessKind::Move,
                    context,
                )),
                PassMode::Borrow => {
                    if let Some(loan) =
                        self.acquire_loan(&receiver_place, LoanKind::Shared, false, context)
                    {
                        receiver_place.loan = Some(loan);
                        temporary_loans.push(loan);
                    }
                    HirArgument::SharedBorrow(receiver_place.clone())
                }
                PassMode::MutBorrow => {
                    if let Some(loan) =
                        self.acquire_loan(&receiver_place, LoanKind::Mutable, false, context)
                    {
                        receiver_place.loan = Some(loan);
                        temporary_loans.push(loan);
                    }
                    HirArgument::MutBorrow(receiver_place.clone())
                }
                PassMode::Inferred => unreachable!("effective mode is explicit"),
            }
        };
        let mut arguments = vec![receiver_argument];
        for (relative_group, arguments_ast) in runtime_groups.iter().enumerate() {
            let group_index = relative_group + 1;
            let params = &signature.groups[group_index];
            let parameter_names = params
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let owner = format!("{target}.{member}");
            let Some(ordered) = self.ordered_call_arguments(
                &owner,
                relative_group + 1,
                arguments_ast,
                &parameter_names,
            ) else {
                return error_expr();
            };
            for (argument, parameter) in ordered.into_iter().zip(params) {
                arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                    &mut argument_temporary_bindings,
                ));
            }
        }

        let complete = consumed_groups == signature.groups.len();
        if complete {
            self.require_function_effects(&canonical, context);
        }
        if !complete
            && arguments.iter().any(|argument| {
                matches!(
                    argument,
                    HirArgument::SharedBorrow(_) | HirArgument::MutBorrow(_)
                ) || matches!(argument, HirArgument::Copy(value) | HirArgument::Move(value) if matches!(value.ty, Ty::Reference { .. }))
            })
        {
            self.error(format!(
                "partial application of bound method `{target}.{member}` cannot capture borrowed arguments"
            ));
        }
        let mut temporary_bindings = temporary_binding.into_iter().collect::<Vec<_>>();
        temporary_bindings.extend(argument_temporary_bindings);
        if complete {
            self.promote_returned_reference_loans(
                &canonical,
                &function_ty.result,
                &arguments,
                &temporary_bindings,
                &mut temporary_loans,
                expected,
                context,
            );
        }
        self.release_loans(&temporary_loans, context);
        let call = if complete {
            let call = HirExpr {
                ty: if function_ty.throws_error.is_some() {
                    (*function_ty.result).clone()
                } else {
                    contextual_reference_result(&function_ty.result, expected)
                },
                kind: HirExprKind::Call {
                    function: canonical,
                    arguments: arguments.clone(),
                    consumed_callable: None,
                    diverges: self.is_uninhabited_type(&function_ty.result),
                },
            };
            if let Some(error) = function_ty.throws_error.as_deref() {
                self.lower_automatic_throws(call, error, expected, context)
            } else {
                call
            }
        } else {
            let callable_ty = partial_callable_ty(
                canonical.clone(),
                consumed_groups,
                FunctionTy {
                    groups: function_ty.groups[consumed_groups..].to_vec(),
                    unsafe_effect: function_ty.unsafe_effect,
                    throws_error: function_ty.throws_error.clone(),
                    custom_effects: function_ty.custom_effects.clone(),
                    result: function_ty.result.clone(),
                },
                &arguments,
            );
            HirExpr {
                ty: callable_ty,
                kind: HirExprKind::Partial {
                    function: canonical,
                    consumed_groups,
                    captures: arguments.clone(),
                },
            }
        };
        self.wrap_call_argument_temporaries(call, &mut arguments, temporary_bindings, context)
    }

    fn lower_local_closure_call(
        &mut self,
        local_name: &str,
        local: &LocalInfo,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let closure = local
            .closure
            .as_ref()
            .expect("closure call requires closure metadata");
        if groups.len() < closure.groups.len() {
            self.error(format!(
                "curried closures require all {} parameter groups in one call; partial application of closure `{local_name}` is not supported",
                closure.groups.len()
            ));
            return error_expr();
        }
        if groups.len() > closure.groups.len() {
            self.error(format!(
                "too many parameter groups in call to closure `{local_name}`: expected {}, found {}",
                closure.groups.len(),
                groups.len()
            ));
            return error_expr();
        }
        for (index, (arguments, parameters)) in groups.iter().zip(&closure.groups).enumerate() {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in group {} of closure `{local_name}`: expected {}, found {}",
                    index + 1,
                    parameters.len(),
                    arguments.len()
                ));
            }
        }
        if closure.unsafe_effect && context.unsafe_depth == 0 {
            self.error(format!(
                "call to unsafe closure `{local_name}` requires an `unsafe` handler"
            ));
        }
        let missing = closure
            .custom_effects
            .iter()
            .filter(|effect| !context.active_custom_effects.contains(*effect))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.report_missing_custom_effects(format!("call to closure `{local_name}`"), missing);
        }

        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            ty: local.ty.clone(),
            capability: LocalCapability::Owned,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        let leaves = self.place_leaf_keys(&callable);
        let callable_kind = if closure.is_fn_once {
            "FnOnce closure"
        } else {
            "closure"
        };
        match context.flow.initialization_status(&leaves) {
            InitializationStatus::Uninitialized => {
                self.error(format!(
                    "{callable_kind} `{local_name}` was moved or already consumed"
                ));
            }
            InitializationStatus::MaybeUninitialized => {
                self.error(format!(
                    "{callable_kind} `{local_name}` may have been moved or consumed"
                ));
            }
            InitializationStatus::Initialized if closure.is_fn_once => {
                self.mark_moved(&callable, context)
            }
            InitializationStatus::Initialized => {}
        }

        let mut lowered_arguments: Vec<_> = closure
            .captures
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, capture)| match capture.mode {
                ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable
                    if capture.forwarded.is_some() =>
                {
                    let forwarded = capture.forwarded.expect("checked forwarded capture");
                    HirArgument::CallableCaptureBorrow {
                        binding: forwarded.binding,
                        index: forwarded.index,
                        callable_ty: forwarded.callable_ty,
                        capture_ty: capture.place.ty,
                        mutable: capture.mode == ClosureCaptureMode::Mutable,
                    }
                }
                ClosureCaptureMode::Shared => HirArgument::SharedBorrow(capture.place),
                ClosureCaptureMode::Mutable => HirArgument::MutBorrow(capture.place),
                ClosureCaptureMode::Move => {
                    let (binding, callable_ty) = capture
                        .forwarded
                        .map(|forwarded| (forwarded.binding, forwarded.callable_ty))
                        .unwrap_or_else(|| (local.id, local.ty.clone()));
                    HirArgument::Move(HirExpr {
                        ty: capture.place.ty,
                        kind: HirExprKind::PartialCapture {
                            binding,
                            index,
                            moves: true,
                            callable_ty,
                        },
                    })
                }
            })
            .collect();
        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        for (group_index, (argument_group, parameters)) in
            groups.iter().zip(&closure.groups).enumerate()
        {
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) = self.ordered_call_arguments(
                local_name,
                group_index + 1,
                argument_group,
                &parameter_names,
            ) else {
                return error_expr();
            };
            for (argument, parameter) in ordered.into_iter().zip(parameters) {
                lowered_arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                    &mut temporary_bindings,
                ));
            }
        }
        self.release_loans(&temporary_loans, context);
        let call = HirExpr {
            ty: closure.result.clone(),
            kind: HirExprKind::Call {
                function: closure.function.clone(),
                arguments: lowered_arguments.clone(),
                consumed_callable: closure.is_fn_once.then_some(local.id),
                diverges: self.is_uninhabited_type(&closure.result),
            },
        };
        let call = if let Some(error) = closure.throws_error.as_ref() {
            self.lower_automatic_throws(call, error, expected, context)
        } else {
            call
        };
        self.wrap_call_argument_temporaries(
            call,
            &mut lowered_arguments,
            temporary_bindings,
            context,
        )
    }

    fn lower_indirect_function_call(
        &mut self,
        local_name: &str,
        local: &LocalInfo,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Ty::Function(function_ty) = &local.ty else {
            return error_expr();
        };
        let function_ty = function_ty.clone();
        if groups.len() != function_ty.groups.len() {
            self.error(format!(
                "indirect call `{local_name}` must supply all {} runtime parameter groups; found {}",
                function_ty.groups.len(),
                groups.len()
            ));
            return error_expr();
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "indirect call `{local_name}` uses a callable type without parameter labels"
            ));
            return error_expr();
        }
        if function_ty.unsafe_effect && context.unsafe_depth == 0 {
            self.error(format!(
                "indirect call to unsafe callable `{local_name}` requires an `unsafe` handler"
            ));
        }
        let missing = function_ty
            .custom_effects
            .iter()
            .filter(|effect| !context.active_custom_effects.contains(*effect))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.report_missing_custom_effects(format!("indirect call `{local_name}`"), missing);
        }

        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        let mut lowered_arguments = Vec::new();
        for (arguments, parameters) in groups.iter().zip(&function_ty.groups) {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in indirect call `{local_name}`: expected {}, found {}",
                    parameters.len(),
                    arguments.len()
                ));
                return error_expr();
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                lowered_arguments.push(self.lower_call_argument(
                    &argument.value,
                    &ParamSig {
                        name: String::new(),
                        ty: parameter.clone(),
                        mode: PassMode::Inferred,
                    },
                    context,
                    &mut temporary_loans,
                    &mut temporary_bindings,
                ));
            }
        }
        self.release_loans(&temporary_loans, context);
        let place = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            ty: local.ty.clone(),
            capability: local.capability,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        let call = HirExpr {
            ty: (*function_ty.result).clone(),
            kind: HirExprKind::IndirectCall {
                callee: Box::new(HirExpr {
                    ty: local.ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Copy,
                    },
                }),
                arguments: lowered_arguments.clone(),
                diverges: self.is_uninhabited_type(&function_ty.result),
            },
        };
        let call = if let Some(error) = function_ty.throws_error.as_deref() {
            self.lower_automatic_throws(call, error, expected, context)
        } else {
            call
        };
        self.wrap_call_argument_temporaries(
            call,
            &mut lowered_arguments,
            temporary_bindings,
            context,
        )
    }

    fn lower_named_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Some(materialized) = self.materialize_direct_handler_action(name, groups) {
            return self.lower_expr(&materialized, expected, context);
        }
        if let Some(distributed) = self.distribute_static_handler_selection(name, groups, context) {
            return self.lower_expr(&distributed, expected, context);
        }
        if let Some((specialized, specialized_groups)) =
            self.specialize_static_handler_call(name, groups, context)
        {
            let specialized_group_refs = specialized_groups
                .iter()
                .map(Vec::as_slice)
                .collect::<Vec<_>>();
            return self.lower_named_function_call(
                &specialized,
                &specialized_group_refs,
                expected,
                context,
            );
        }
        let function_ty = self.function_type(name);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[name].clone();
        if groups.len() > function_ty.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected {}, found {}",
                function_ty.groups.len(),
                groups.len()
            ));
            return error_expr();
        }

        let mut arguments = Vec::new();
        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        for (group_index, (arguments_ast, params)) in
            groups.iter().zip(&signature.groups).enumerate()
        {
            let parameter_names = params
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) =
                self.ordered_call_arguments(name, group_index + 1, arguments_ast, &parameter_names)
            else {
                return error_expr();
            };
            for (parameter_index, (argument, parameter)) in
                ordered.into_iter().zip(params).enumerate()
            {
                if let Some(action) = self.runtime_handler_actions.get(&(
                    name.to_owned(),
                    group_index,
                    parameter_index,
                )) {
                    if let Expr::Name(local_name) = &argument.value {
                        if context.lookup(local_name).is_some_and(|local| {
                            local.closure.as_ref().is_some_and(|closure| {
                                !matches!(closure.groups.as_slice(), [group] if group.len() == 2)
                            })
                        }) {
                            self.error(
                                "a source effect closure passed to a reusable handler must currently be passed directly from its original explicitly typed binding; callable aliases and other erased action values are not connected yet",
                            );
                            arguments.push(HirArgument::Move(error_expr()));
                            continue;
                        }
                    }
                    let expected_action = Ty::EffectCallable {
                        input: Box::new(action.input.clone()),
                        output: Box::new(action.output.clone()),
                        answer: Box::new(action.answer.clone()),
                    };
                    let erased = Expr::Call(
                        Box::new(Expr::Name("$handler$erase$effect$callable".to_owned())),
                        vec![CallArg {
                            label: None,
                            value: argument.value.clone(),
                        }],
                    );
                    let erased = self.lower_expr(&erased, Some(&expected_action), context);
                    self.require_same_type(
                        &erased.ty,
                        &expected_action,
                        format_args!("handler action parameter `{}`", parameter.name),
                    );
                    arguments.push(HirArgument::Move(erased));
                    continue;
                }
                arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                    &mut temporary_bindings,
                ));
            }
        }

        let complete = groups.len() == function_ty.groups.len();
        if complete {
            self.require_function_effects(name, context);
        }
        if !complete
            && arguments.iter().any(|argument| {
                matches!(
                    argument,
                    HirArgument::SharedBorrow(_) | HirArgument::MutBorrow(_)
                ) || matches!(argument, HirArgument::Copy(value) | HirArgument::Move(value) if matches!(value.ty, Ty::Reference { .. }))
            })
        {
            self.error("partial application cannot capture borrowed arguments");
        }
        if complete {
            self.promote_returned_reference_loans(
                name,
                &function_ty.result,
                &arguments,
                &temporary_bindings,
                &mut temporary_loans,
                expected,
                context,
            );
        }
        self.release_loans(&temporary_loans, context);

        let call = if complete {
            let call = HirExpr {
                ty: if function_ty.throws_error.is_some() {
                    (*function_ty.result).clone()
                } else {
                    contextual_reference_result(&function_ty.result, expected)
                },
                kind: HirExprKind::Call {
                    function: name.to_owned(),
                    arguments: arguments.clone(),
                    consumed_callable: None,
                    diverges: self.is_uninhabited_type(&function_ty.result),
                },
            };
            if let Some(error) = function_ty.throws_error.as_deref() {
                self.lower_automatic_throws(call, error, expected, context)
            } else {
                call
            }
        } else {
            let remaining = function_ty.groups[groups.len()..].to_vec();
            let callable_ty = partial_callable_ty(
                name.to_owned(),
                groups.len(),
                FunctionTy {
                    groups: remaining,
                    unsafe_effect: function_ty.unsafe_effect,
                    throws_error: function_ty.throws_error.clone(),
                    custom_effects: function_ty.custom_effects.clone(),
                    result: function_ty.result.clone(),
                },
                &arguments,
            );
            HirExpr {
                ty: callable_ty,
                kind: HirExprKind::Partial {
                    function: name.to_owned(),
                    consumed_groups: groups.len(),
                    captures: arguments.clone(),
                },
            }
        };
        self.wrap_call_argument_temporaries(call, &mut arguments, temporary_bindings, context)
    }

    fn materialize_direct_handler_action(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
    ) -> Option<Expr> {
        let function = self.functions.get(name)?.clone();
        if groups.len() != function.groups.len() {
            return None;
        }
        let action_positions = self
            .runtime_handler_actions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for (candidate, group_index, parameter_index) in action_positions {
            if candidate != name {
                continue;
            }
            let arguments = groups.get(group_index).copied()?;
            let parameter = function.groups.get(group_index)?.get(parameter_index)?;
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                parameter_index
            } else {
                arguments.iter().position(|argument| {
                    argument.label.as_deref() == Some(parameter.name.as_str())
                })?
            };
            let Some(CallArg {
                value: Expr::Closure(_, _),
                ..
            }) = arguments.get(argument_index)
            else {
                continue;
            };
            let mut rewritten_groups = groups
                .iter()
                .map(|group| group.to_vec())
                .collect::<Vec<_>>();
            let mut bindings = Vec::new();
            for (earlier_group, rewritten_group) in rewritten_groups
                .iter_mut()
                .enumerate()
                .take(group_index + 1)
            {
                let end = if earlier_group == group_index {
                    argument_index
                } else {
                    rewritten_group.len()
                };
                for (earlier_argument, rewritten_argument) in
                    rewritten_group.iter_mut().enumerate().take(end)
                {
                    let earlier_parameter =
                        function.groups.get(earlier_group)?.get(earlier_argument)?;
                    let parameter_ty = self.lower_source_type(&earlier_parameter.ty);
                    if matches!(
                        self.effective_pass_mode(earlier_parameter.mode, &parameter_ty),
                        PassMode::Borrow | PassMode::MutBorrow
                    ) {
                        return None;
                    }
                    let id = self.next_closure;
                    self.next_closure += 1;
                    let local = format!("$handler$direct$argument${id}");
                    bindings.push(Stmt::Let(Binding {
                        mutable: false,
                        name: local.clone(),
                        annotation: Some(earlier_parameter.ty.clone()),
                        value: rewritten_argument.value.clone(),
                    }));
                    rewritten_argument.value = Expr::Name(local);
                }
            }
            let id = self.next_closure;
            self.next_closure += 1;
            let local = format!("$handler$direct$action${id}");
            bindings.push(Stmt::Let(Binding {
                mutable: true,
                name: local.clone(),
                annotation: Some(parameter.ty.clone()),
                value: arguments[argument_index].value.clone(),
            }));
            rewritten_groups[group_index][argument_index].value = Expr::Name(local);
            let mut call = Expr::Name(name.to_owned());
            for group in rewritten_groups {
                call = Expr::Call(Box::new(call), group);
            }
            return Some(Expr::Block(bindings, Some(Box::new(call))));
        }
        None
    }

    fn specialize_capturing_handler_action_binding(
        &mut self,
        binding: &Binding,
        call: &mut Expr,
        context: &LowerCtx,
    ) -> bool {
        let Some(Type::Function { effects, .. }) = binding.annotation.as_ref() else {
            return false;
        };
        let Expr::Closure(parameters, body) = &binding.value else {
            return false;
        };
        let mut group_refs = Vec::new();
        let Expr::Name(target) = flatten_call(call, &mut group_refs) else {
            return false;
        };
        let Some(function) = self.functions.get(target).cloned() else {
            return false;
        };
        if group_refs.len() != function.groups.len() {
            return false;
        }

        let mut action_position = None;
        for ((candidate, group_index, parameter_index), action) in &self.runtime_handler_actions {
            if candidate != target
                || !effects
                    .custom
                    .iter()
                    .any(|effect| source_effect_identity(effect) == action.effect)
            {
                continue;
            }
            let arguments = group_refs.get(*group_index).copied().unwrap_or_default();
            let parameter = &function.groups[*group_index][*parameter_index];
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                *parameter_index
            } else {
                let Some(index) = arguments.iter().position(|argument| {
                    argument.label.as_deref() == Some(parameter.name.as_str())
                }) else {
                    continue;
                };
                index
            };
            if matches!(arguments.get(argument_index), Some(CallArg { value: Expr::Name(name), .. }) if name == &binding.name)
            {
                action_position = Some((
                    *group_index,
                    *parameter_index,
                    argument_index,
                    action.clone(),
                    parameter.name.clone(),
                ));
                break;
            }
        }
        let Some((group_index, parameter_index, argument_index, action, parameter_name)) =
            action_position
        else {
            return false;
        };

        let mut bound = parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        let mut captures = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, context, &mut captures) {
            return false;
        }
        if captures.iter().any(|capture| {
            context
                .lookup(&capture.name)
                .is_none_or(|local| match capture.mode {
                    ClosureCaptureMode::Shared => !self.is_copy_type(&local.ty),
                    ClosureCaptureMode::Mutable => {
                        !local.mutable
                            || local.capability != LocalCapability::Owned
                            || !self.is_copy_type(&local.ty)
                    }
                    ClosureCaptureMode::Move => {
                        local.capability != LocalCapability::Owned
                            || !matches!(
                                local.ty,
                                Ty::Struct(_)
                                    | Ty::Enum(_)
                                    | Ty::Callable(_)
                                    | Ty::Continuation { .. }
                                    | Ty::EffectCallable { .. }
                            )
                    }
                })
        }) {
            return false;
        }

        let specialization = self.next_closure;
        self.next_closure += 1;
        let canonical = format!("$capturing$handler${}${specialization}", hex_name(target));
        let mut specialized = function;
        specialized.name = canonical.clone();
        specialized.groups[group_index].remove(parameter_index);
        let mut replacements = HashMap::new();
        let mut lifted_arguments = Vec::new();
        for (index, capture) in captures.iter().enumerate() {
            let local = context
                .lookup(&capture.name)
                .expect("capture scanner records visible locals");
            let Some(source_ty) = self.source_type_for_ty(&local.ty) else {
                return false;
            };
            let lifted = format!("$handler$action$capture${specialization}${index}");
            replacements.insert(capture.name.clone(), lifted.clone());
            let mode = match capture.mode {
                ClosureCaptureMode::Shared => PassMode::Borrow,
                ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                ClosureCaptureMode::Move => PassMode::Move,
            };
            specialized.groups[group_index].insert(
                parameter_index + index,
                Param {
                    mode,
                    access: None,
                    passing: None,
                    region: None,
                    name: lifted.clone(),
                    ty: source_ty,
                },
            );
            lifted_arguments.push((lifted, capture.name.clone()));
        }
        let mut injected = binding.clone();
        injected.name = parameter_name;
        rewrite_static_function_values(&mut injected.value, &replacements);
        let Some(specialized_body) = specialized.body.as_mut() else {
            return false;
        };
        if !inject_handler_action_binding(specialized_body, &action.effect, injected) {
            return false;
        }

        let signature = FunctionSig {
            groups: specialized
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|parameter| ParamSig {
                            name: parameter.name.clone(),
                            ty: self.lower_source_type(&parameter.ty),
                            mode: parameter.mode,
                        })
                        .collect()
                })
                .collect(),
            unsafe_effect: self.function_effects_unsafe(&specialized.effects),
            throws_error: specialized
                .effects
                .throws
                .as_deref()
                .map(|error| self.lower_source_type(error)),
            custom_effects: self.function_effects_custom_identities(&specialized.effects),
            result: specialized
                .return_type
                .as_ref()
                .map(|result| self.lower_source_type(result)),
        };
        self.functions.insert(canonical.clone(), specialized);
        self.signatures.insert(canonical.clone(), signature);
        self.function_origins
            .insert(canonical.clone(), self.function_origins[target].clone());
        let origin = self
            .function_origins
            .get(target)
            .cloned()
            .unwrap_or_default();
        let access = self
            .function_accesses
            .get(target)
            .cloned()
            .unwrap_or(AccessBoundary {
                visibility: Visibility::Private,
                origin,
            });
        self.function_accesses.insert(canonical.clone(), access);
        self.function_order.push(canonical.clone());

        let mut groups = group_refs
            .iter()
            .map(|group| group.to_vec())
            .collect::<Vec<_>>();
        let labeled = groups[group_index]
            .iter()
            .all(|argument| argument.label.is_some());
        groups[group_index].remove(argument_index);
        for (offset, (label, name)) in lifted_arguments.into_iter().enumerate() {
            groups[group_index].insert(
                argument_index + offset,
                CallArg {
                    label: labeled.then_some(label),
                    value: Expr::Name(name),
                },
            );
        }
        let mut rewritten = Expr::Name(canonical);
        for group in groups {
            rewritten = Expr::Call(Box::new(rewritten), group);
        }
        *call = rewritten;
        true
    }

    fn distribute_static_handler_selection(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<Expr> {
        let function = self.functions.get(name)?.clone();
        if groups.len() != function.groups.len() || function.groups.first()?.is_empty() {
            return None;
        }
        let Type::Function { effects, .. } = &function.groups[0][0].ty else {
            return None;
        };
        if !effects.custom.iter().any(|effect| {
            let identity = source_effect_identity(effect);
            let root = identity.split('(').next().unwrap_or(&identity);
            self.effect_defs
                .get(root)
                .is_some_and(|definition| !definition.operations.is_empty())
        }) {
            return None;
        }

        let mut ordered_groups = Vec::with_capacity(groups.len());
        for (parameters, arguments) in function.groups.iter().zip(groups) {
            if parameters.len() != arguments.len() {
                return None;
            }
            let ordered = if arguments.iter().all(|argument| argument.label.is_none()) {
                Some(arguments.to_vec())
            } else if arguments.iter().all(|argument| argument.label.is_some()) {
                parameters
                    .iter()
                    .map(|parameter| {
                        let mut matches = arguments.iter().filter(|argument| {
                            argument.label.as_deref() == Some(parameter.name.as_str())
                        });
                        let argument = matches.next()?.clone();
                        matches.next().is_none().then_some(argument)
                    })
                    .collect::<Option<Vec<_>>>()
            } else {
                None
            }?;
            ordered_groups.push(ordered);
        }

        let mut targets = Vec::new();
        let selection = static_callable_selection(&ordered_groups[0][0].value, &mut targets)?;
        if targets.len() < 2
            || targets.iter().any(|target| {
                !self.functions.contains_key(target)
                    && context.lookup(target).is_none_or(|local| {
                        local.partial.as_ref().is_none_or(|partial| {
                            partial.consumed_groups != 0 || partial.capture_count != 0
                        })
                    })
            })
        {
            return None;
        }

        let calls = targets
            .into_iter()
            .map(|target| {
                let mut target_groups = ordered_groups.clone();
                target_groups[0][0] = CallArg {
                    label: None,
                    value: Expr::Name(target),
                };
                let mut call = Expr::Name(name.to_owned());
                for group in target_groups {
                    call = Expr::Call(Box::new(call), group);
                }
                call
            })
            .collect::<Vec<_>>();
        Some(replace_static_selection_leaves(selection, &calls))
    }

    fn specialize_static_handler_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> Option<(String, Vec<Vec<CallArg>>)> {
        if let Some(specialized) = self.specialize_stored_handler_action_call(name, groups, context)
        {
            return Some(specialized);
        }
        let mut function = self.functions.get(name)?.clone();
        if groups.len() > function.groups.len() {
            return None;
        }

        let mut replacements = HashMap::new();
        let mut omitted = Vec::with_capacity(groups.len());
        let mut specialized_groups = Vec::with_capacity(groups.len());
        let mut key = String::new();
        for (group_index, (parameters, arguments)) in function.groups.iter().zip(groups).enumerate()
        {
            if parameters.len() != arguments.len() {
                return None;
            }
            let ordered = if arguments.iter().all(|argument| argument.label.is_none()) {
                Some(arguments.iter().collect::<Vec<_>>())
            } else if arguments.iter().all(|argument| argument.label.is_some()) {
                parameters
                    .iter()
                    .map(|parameter| {
                        let mut matches = arguments.iter().filter(|argument| {
                            argument.label.as_deref() == Some(parameter.name.as_str())
                        });
                        let argument = matches.next()?;
                        matches.next().is_none().then_some(argument)
                    })
                    .collect::<Option<Vec<_>>>()
            } else {
                None
            }?;
            let mut omitted_group = vec![false; parameters.len()];
            let mut runtime_arguments = Vec::new();
            for (index, (parameter, argument)) in parameters.iter().zip(ordered).enumerate() {
                let Type::Function { effects, .. } = &parameter.ty else {
                    runtime_arguments.push(argument.clone());
                    continue;
                };
                let has_algebraic_effect = effects.custom.iter().any(|effect| {
                    let identity = source_effect_identity(effect);
                    let root = identity.split('(').next().unwrap_or(&identity);
                    self.effect_defs
                        .get(root)
                        .is_some_and(|definition| !definition.operations.is_empty())
                });
                if !has_algebraic_effect {
                    runtime_arguments.push(argument.clone());
                    continue;
                }
                let Expr::Name(source_target) = &argument.value else {
                    runtime_arguments.push(argument.clone());
                    continue;
                };
                let target = if self.functions.contains_key(source_target) {
                    source_target.clone()
                } else if let Some(target) = context.lookup(source_target).and_then(|local| {
                    local.partial.as_ref().and_then(|partial| {
                        (partial.consumed_groups == 0 && partial.capture_count == 0)
                            .then(|| partial.function.clone())
                    })
                }) {
                    target
                } else {
                    runtime_arguments.push(argument.clone());
                    continue;
                };
                let Some(target_function) = self.functions.get(&target) else {
                    runtime_arguments.push(argument.clone());
                    continue;
                };
                if !target_function.compile_groups.is_empty() {
                    runtime_arguments.push(argument.clone());
                    continue;
                }
                let actual = self.function_type(&target);
                let expected = self.lower_source_type(&parameter.ty);
                if !type_is_assignable(&actual, &expected) {
                    runtime_arguments.push(argument.clone());
                    continue;
                }
                omitted_group[index] = true;
                replacements.insert(parameter.name.clone(), target.clone());
                key.push_str(&format!("{group_index}:{index}:{};", hex_name(&target)));
            }
            omitted.push(omitted_group);
            specialized_groups.push(runtime_arguments);
        }
        omitted.extend(
            function.groups[groups.len()..]
                .iter()
                .map(|group| vec![false; group.len()]),
        );
        if replacements.is_empty() {
            return None;
        }

        let canonical = format!("$static$handler${}${}", hex_name(name), hex_name(&key));
        if !self.functions.contains_key(&canonical) {
            for (group, omitted) in function.groups.iter_mut().zip(&omitted) {
                let mut index = 0;
                group.retain(|_| {
                    let keep = !omitted[index];
                    index += 1;
                    keep
                });
            }
            if let Some(body) = &mut function.body {
                rewrite_static_function_values(body, &replacements);
            }
            function.name = canonical.clone();
            let signature = FunctionSig {
                groups: function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|parameter| ParamSig {
                                name: parameter.name.clone(),
                                ty: self.lower_source_type(&parameter.ty),
                                mode: parameter.mode,
                            })
                            .collect()
                    })
                    .collect(),
                unsafe_effect: self.function_effects_unsafe(&function.effects),
                throws_error: function
                    .effects
                    .throws
                    .as_deref()
                    .map(|error| self.lower_source_type(error)),
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result: function
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result)),
            };
            self.functions.insert(canonical.clone(), function);
            self.signatures.insert(canonical.clone(), signature);
            self.function_origins
                .insert(canonical.clone(), self.function_origins[name].clone());
            let origin = self.function_origins.get(name).cloned().unwrap_or_default();
            let access = self
                .function_accesses
                .get(name)
                .cloned()
                .unwrap_or(AccessBoundary {
                    visibility: Visibility::Private,
                    origin,
                });
            self.function_accesses.insert(canonical.clone(), access);
            self.function_order.push(canonical.clone());
        }
        Some((canonical, specialized_groups))
    }

    fn specialize_stored_handler_action_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> Option<(String, Vec<Vec<CallArg>>)> {
        let function = self.functions.get(name)?.clone();
        if groups.len() != function.groups.len() {
            return None;
        }
        let mut selected = None;
        for ((candidate, group_index, parameter_index), action) in &self.runtime_handler_actions {
            if candidate != name {
                continue;
            }
            let arguments = groups.get(*group_index).copied()?;
            let parameter = function.groups.get(*group_index)?.get(*parameter_index)?;
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                *parameter_index
            } else {
                arguments.iter().position(|argument| {
                    argument.label.as_deref() == Some(parameter.name.as_str())
                })?
            };
            let Some(CallArg {
                value: Expr::Name(local_name),
                ..
            }) = arguments.get(argument_index)
            else {
                continue;
            };
            let Some(local) = context.lookup(local_name).cloned() else {
                continue;
            };
            let Some(closure) = local.closure.clone() else {
                continue;
            };
            let Some(source) = context.source_closures.get(&local.id).cloned() else {
                continue;
            };
            if closure.capture_names.len() != closure.captures.len() {
                continue;
            }
            selected = Some((
                *group_index,
                *parameter_index,
                argument_index,
                action.clone(),
                parameter.name.clone(),
                local_name.clone(),
                local,
                closure,
                source,
            ));
            break;
        }
        let (
            group_index,
            parameter_index,
            argument_index,
            action,
            parameter_name,
            local_name,
            local,
            closure,
            mut source,
        ) = selected?;

        let specialization = self.next_closure;
        self.next_closure += 1;
        let canonical = format!("$stored$handler${}${specialization}", hex_name(name));
        let mut specialized = function;
        specialized.name = canonical.clone();
        specialized.groups[group_index].remove(parameter_index);
        let mut replacements = HashMap::new();
        let mut lifted_arguments = Vec::new();
        for (index, (capture_name, capture)) in closure
            .capture_names
            .iter()
            .zip(&closure.captures)
            .enumerate()
        {
            let source_ty = self.source_type_for_ty(&capture.place.ty)?;
            let lifted = format!("$handler$stored$capture${specialization}${index}");
            replacements.insert(capture_name.clone(), lifted.clone());
            let mode = match capture.mode {
                ClosureCaptureMode::Shared => PassMode::Borrow,
                ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                ClosureCaptureMode::Move => PassMode::Move,
            };
            specialized.groups[group_index].insert(
                parameter_index + index,
                Param {
                    mode,
                    access: None,
                    passing: None,
                    region: None,
                    name: lifted.clone(),
                    ty: source_ty,
                },
            );
            lifted_arguments.push((
                lifted,
                Expr::Call(
                    Box::new(Expr::Name(format!("$handler$stored$capture${index}"))),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(local_name.clone()),
                    }],
                ),
            ));
        }
        source.name = parameter_name;
        rewrite_static_function_values(&mut source.value, &replacements);
        let specialized_body = specialized.body.as_mut()?;
        if !inject_handler_action_binding(specialized_body, &action.effect, source) {
            return None;
        }

        let signature = FunctionSig {
            groups: specialized
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|parameter| ParamSig {
                            name: parameter.name.clone(),
                            ty: self.lower_source_type(&parameter.ty),
                            mode: parameter.mode,
                        })
                        .collect()
                })
                .collect(),
            unsafe_effect: self.function_effects_unsafe(&specialized.effects),
            throws_error: specialized
                .effects
                .throws
                .as_deref()
                .map(|error| self.lower_source_type(error)),
            custom_effects: self.function_effects_custom_identities(&specialized.effects),
            result: specialized
                .return_type
                .as_ref()
                .map(|result| self.lower_source_type(result)),
        };
        self.functions.insert(canonical.clone(), specialized);
        self.signatures.insert(canonical.clone(), signature);
        self.function_origins
            .insert(canonical.clone(), self.function_origins[name].clone());
        let origin = self.function_origins.get(name).cloned().unwrap_or_default();
        let access = self
            .function_accesses
            .get(name)
            .cloned()
            .unwrap_or(AccessBoundary {
                visibility: Visibility::Private,
                origin,
            });
        self.function_accesses.insert(canonical.clone(), access);
        self.function_order.push(canonical.clone());
        self.lifted_functions
            .retain(|function| function.name != closure.function);
        self.continuation_adapters
            .retain(|adapter| adapter.function != closure.function);
        self.effect_callable_adapters
            .retain(|adapter| adapter.function != closure.function);

        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            ty: local.ty,
            capability: local.capability,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        self.ensure_available(&callable, context);
        self.mark_moved(&callable, context);
        let capture_loans = closure
            .captures
            .iter()
            .filter_map(|capture| capture.place.loan)
            .collect::<Vec<_>>();
        self.release_loans(&capture_loans, context);

        let mut rewritten_groups = groups
            .iter()
            .map(|group| group.to_vec())
            .collect::<Vec<_>>();
        let labeled = rewritten_groups[group_index]
            .iter()
            .all(|argument| argument.label.is_some());
        rewritten_groups[group_index].remove(argument_index);
        for (offset, (label, value)) in lifted_arguments.into_iter().enumerate() {
            rewritten_groups[group_index].insert(
                argument_index + offset,
                CallArg {
                    label: labeled.then_some(label),
                    value,
                },
            );
        }
        Some((canonical, rewritten_groups))
    }

    #[allow(clippy::too_many_arguments)]
    fn promote_returned_reference_loans(
        &mut self,
        function: &str,
        result: &Ty,
        arguments: &[HirArgument],
        temporary_bindings: &[HirBinding],
        temporary_loans: &mut Vec<LoanId>,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) {
        let Ty::Reference {
            mutable: result_mutable,
            region: result_region,
            ..
        } = result
        else {
            return;
        };
        let Some(source) = self.functions.get(function) else {
            self.error(format!(
                "internal error: reference-returning function `{function}` has no source signature"
            ));
            return;
        };
        let source_parameters = source.groups.iter().flatten().cloned().collect::<Vec<_>>();
        if source_parameters.len() != arguments.len() {
            self.error(format!(
                "internal error: reference-returning call `{function}` lost parameter alignment"
            ));
            return;
        }
        let temporary_ids = temporary_bindings
            .iter()
            .map(|binding| binding.id)
            .collect::<HashSet<_>>();
        let mut sources = Vec::new();
        for (parameter, argument) in source_parameters.into_iter().zip(arguments) {
            let parameter_region = match (&parameter.mode, &parameter.ty) {
                (PassMode::Borrow | PassMode::MutBorrow, _) => Some(&parameter.region),
                (_, Type::Borrow { region, .. }) => Some(region),
                _ => None,
            };
            let Some(parameter_region) = parameter_region else {
                continue;
            };
            if result_region.is_some() && parameter_region != result_region {
                continue;
            }
            let (place, origin) = match argument {
                HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => (
                    Some(place),
                    context
                        .borrowed_parameter_regions
                        .get(&place.local)
                        .cloned(),
                ),
                HirArgument::Copy(value) | HirArgument::Move(value) => {
                    let place = match &value.kind {
                        HirExprKind::Borrow { place, .. } | HirExprKind::Read { place, .. } => {
                            Some(place)
                        }
                        _ => None,
                    };
                    (place, self.reference_origin_for_hir_expr(value, context))
                }
                HirArgument::CallableCaptureBorrow { .. } => (None, None),
            };
            if let Some(place) = place {
                if temporary_ids.contains(&place.local) {
                    self.error("a returned borrow cannot originate from a temporary call argument");
                }
                if let Some(loan) = place.loan {
                    temporary_loans.retain(|candidate| *candidate != loan);
                    let lexical_loans = &mut context
                        .scopes
                        .last_mut()
                        .expect("call lowering has a lexical scope")
                        .lexical_loans;
                    if !lexical_loans.contains(&loan) {
                        lexical_loans.push(loan);
                    }
                }
            }
            sources.push(origin);
        }
        if sources.is_empty() {
            self.error(format!(
                "reference-returning call `{function}` has no argument for {}",
                display_region(result_region.as_deref())
            ));
        }

        if let Some(Ty::Reference {
            mutable: expected_mutable,
            region: expected_region,
            ..
        }) = expected
        {
            if result_region.is_some() && expected_region != result_region {
                self.error(format!(
                    "returned call region mismatch: expected {}, found {}",
                    display_region(expected_region.as_deref()),
                    display_region(result_region.as_deref())
                ));
            }
            if *expected_mutable && !result_mutable {
                self.error("cannot return a shared call result as a mutable borrow");
            }
            for source in sources {
                match source {
                    Some((source_region, source_mutable)) => {
                        if expected_region.is_some() && source_region != *expected_region {
                            self.error(format!(
                                "returned call argument region mismatch: expected {}, found {}",
                                display_region(expected_region.as_deref()),
                                display_region(source_region.as_deref())
                            ));
                        }
                        if *expected_mutable && !source_mutable {
                            self.error(
                                "cannot return a mutable call result through a shared borrow parameter",
                            );
                        }
                    }
                    None if context.reference_value_depth > 0 => {}
                    None => self.error(
                        "cannot return a call result borrowing a local value; its source must be a region-bound borrow parameter",
                    ),
                }
            }
        }
    }

    fn resolve_function_overload(&mut self, name: &str, groups: &[&[CallArg]]) -> Option<String> {
        let candidates = self.function_overloads.get(name)?.clone();
        if !groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "overloaded call `{name}` requires named arguments to select an overload"
            ));
            return None;
        }
        let matches = self.matching_function_overloads(&candidates, groups, 0);
        match matches.as_slice() {
            [selected] => Some(selected.clone()),
            [] => {
                let supplied = groups
                    .iter()
                    .map(|group| {
                        format!(
                            "({})",
                            group
                                .iter()
                                .filter_map(|argument| argument.label.as_deref())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");
                self.error(format!(
                    "no overload of `{name}` matches named parameter groups {supplied}"
                ));
                None
            }
            _ => {
                self.error(format!(
                    "overloaded call `{name}` remains ambiguous; supply a parameter group whose names distinguish one overload"
                ));
                None
            }
        }
    }

    fn resolve_inherent_overload(
        &mut self,
        target: &str,
        member: &str,
        is_method: bool,
        groups: &[&[CallArg]],
    ) -> Option<String> {
        let key = (target.to_owned(), member.to_owned(), is_method);
        let candidates = self.inherent_overloads.get(&key)?.clone();
        if !groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "overloaded call `{target}.{member}` requires named arguments to select an overload"
            ));
            return None;
        }
        let matches = self.matching_function_overloads(&candidates, groups, usize::from(is_method));
        match matches.as_slice() {
            [selected] => Some(selected.clone()),
            [] => {
                self.error(format!(
                    "no overload of `{target}.{member}` matches the supplied named parameter groups"
                ));
                None
            }
            _ => {
                self.error(format!(
                    "overloaded call `{target}.{member}` remains ambiguous; name a parameter from a distinguishing group"
                ));
                None
            }
        }
    }

    fn matching_function_overloads(
        &self,
        candidates: &[String],
        groups: &[&[CallArg]],
        parameter_group_offset: usize,
    ) -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| {
                let parameter_names = if let Some(signature) = self.signatures.get(*candidate) {
                    signature.groups[parameter_group_offset..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else if let Some(template) = self.function_templates.get(*candidate) {
                    template.groups[parameter_group_offset..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else {
                    return false;
                };
                let matches_runtime = |runtime_groups: &[&[CallArg]]| {
                    if runtime_groups.len() > parameter_names.len() {
                        return false;
                    }
                    runtime_groups
                        .iter()
                        .zip(&parameter_names)
                        .all(|(arguments, parameters)| {
                            if arguments.len() != parameters.len() {
                                return false;
                            }
                            let labeled = arguments
                                .iter()
                                .filter(|argument| argument.label.is_some())
                                .count();
                            labeled == 0
                                || labeled == arguments.len()
                                    && parameters.iter().all(|parameter| {
                                        arguments
                                            .iter()
                                            .filter(|argument| {
                                                argument.label.as_deref()
                                                    == Some(parameter.as_str())
                                            })
                                            .count()
                                            == 1
                                    })
                        })
                };
                if self.signatures.contains_key(*candidate) {
                    matches_runtime(groups)
                } else {
                    let compile_group_count =
                        self.function_templates[*candidate].compile_groups.len();
                    (0..=compile_group_count.min(groups.len())).any(|runtime_start| {
                        groups[runtime_start..]
                            .iter()
                            .flat_map(|group| group.iter())
                            .any(|argument| argument.label.is_some())
                            && matches_runtime(&groups[runtime_start..])
                    })
                }
            })
            .cloned()
            .collect()
    }

    fn ordered_call_arguments<'a>(
        &mut self,
        owner: &str,
        group_number: usize,
        arguments: &'a [CallArg],
        parameter_names: &[String],
    ) -> Option<Vec<&'a CallArg>> {
        if arguments.len() != parameter_names.len() {
            self.error(format!(
                "argument count mismatch in group {group_number} of `{owner}`: expected {}, found {}",
                parameter_names.len(),
                arguments.len()
            ));
            return None;
        }
        if arguments.iter().all(|argument| argument.label.is_none()) {
            return Some(arguments.iter().collect());
        }
        if arguments.iter().any(|argument| argument.label.is_none()) {
            self.error(format!(
                "cannot mix named and positional arguments in group {group_number} of `{owner}`"
            ));
            return None;
        }

        let mut ordered = vec![None; parameter_names.len()];
        for (source_index, argument) in arguments.iter().enumerate() {
            let label = argument.label.as_deref().expect("all arguments are named");
            let Some(index) = parameter_names.iter().position(|name| name == label) else {
                self.error(format!(
                    "unknown parameter `{label}` in group {group_number} of `{owner}`"
                ));
                return None;
            };
            if index != source_index {
                self.error(format!(
                    "named arguments in group {group_number} of `{owner}` must follow parameter declaration order; expected `{}` before `{label}`",
                    parameter_names[source_index]
                ));
                return None;
            }
            if ordered[index].replace(argument).is_some() {
                self.error(format!(
                    "duplicate argument for parameter `{label}` in group {group_number} of `{owner}`"
                ));
                return None;
            }
        }
        for (index, argument) in ordered.iter().enumerate() {
            if argument.is_none() {
                self.error(format!(
                    "missing argument for parameter `{}` in group {group_number} of `{owner}`",
                    parameter_names[index]
                ));
                return None;
            }
        }
        Some(ordered.into_iter().flatten().collect())
    }

    fn lower_local_partial_call(
        &mut self,
        local_name: &str,
        local: &LocalInfo,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let partial = local
            .partial
            .as_ref()
            .expect("partial call requires partial metadata");
        let function_ty = self.function_type(&partial.function);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[&partial.function].clone();
        let remaining_groups = function_ty.groups.len() - partial.consumed_groups;
        if groups.len() > remaining_groups {
            self.error(format!(
                "too many parameter groups in call to `{local_name}`: expected at most {remaining_groups}, found {}",
                groups.len()
            ));
            return error_expr();
        }

        let captured_params: Vec<_> = signature
            .groups
            .iter()
            .take(partial.consumed_groups)
            .flatten()
            .cloned()
            .collect();
        if captured_params.len() != partial.capture_count {
            self.error(format!(
                "internal error: invalid capture count for partial `{local_name}`"
            ));
            return error_expr();
        }
        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            ty: local.ty.clone(),
            capability: LocalCapability::Owned,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        let leaves = self.place_leaf_keys(&callable);
        let callable_kind = if partial.is_fn_once {
            "FnOnce partial application"
        } else {
            "partial application"
        };
        match context.flow.initialization_status(&leaves) {
            InitializationStatus::Uninitialized => {
                self.error(format!(
                    "{callable_kind} `{local_name}` was moved or already consumed"
                ));
            }
            InitializationStatus::MaybeUninitialized => {
                self.error(format!(
                    "{callable_kind} `{local_name}` may have been moved or consumed"
                ));
            }
            InitializationStatus::Initialized if partial.is_fn_once => {
                self.mark_moved(&callable, context)
            }
            InitializationStatus::Initialized => {}
        }
        let mut arguments: Vec<_> = captured_params
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                let capture = HirExpr {
                    ty: parameter.ty.clone(),
                    kind: HirExprKind::PartialCapture {
                        binding: local.id,
                        index,
                        moves: self.effective_pass_mode(parameter.mode, &parameter.ty)
                            == PassMode::Move,
                        callable_ty: local.ty.clone(),
                    },
                };
                match self.effective_pass_mode(parameter.mode, &parameter.ty) {
                    PassMode::Copy => HirArgument::Copy(capture),
                    PassMode::Move => HirArgument::Move(capture),
                    PassMode::Borrow | PassMode::MutBorrow => {
                        unreachable!("borrowed partial applications are rejected at creation")
                    }
                    PassMode::Inferred => unreachable!("effective mode is explicit"),
                }
            })
            .collect();

        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        for (relative_group, arguments_ast) in groups.iter().enumerate() {
            let group_index = partial.consumed_groups + relative_group;
            let params = &signature.groups[group_index];
            let parameter_names = params
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) = self.ordered_call_arguments(
                local_name,
                relative_group + 1,
                arguments_ast,
                &parameter_names,
            ) else {
                return error_expr();
            };
            for (argument, parameter) in ordered.into_iter().zip(params) {
                arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                    &mut temporary_bindings,
                ));
            }
        }

        let consumed_groups = partial.consumed_groups + groups.len();
        if consumed_groups == function_ty.groups.len() {
            self.require_function_effects(&partial.function, context);
        }
        if consumed_groups != function_ty.groups.len()
            && arguments.iter().any(|argument| {
                matches!(
                    argument,
                    HirArgument::SharedBorrow(_) | HirArgument::MutBorrow(_)
                ) || matches!(argument, HirArgument::Copy(value) | HirArgument::Move(value) if matches!(value.ty, Ty::Reference { .. }))
            })
        {
            self.error("partial application cannot capture borrowed arguments");
        }
        self.release_loans(&temporary_loans, context);
        let call = if consumed_groups == function_ty.groups.len() {
            let call = HirExpr {
                ty: (*function_ty.result).clone(),
                kind: HirExprKind::Call {
                    function: partial.function.clone(),
                    arguments: arguments.clone(),
                    consumed_callable: partial.is_fn_once.then_some(local.id),
                    diverges: self.is_uninhabited_type(&function_ty.result),
                },
            };
            if let Some(error) = function_ty.throws_error.as_deref() {
                self.lower_automatic_throws(call, error, expected, context)
            } else {
                call
            }
        } else {
            let callable_ty = partial_callable_ty(
                partial.function.clone(),
                consumed_groups,
                FunctionTy {
                    groups: function_ty.groups[consumed_groups..].to_vec(),
                    unsafe_effect: function_ty.unsafe_effect,
                    throws_error: function_ty.throws_error.clone(),
                    custom_effects: function_ty.custom_effects.clone(),
                    result: function_ty.result.clone(),
                },
                &arguments,
            );
            HirExpr {
                ty: callable_ty,
                kind: HirExprKind::Partial {
                    function: partial.function.clone(),
                    consumed_groups,
                    captures: arguments.clone(),
                },
            }
        };
        self.wrap_call_argument_temporaries(call, &mut arguments, temporary_bindings, context)
    }

    fn lower_call_argument(
        &mut self,
        argument: &Expr,
        parameter: &ParamSig,
        context: &mut LowerCtx,
        temporary_loans: &mut Vec<LoanId>,
        temporary_bindings: &mut Vec<HirBinding>,
    ) -> HirArgument {
        if let Some((local_name, index)) = internal_stored_callable_capture(argument) {
            let Some(local) = context.lookup(local_name).cloned() else {
                self.error("internal stored callable capture refers to an unknown local");
                return HirArgument::Move(error_expr());
            };
            let Some(closure) = local.closure.as_ref() else {
                self.error("internal stored callable capture requires a closure local");
                return HirArgument::Move(error_expr());
            };
            let Some(capture) = closure.captures.get(index) else {
                self.error("internal stored callable capture index is out of bounds");
                return HirArgument::Move(error_expr());
            };
            self.require_same_type(
                &capture.place.ty,
                &parameter.ty,
                format_args!("lifted handler capture `{}`", parameter.name),
            );
            return match capture.mode {
                ClosureCaptureMode::Shared => HirArgument::CallableCaptureBorrow {
                    binding: local.id,
                    index,
                    callable_ty: local.ty,
                    capture_ty: capture.place.ty.clone(),
                    mutable: false,
                },
                ClosureCaptureMode::Mutable => HirArgument::CallableCaptureBorrow {
                    binding: local.id,
                    index,
                    callable_ty: local.ty,
                    capture_ty: capture.place.ty.clone(),
                    mutable: true,
                },
                ClosureCaptureMode::Move => HirArgument::Move(HirExpr {
                    ty: capture.place.ty.clone(),
                    kind: HirExprKind::PartialCapture {
                        binding: local.id,
                        index,
                        moves: true,
                        callable_ty: local.ty,
                    },
                }),
            };
        }
        let mode = self.effective_pass_mode(parameter.mode, &parameter.ty);
        match mode {
            PassMode::Copy | PassMode::Move => {
                let value = if matches!(parameter.ty, Ty::Reference { .. }) {
                    if let Expr::Name(name) = argument {
                        if let Some(local) = context
                            .lookup(name)
                            .cloned()
                            .filter(|local| matches!(local.ty, Ty::Reference { .. }))
                        {
                            let place = HirPlace {
                                local: local.id,
                                root_ty: local.ty.clone(),
                                projections: Vec::new(),
                                ty: local.ty,
                                capability: LocalCapability::Owned,
                                root_mutable: local.mutable,
                                loan: None,
                                indirect: false,
                            };
                            let mut value = self.access_place(
                                place,
                                if mode == PassMode::Copy {
                                    AccessKind::Copy
                                } else {
                                    AccessKind::Move
                                },
                                context,
                            );
                            if reference_value_types_compatible(&value.ty, &parameter.ty) {
                                value.ty = parameter.ty.clone();
                            }
                            value
                        } else {
                            self.lower_reference_value_expr(argument, &parameter.ty, context)
                        }
                    } else {
                        self.lower_reference_value_expr(argument, &parameter.ty, context)
                    }
                } else if let (Ty::Function(function_ty), Expr::Closure(params, body)) =
                    (&parameter.ty, argument)
                {
                    self.lower_noncapturing_closure_argument_as_function(
                        params,
                        body,
                        function_ty,
                        &parameter.name,
                        context,
                    )
                } else if let Some(place) = self.lower_place_without_diagnostic(argument, context) {
                    let access = if mode == PassMode::Copy {
                        AccessKind::Copy
                    } else {
                        AccessKind::Move
                    };
                    self.access_place(place, access, context)
                } else {
                    self.lower_expr(argument, Some(&parameter.ty), context)
                };
                self.require_same_type(
                    &value.ty,
                    &parameter.ty,
                    format!("argument for parameter `{}`", parameter.name),
                );
                if mode == PassMode::Copy {
                    if !self.is_copy_type(&parameter.ty) {
                        let ty = self.diagnostic_type_name(&parameter.ty);
                        self.error(format!(
                            "parameter `{}` requires Copy, but `{}` does not implement Copy",
                            parameter.name, ty
                        ));
                    }
                    HirArgument::Copy(value)
                } else {
                    HirArgument::Move(value)
                }
            }
            PassMode::Borrow | PassMode::MutBorrow => {
                let mutable = mode == PassMode::MutBorrow;
                let mut place =
                    if let Some(place) = self.lower_place_without_diagnostic(argument, context) {
                        place
                    } else {
                        let value = self.lower_expr(argument, Some(&parameter.ty), context);
                        let id = context.fresh_local();
                        let ty = value.ty.clone();
                        temporary_bindings.push(HirBinding {
                            id,
                            name: format!("$temporary argument for {}", parameter.name),
                            ty: ty.clone(),
                            mutable,
                            value,
                        });
                        HirPlace {
                            local: id,
                            root_ty: ty.clone(),
                            projections: Vec::new(),
                            ty,
                            capability: LocalCapability::Owned,
                            root_mutable: mutable,
                            loan: None,
                            indirect: false,
                        }
                    };
                self.require_same_type(
                    &place.ty,
                    &parameter.ty,
                    format!("argument for parameter `{}`", parameter.name),
                );
                if mutable {
                    self.ensure_writable(&place);
                }
                let kind = if mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };
                if let Some(loan) = self.acquire_loan(&place, kind, false, context) {
                    place.loan = Some(loan);
                    temporary_loans.push(loan);
                }
                if mutable {
                    HirArgument::MutBorrow(place)
                } else {
                    HirArgument::SharedBorrow(place)
                }
            }
            PassMode::Inferred => unreachable!("effective mode is explicit"),
        }
    }

    fn lower_noncapturing_closure_argument_as_function(
        &mut self,
        params: &[crate::ast::Param],
        body: &Expr,
        function_ty: &FunctionTy,
        parameter_name: &str,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(captures) = self.closure_literal_capture_uses(params, body, context) else {
            return error_expr();
        };
        if !captures.is_empty() {
            self.error(format!(
                "capturing closure cannot be passed to function-typed parameter `{parameter_name}` yet"
            ));
            return error_expr();
        }
        let custom_effect_sources =
            source_effect_source_map(&effect_identity_sources(&function_ty.custom_effects));
        let lowered = self.lower_local_closure(
            params,
            body,
            Some((*function_ty.result).clone()),
            ClosureEffectContext {
                unsafe_depth: usize::from(function_ty.unsafe_effect),
                throws_error: function_ty.throws_error.as_deref().cloned(),
                custom_effects: function_ty.custom_effects.iter().cloned().collect(),
                custom_effect_sources,
                lexical_handler_effects: HashSet::new(),
                lexical_handler_effect_sources: HashMap::new(),
            },
            context,
        );
        let HirExprKind::LocalClosure(closure) = lowered.kind else {
            return error_expr();
        };
        if !closure.captures.is_empty() {
            self.error(format!(
                "capturing closure cannot be passed to function-typed parameter `{parameter_name}` yet"
            ));
            return error_expr();
        }
        HirExpr {
            ty: Ty::Function(FunctionTy {
                groups: closure
                    .groups
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafe_effect: closure.unsafe_effect,
                throws_error: closure.throws_error.clone().map(Box::new),
                custom_effects: closure.custom_effects.clone(),
                result: Box::new(closure.result.clone()),
            }),
            kind: HirExprKind::Function(closure.function),
        }
    }

    fn closure_literal_capture_uses(
        &mut self,
        params: &[crate::ast::Param],
        body: &Expr,
        context: &LowerCtx,
    ) -> Option<Vec<ClosureCaptureUse>> {
        let mut bound = HashSet::new();
        let mut current_params = params;
        let mut current_body = body;
        loop {
            bound.extend(
                current_params
                    .iter()
                    .map(|parameter| parameter.name.clone()),
            );
            if let Expr::Closure(nested_params, nested_body) = current_body {
                current_params = nested_params;
                current_body = nested_body;
            } else {
                break;
            }
        }
        let mut captures = Vec::new();
        self.scan_simple_closure_captures(current_body, &mut bound, context, &mut captures)
            .then_some(captures)
    }

    fn wrap_call_argument_temporaries(
        &mut self,
        mut expression: HirExpr,
        arguments: &mut [HirArgument],
        temporary_bindings: Vec<HirBinding>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if temporary_bindings.is_empty() {
            return expression;
        }
        let mut borrowed_temporaries = temporary_bindings
            .into_iter()
            .map(|binding| (binding.id, binding))
            .collect::<HashMap<_, _>>();
        let mut statements = Vec::new();
        for argument in &mut *arguments {
            let moves = matches!(&*argument, HirArgument::Move(_));
            match argument {
                HirArgument::Copy(value) | HirArgument::Move(value) => {
                    if matches!(value.kind, HirExprKind::Function(_)) {
                        continue;
                    }
                    if let HirExprKind::Read { place, .. } = &value.kind {
                        if let Some(binding) = borrowed_temporaries.remove(&place.local) {
                            statements.push(HirStmt::Let(binding));
                        }
                    }
                    let id = context.fresh_local();
                    let ty = value.ty.clone();
                    statements.push(HirStmt::Let(HirBinding {
                        id,
                        name: "$staged call argument".to_owned(),
                        ty: ty.clone(),
                        mutable: false,
                        value: value.clone(),
                    }));
                    *value = HirExpr {
                        ty: ty.clone(),
                        kind: HirExprKind::Read {
                            place: HirPlace {
                                local: id,
                                root_ty: ty.clone(),
                                projections: Vec::new(),
                                ty,
                                capability: LocalCapability::Owned,
                                root_mutable: false,
                                loan: None,
                                indirect: false,
                            },
                            kind: if moves {
                                HirReadKind::Move
                            } else {
                                HirReadKind::Copy
                            },
                        },
                    };
                }
                HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                    if let Some(binding) = borrowed_temporaries.remove(&place.local) {
                        statements.push(HirStmt::Let(binding));
                    }
                }
                HirArgument::CallableCaptureBorrow { .. } => {}
            }
        }
        debug_assert!(borrowed_temporaries.is_empty());
        expression.kind = match expression.kind {
            HirExprKind::Call {
                function,
                consumed_callable,
                diverges,
                ..
            } => HirExprKind::Call {
                function,
                arguments: arguments.to_vec(),
                consumed_callable,
                diverges,
            },
            HirExprKind::Partial {
                function,
                consumed_groups,
                ..
            } => HirExprKind::Partial {
                function,
                consumed_groups,
                captures: arguments.to_vec(),
            },
            _ => unreachable!("call temporary wrapper requires a call or partial expression"),
        };
        HirExpr {
            ty: expression.ty.clone(),
            kind: HirExprKind::Block(statements, Some(Box::new(expression))),
        }
    }

    fn lower_struct_literal(
        &mut self,
        constructor: &Expr,
        fields: &[CallArg],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if fields.iter().any(|field| field.label.is_none()) {
            self.error("struct literal fields must be named; use `field: value` inside `{ ... }`");
            return error_expr();
        }
        let mut groups = Vec::new();
        let root = flatten_call(constructor, &mut groups);
        let Expr::Name(name) = root else {
            self.error("struct literal requires a struct type name");
            return error_expr();
        };
        if context.lookup(name).is_some() {
            self.error(format!(
                "local value `{name}` cannot be used as a struct literal constructor"
            ));
            return error_expr();
        }
        if context.has_type_parameter(name) {
            self.error(format!(
                "type parameter `{name}` cannot be used as a struct literal constructor"
            ));
            return error_expr();
        }
        if name == "Self" && !context.type_substitutions.contains_key("Self") {
            self.error("expression `Self` is only available inside an extend member");
            return error_expr();
        }
        if groups.is_empty() && self.struct_layouts.contains_key(name) {
            return self.lower_struct_constructor(name, &[fields], context);
        }
        if self.struct_templates.contains_key(name) {
            let mut construction_groups = groups;
            construction_groups.push(fields);
            let Some((canonical, runtime_start)) = self.resolve_inferred_generic_struct_instance(
                name,
                &construction_groups,
                expected,
                context,
            ) else {
                return error_expr();
            };
            return self.lower_struct_constructor(
                &canonical,
                &construction_groups[runtime_start..],
                context,
            );
        }
        if self.enum_layouts.contains_key(name) || self.enum_templates.contains_key(name) {
            self.error(format!(
                "struct literal `{name} {{ ... }}` requires a struct type, found enum `{name}`"
            ));
            return error_expr();
        }
        if self.struct_layouts.contains_key(name) {
            self.error(format!(
                "struct `{name}` does not accept type argument groups in a struct literal"
            ));
            return error_expr();
        }
        self.error(format!("unknown struct `{name}`"));
        error_expr()
    }

    fn lower_struct_constructor(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "struct constructor `{name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let Some(layout) = self.struct_layout_or_diagnostic(name) else {
            return error_expr();
        };
        let mut accessible = true;
        for field in &layout.fields {
            accessible &= self.require_field_access(name, field, &context.origin);
        }
        if !accessible {
            return error_expr();
        }
        let fields = self.lower_constructor_fields(
            groups[0],
            &layout.fields,
            true,
            &format!("struct `{name}`"),
            context,
        );
        HirExpr {
            ty: Ty::Struct(name.to_owned()),
            kind: HirExprKind::ConstructStruct {
                name: name.to_owned(),
                fields,
            },
        }
    }

    fn lower_enum_constructor(
        &mut self,
        enum_name: &str,
        variant: usize,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "enum variant constructor `{enum_name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
        let variant_layout = &layout.variants[variant];
        if variant_layout.fields.is_empty() {
            self.error(format!(
                "unit variant `{enum_name}.{}` is a value and must not be called",
                variant_layout.name
            ));
            return error_expr();
        }
        let owner = format!("{enum_name}.{}", variant_layout.name);
        let mut accessible = true;
        for field in &variant_layout.fields {
            accessible &= self.require_field_access(&owner, field, &context.origin);
        }
        if !accessible {
            return error_expr();
        }
        let fields = self.lower_constructor_fields(
            groups[0],
            &variant_layout.fields,
            variant_layout.named,
            &format!("variant `{enum_name}.{}`", variant_layout.name),
            context,
        );
        HirExpr {
            ty: Ty::Enum(enum_name.to_owned()),
            kind: HirExprKind::ConstructEnum {
                name: enum_name.to_owned(),
                variant,
                fields,
            },
        }
    }

    fn lower_constructor_fields(
        &mut self,
        arguments: &[CallArg],
        fields: &[FieldLayout],
        labels_allowed: bool,
        constructor: &str,
        context: &mut LowerCtx,
    ) -> Vec<(usize, HirExpr)> {
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            self.error(format!(
                "cannot mix labeled and positional arguments in {constructor}"
            ));
            return Vec::new();
        }

        if labeled == 0 {
            if arguments.len() != fields.len() {
                self.error(format!(
                    "argument count mismatch for {constructor}: expected {}, found {}",
                    fields.len(),
                    arguments.len()
                ));
            }
            return arguments
                .iter()
                .zip(fields)
                .enumerate()
                .map(|(index, (argument, field))| {
                    (
                        index,
                        self.lower_expr(&argument.value, Some(&field.ty), context),
                    )
                })
                .collect();
        }

        if !labels_allowed {
            self.error(format!("{constructor} does not accept labeled arguments"));
            return Vec::new();
        }
        let mut initialized = HashSet::new();
        let mut lowered = Vec::new();
        for argument in arguments {
            let label = argument
                .label
                .as_deref()
                .expect("all arguments are labeled");
            let Some((index, field)) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == label)
            else {
                self.error(format!("unknown field `{label}` in {constructor}"));
                continue;
            };
            if !initialized.insert(index) {
                self.error(format!("duplicate field `{label}` in {constructor}"));
                continue;
            }
            lowered.push((
                index,
                self.lower_expr(&argument.value, Some(&field.ty), context),
            ));
        }
        for (index, field) in fields.iter().enumerate() {
            if !initialized.contains(&index) {
                self.error(format!("missing field `{}` in {constructor}", field.name));
            }
        }
        lowered
    }

    fn resolve_short_variant(
        &mut self,
        name: &str,
        expected: Option<&Ty>,
        origin: &ItemOrigin,
    ) -> Option<(String, usize)> {
        if let Some(Ty::Enum(enum_name)) = expected {
            let layout = self.enum_layout_or_diagnostic(enum_name)?;
            let enum_is_accessible = self
                .nominal_accesses
                .get(enum_name)
                .is_some_and(|access| Self::access_boundary_allows(origin, access));
            if enum_is_accessible {
                if let Some(index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                {
                    return Some((enum_name.clone(), index));
                }
            }
        }
        let candidates: Vec<_> = self
            .enum_layouts
            .iter()
            .filter_map(|(enum_name, layout)| {
                let is_non_generic = self
                    .nominal_instances
                    .get(enum_name)
                    .is_some_and(|instance| instance.key.arguments.is_empty());
                if !is_non_generic {
                    return None;
                }
                if !self
                    .nominal_accesses
                    .get(enum_name)
                    .is_some_and(|access| Self::access_boundary_allows(origin, access))
                {
                    return None;
                }
                layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .map(|variant| (enum_name.clone(), variant))
            })
            .collect();
        match candidates.as_slice() {
            [candidate] => Some(candidate.clone()),
            [] => None,
            _ => {
                self.error(format!(
                    "variant name `{name}` is ambiguous; qualify it with its enum"
                ));
                None
            }
        }
    }

    fn require_same_type(&mut self, actual: &Ty, expected: &Ty, context: impl fmt::Display) {
        if type_is_assignable(actual, expected)
            || self.is_uninhabited_type(actual)
            || *actual == Ty::Error
            || *expected == Ty::Error
        {
            return;
        }
        self.error(format!(
            "type mismatch for {context}: expected `{expected}`, found `{actual}`"
        ));
    }

    fn require_function_effects(&mut self, name: &str, context: &LowerCtx) {
        let Some(effects) = self
            .functions
            .get(name)
            .map(|function| function.effects.clone())
        else {
            return;
        };
        if self.function_effects_unsafe(&effects) && context.unsafe_depth == 0 {
            self.error(format!(
                "call to unsafe function `{name}` requires an `unsafe` handler"
            ));
        }
        let required = self.function_effects_custom_identities(&effects);
        let missing = required
            .iter()
            .filter(|effect| !context.active_custom_effects.contains(*effect))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.report_missing_custom_effects(format!("call to `{name}`"), missing);
        }
    }

    fn report_missing_custom_effects(&mut self, prefix: String, missing: Vec<String>) {
        if missing.len() == 1 && self.effect_identity_is_standard_throws(&missing[0]) {
            self.error(format!(
                "{prefix} requires `{}`; handle it with `try {{ ... }}` or propagate it from the current function",
                missing[0]
            ));
            return;
        }
        self.error(format!(
            "{prefix} requires custom effect{} `{}`",
            if missing.len() == 1 { "" } else { "s" },
            missing.join(", ")
        ));
    }

    fn effect_identity_is_standard_throws(&self, identity: &str) -> bool {
        source_type_from_identity(identity).is_some_and(|source| {
            standard_throws_error_source(&source, self.lang_item_name(LangItemKind::ThrowsEffect))
                .is_some()
        })
    }

    fn unify_types(&mut self, left: &Ty, right: &Ty, context: impl fmt::Display) -> Ty {
        if left == right {
            return left.clone();
        }
        if self.is_uninhabited_type(left) {
            return right.clone();
        }
        if self.is_uninhabited_type(right) {
            return left.clone();
        }
        if *left == Ty::Error || *right == Ty::Error {
            return Ty::Error;
        }
        self.error(format!(
            "type mismatch for {context}: `{left}` and `{right}` cannot be unified"
        ));
        Ty::Error
    }

    fn is_uninhabited_type(&self, ty: &Ty) -> bool {
        *ty == Ty::Never
            || matches!(ty, Ty::Enum(name) if self.enum_layouts.get(name).is_some_and(|layout| layout.variants.is_empty()))
    }

    fn error(&mut self, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(message));
    }
}

#[cfg(test)]
mod tests;
