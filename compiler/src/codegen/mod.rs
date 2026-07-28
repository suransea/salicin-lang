//! Type checking and textual LLVM IR generation for Salicin's M0 subset.
//!
//! The backend intentionally consumes the parser AST directly but first lowers
//! it to a small typed representation.  No malformed program reaches the LLVM
//! emitter, which keeps the generated IR simple enough to inspect in tests.

use std::collections::{HashMap, HashSet};

use crate::alloc::AllocBundle;
use crate::ast::{
    BinaryOp, CallArg, CompileParam, Expr, Function, Item, ItemOrigin, PassMode, Program, Sort,
    Type, UnaryOp, Visibility, WherePredicate,
};
use crate::core::{CoreBundle, LangItemKind, LangItems};
use crate::manifest::Edition;
use crate::standard::StdBundle;
use crate::static_semantics::{Constraint, Goal, GoalResult};

mod access;
mod analyzer_state;
mod arrays;
mod assignment;
mod async_lowering;
mod async_source;
mod call_lowering;
mod calls;
mod chain;
mod cleanup_plan;
mod coalesce;
mod compile_time;
mod constructors;
mod control;
mod ctfe_value;
mod defer;
mod diagnostic;
mod effects;
mod emitter;
mod expression_lowering;
mod extension_collection;
mod failure;
mod fallible;
mod flow;
mod functions;
mod handlers;
mod hir;
mod inference;
mod layouts;
mod lower;
mod matches;
mod members;
mod names;
mod nominals;
mod operators;
mod ownership;
mod pipeline;
mod places;
mod probe;
mod raw;
mod references;
mod registry;
mod source_rewrite;
mod static_eval;
mod target;
mod trait_collection;
mod types;

use analyzer_state::{CollectionState, LoweringState};
use compile_time::*;
use flow::*;
use hir::*;
use lower::*;
use names::*;
use registry::*;
use source_rewrite::*;

pub use diagnostic::Diagnostic;
pub use pipeline::{check, check_library, compile, compile_library};

fn primitive_scalar_type(ty: &Ty) -> bool {
    ty.is_integer() || *ty == Ty::Bool
}

fn remaining_sort_groups(groups: &[Vec<Sort>], mut consumed: usize) -> Vec<Vec<Sort>> {
    let mut remaining = Vec::new();
    for group in groups {
        if consumed >= group.len() {
            consumed -= group.len();
            continue;
        }
        remaining.push(group[consumed..].to_vec());
        consumed = 0;
    }
    remaining
}

fn method_compile_parameter_groups_match(expected: &Function, actual: &Function) -> bool {
    let mut expected = expected.clone();
    let mut actual = actual.clone();
    alpha_normalize_method_compile_binders(&mut expected);
    alpha_normalize_method_compile_binders(&mut actual);
    compile_parameter_groups_match(&expected.compile_groups, &actual.compile_groups)
}

fn generic_method_contracts_match(expected: &Function, actual: &Function) -> bool {
    let mut expected = expected.clone();
    let mut actual = actual.clone();
    alpha_normalize_method_compile_binders(&mut expected);
    alpha_normalize_method_compile_binders(&mut actual);
    source_function_shapes_match(&expected, &actual)
        && expected.where_predicates == actual.where_predicates
}

#[cfg(test)]
use cleanup_plan::{HirCleanupPlanner, MAX_CLEANUP_MOVE_PATHS};

struct Analyzer {
    lang_items: Box<LangItems>,
    primary_package: usize,
    primary_package_identity: String,
    package_identities: HashMap<usize, String>,
    collection: CollectionState,
    lowering: LoweringState,
    diagnostics: Vec<Diagnostic>,
    current_origin: Option<Box<ItemOrigin>>,
}

impl Analyzer {
    fn try_new(program: &Program) -> Result<Box<Self>, String> {
        let core = CoreBundle::cached_for_edition(Edition::Edition2026)
            .map_err(|error| error.to_string())?;
        let alloc = AllocBundle::cached_for_edition(Edition::Edition2026)
            .map_err(|error| error.to_string())?;
        let std = StdBundle::cached_for_edition(Edition::Edition2026)
            .map_err(|error| error.to_string())?;
        let mut analyzer = Box::new(Self {
            lang_items: Box::new(core.lang_items().clone()),
            primary_package: program.primary_package,
            primary_package_identity: program.primary_package_identity.clone(),
            package_identities: program.package_identities.clone(),
            collection: CollectionState::default(),
            lowering: LoweringState::default(),
            diagnostics: Vec::new(),
            current_origin: None,
        });
        if !program.uses.is_empty() {
            analyzer.error(
                "unresolved `use` declarations reached semantic analysis; resolve source modules before code generation",
            );
        }
        let mut core_program = core.program().clone();
        remove_nonruntime_syntax_contracts(&mut core_program, &analyzer.lang_items);
        let mut alloc_program = alloc.program().clone();
        let mut std_program = std.program().clone();
        let mut source_program = program.clone();
        erase_region_parameters(&mut core_program);
        erase_region_parameters(&mut alloc_program);
        erase_region_parameters(&mut std_program);
        erase_region_parameters(&mut source_program);
        for diagnostic in normalize_labeled_type_arguments([
            &mut core_program,
            &mut alloc_program,
            &mut std_program,
            &mut source_program,
        ]) {
            analyzer.error(diagnostic);
        }
        promote_inferred_type_aliases([
            &mut core_program,
            &mut alloc_program,
            &mut std_program,
            &mut source_program,
        ]);
        analyzer.collection.type_aliases =
            collect_type_aliases([&core_program, &alloc_program, &std_program, &source_program]);
        for diagnostic in expand_type_aliases([
            &mut core_program,
            &mut alloc_program,
            &mut std_program,
            &mut source_program,
        ]) {
            analyzer.error(diagnostic);
        }
        normalize_source_call_groups(&mut core_program);
        normalize_source_call_groups(&mut alloc_program);
        normalize_source_call_groups(&mut std_program);
        normalize_source_call_groups(&mut source_program);
        analyzer.collect_items(&core_program, &alloc_program, &std_program, &source_program);
        Ok(analyzer)
    }

    #[cfg(test)]
    fn new(program: &Program) -> Box<Self> {
        Self::try_new(program)
            .expect("the compiler-embedded edition 2026 core bundle must be valid")
    }

    fn lang_item_name(&self, kind: LangItemKind) -> &str {
        self.lang_items.get(kind).canonical_name()
    }

    fn is_lang_item_name(&self, name: &str, kind: LangItemKind) -> bool {
        name == self.lang_item_name(kind)
            || name == kind.source_name()
            || matches!(
                (kind, name),
                (LangItemKind::If, "$lang$if") | (LangItemKind::Match, "$lang$match")
            )
    }

    #[cfg(test)]
    fn analyze(&mut self) -> Option<HirProgram> {
        self.analyze_target(true)
    }

    fn analyze_target(&mut self, require_entry_point: bool) -> Option<HirProgram> {
        self.validate_foreign_declarations();
        for name in self.collection.global_order.clone() {
            self.lower_global(&name);
        }
        let mut function_index = 0;
        while function_index < self.collection.function_order.len() {
            let name = self.collection.function_order[function_index].clone();
            self.lower_function(&name);
            function_index += 1;
        }
        self.evaluate_static_globals();
        self.validate_nominal_layouts();
        self.validate_inferred_api_visibility();
        if require_entry_point {
            self.validate_entry_point();
        }

        if !self.diagnostics.is_empty() {
            return None;
        }

        let mut functions: Vec<_> = self
            .collection
            .function_order
            .iter()
            .filter_map(|name| self.lowering.hir_functions.get(name).cloned())
            .collect();
        functions.extend(self.lowering.lifted_functions.clone());
        let exported_functions = functions
            .iter()
            .filter(|function| {
                !function.name.starts_with("$mono$fn$")
                    && self
                        .collection
                        .function_accesses
                        .get(&function.name)
                        .is_some_and(|access| {
                            access.visibility == Visibility::Public
                                && access.origin.package == self.primary_package
                        })
            })
            .map(|function| {
                (
                    function.name.clone(),
                    exported_function_symbol(self, &function.name, function),
                )
            })
            .collect();
        let exported_globals = self
            .collection
            .global_order
            .iter()
            .filter(|name| {
                self.lowering
                    .hir_globals
                    .get(*name)
                    .is_some_and(|global| global.ty != Ty::Unit)
                    && self
                        .collection
                        .global_accesses
                        .get(*name)
                        .is_some_and(|access| {
                            access.visibility == Visibility::Public
                                && access.origin.package == self.primary_package
                        })
            })
            .map(|name| {
                let global = &self.lowering.hir_globals[name];
                (name.clone(), exported_global_symbol(self, name, &global.ty))
            })
            .collect();
        Some(HirProgram {
            structs: self
                .collection
                .struct_order
                .iter()
                .map(|name| self.collection.struct_layouts[name].clone())
                .collect(),
            enums: self
                .collection
                .enum_order
                .iter()
                .map(|name| self.collection.enum_layouts[name].clone())
                .collect(),
            globals: self
                .collection
                .global_order
                .iter()
                .map(|name| self.lowering.hir_globals[name].clone())
                .collect(),
            normalized_globals: self.lowering.ctfe_global_values.clone(),
            exported_globals,
            functions,
            exported_functions,
            foreign_functions: self
                .collection
                .function_order
                .iter()
                .filter_map(|name| {
                    let function = &self.collection.functions[name];
                    let foreign = function.foreign.as_ref()?;
                    let signature = self.lowering.signatures.get(name)?;
                    Some(HirForeignFunction {
                        name: name.clone(),
                        link_name: foreign.link_name.clone(),
                        params: signature
                            .groups
                            .iter()
                            .flatten()
                            .map(|parameter| parameter.ty.clone())
                            .collect(),
                        result: signature.result.clone()?,
                    })
                })
                .collect(),
            drop_methods: self
                .collection
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
            array_types: self.lowering.array_types.clone(),
            tuple_types: self.lowering.tuple_types.clone(),
            string_literals: self.lowering.string_literals.clone(),
            continuation_adapters: self.lowering.continuation_adapters.clone(),
            effect_callable_adapters: self.lowering.effect_callable_adapters.clone(),
            async_states: self
                .lowering
                .async_futures
                .iter()
                .map(|(name, future)| {
                    (
                        name.clone(),
                        hir::AsyncStateLayout {
                            owned_capture_fields: future
                                .capture_modes
                                .iter()
                                .enumerate()
                                .filter_map(|(index, mode)| {
                                    (*mode == PassMode::Move).then_some(index + 1)
                                })
                                .chain(future.awaited.iter().flat_map(|awaited| {
                                    awaited
                                        .continuation_capture_modes
                                        .iter()
                                        .zip(&awaited.continuation_fields)
                                        .filter_map(|(mode, field)| {
                                            (*mode == PassMode::Move).then_some(*field)
                                        })
                                }))
                                .collect(),
                            starting_fields: future
                                .awaited
                                .iter()
                                .flat_map(|awaited| {
                                    awaited
                                        .continuation_capture_modes
                                        .iter()
                                        .zip(&awaited.continuation_fields)
                                        .filter_map(|(mode, field)| {
                                            (*mode == PassMode::Move).then_some(*field)
                                        })
                                })
                                .collect(),
                            suspended_fields: future
                                .awaited
                                .as_ref()
                                .map(|awaited| {
                                    std::iter::once(awaited.field)
                                        .chain(
                                            awaited
                                                .continuation_capture_modes
                                                .iter()
                                                .zip(&awaited.continuation_fields)
                                                .filter_map(|(mode, field)| {
                                                    (*mode == PassMode::Move).then_some(*field)
                                                }),
                                        )
                                        .chain(
                                            awaited
                                                .retained_modes
                                                .iter()
                                                .zip(&awaited.retained_fields)
                                                .filter_map(|(mode, field)| {
                                                    (*mode == PassMode::Move).then_some(*field)
                                                }),
                                        )
                                        .collect()
                                })
                                .unwrap_or_default(),
                            chained_fields: future
                                .awaited
                                .as_ref()
                                .and_then(|awaited| awaited.next.as_ref())
                                .map(|next| vec![next.field])
                                .unwrap_or_default(),
                        },
                    )
                })
                .collect(),
        })
    }

    fn validate_foreign_declarations(&mut self) {
        let mut links = HashMap::<String, String>::new();
        for name in self.collection.function_order.clone() {
            let Some(foreign) = self.collection.functions[&name].foreign.clone() else {
                continue;
            };
            self.current_origin = self
                .collection
                .function_origins
                .get(&name)
                .cloned()
                .map(Box::new);
            if name == "main" {
                self.error("foreign function `main` cannot be the Salicin entry point");
            }
            if matches!(
                foreign.link_name.as_str(),
                "main" | "salicin_alloc" | "salicin_dealloc"
            ) || foreign.link_name.starts_with("llvm.")
                || foreign.link_name.starts_with("sali.")
            {
                self.error(format!(
                    "foreign function `{name}` uses reserved link symbol `{}`",
                    foreign.link_name
                ));
            }
            if let Some(previous) = links.insert(foreign.link_name.clone(), name.clone()) {
                self.error(format!(
                    "foreign functions `{previous}` and `{name}` use the same link symbol `{}`",
                    foreign.link_name
                ));
            }
        }
        self.current_origin = None;
    }

    fn validate_function_templates(&mut self) {
        for template_name in self.collection.function_template_order.clone() {
            self.current_origin = self
                .collection
                .function_template_origins
                .get(&template_name)
                .cloned()
                .map(Box::new);
            let template = self.collection.function_templates[&template_name].clone();
            if self
                .collection
                .integer_conversion_templates
                .contains_key(&template_name)
            {
                // Every concrete `checked_into` instance is validated against
                // its selected integer target when it is instantiated.
                continue;
            }
            if [LangItemKind::DoWhile, LangItemKind::For]
                .into_iter()
                .any(|kind| {
                    let lang_item = self.lang_item_name(kind);
                    template_name == lang_item
                        || overloaded_function_name(
                            lang_item,
                            &function_parameter_labels(&template),
                        ) == template_name
                })
            {
                // These retain source bodies as portable contracts, while
                // this compiler always takes their syntax-directed fast paths.
                continue;
            }
            if template.builtin
                && [
                    LangItemKind::Do,
                    LangItemKind::Try,
                    LangItemKind::Throw,
                    LangItemKind::Unsafe,
                    LangItemKind::Loop,
                    LangItemKind::If,
                    LangItemKind::Match,
                    LangItemKind::BorrowValueForm,
                    LangItemKind::PtrValueForm,
                    LangItemKind::SizeOf,
                    LangItemKind::AlignOf,
                    LangItemKind::AsyncFunction,
                ]
                .into_iter()
                .any(|kind| {
                    let lang_item_name = self.lang_item_name(kind);
                    lang_item_name == template_name
                        || overloaded_function_name(
                            lang_item_name,
                            &function_parameter_labels(&template),
                        ) == template_name
                })
            {
                continue;
            }
            if template.builtin && template_name == "core::control::defer" {
                // `defer` is a compiler-provided lexical cleanup contract.
                continue;
            }
            if template.body.is_none() && template_name.starts_with("$trait$impl$") {
                continue;
            }
            if template.return_type.is_none() {
                self.error(format!(
                    "generic function `{template_name}` requires an explicit return type"
                ));
                continue;
            }
            let compile_parameter_sorts = compile_parameter_sorts(&template.compile_groups);
            if !self.validate_where_predicate_shapes(
                &format!("generic function `{template_name}`"),
                &template.where_predicates,
                &compile_parameter_sorts,
            ) {
                continue;
            }
            if template.compile_groups.iter().flatten().any(|parameter| {
                matches!(
                    parameter.kind,
                    Sort::TypeConstructor { .. } | Sort::EffectConstructor { .. }
                )
            }) {
                continue;
            }

            let mut substitutions = HashMap::new();
            for (index, parameter) in template.compile_groups.iter().flatten().enumerate() {
                if parameter.kind == Sort::USize {
                    substitutions.insert(parameter.name.clone(), Type::CompileUSize(0));
                    continue;
                }
                let marker = match parameter.kind.clone() {
                    Sort::Universe(_) => continue,
                    Sort::Type => {
                        let marker =
                            generic_parameter_marker(&template_name, index, &parameter.name);
                        self.collection
                            .abstract_type_parameters
                            .insert(marker.clone(), parameter.name.clone());
                        marker
                    }
                    // Abstract validation uses the maximal currently supported row. Every
                    // concrete instance is lowered again after substituting its selected row.
                    Sort::Effect | Sort::Effects => EFFECT_UNSAFE_MARKER.to_owned(),
                    Sort::Parameters => continue,
                    Sort::ParameterPack => continue,
                    Sort::ParameterModifier => PARAMETER_MODIFIER_MOVE_MARKER.to_owned(),
                    Sort::Region => continue,
                    Sort::USize => unreachable!("handled before marker selection"),
                    Sort::TypeConstructor { .. } | Sort::EffectConstructor { .. } => unreachable!(
                        "constructor parameters are validated through concrete instances"
                    ),
                    Sort::Named(compile_type) => {
                        let Some(member) = self
                            .collection
                            .closed_type_values
                            .get(&compile_type)
                            .and_then(|members| members.first())
                        else {
                            self.error(format!(
                                "compile-time parameter `{}` uses unknown or empty closed type `{compile_type}`",
                                parameter.name
                            ));
                            continue;
                        };
                        closed_value_marker(&compile_type, member)
                    }
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

            let functions_before = self.collection.functions.clone();
            let function_origins_before = self.collection.function_origins.clone();
            let function_accesses_before = self.collection.function_accesses.clone();
            let function_order_before = self.collection.function_order.clone();
            let signatures_before = self.lowering.signatures.clone();
            let function_states_before = self.lowering.function_states.clone();
            let hir_functions_before = self.lowering.hir_functions.clone();
            let global_states_before = self.lowering.global_states.clone();
            let hir_globals_before = self.lowering.hir_globals.clone();
            let nominals_before = self.snapshot_nominals();
            let instance_names_before = self.collection.function_instance_names.clone();
            let instances_before = self.collection.function_instances.clone();
            let type_substitutions_before = self.collection.function_type_substitutions.clone();
            let lifted_functions_before = self.lowering.lifted_functions.clone();
            let handler_frame_parameter_modes_before =
                self.lowering.handler_frame_parameter_modes.clone();
            let continuation_adapters_before = self.lowering.continuation_adapters.clone();
            let effect_callable_adapters_before = self.lowering.effect_callable_adapters.clone();
            let callable_bridge_specializations_before =
                self.lowering.callable_bridge_specializations.clone();
            let next_closure = self.lowering.next_closure;
            let inherent_members_before = self.collection.inherent_members.clone();
            let instantiated_pointer_extensions_before =
                self.collection.instantiated_pointer_extensions.clone();
            let instantiated_slice_extensions_before =
                self.collection.instantiated_slice_extensions.clone();
            let copy_nominals_before = self.collection.copy_nominals.clone();
            let trait_impl_headers_before = self.collection.trait_impl_headers.clone();
            let trait_impls_before = self.collection.trait_impls.clone();
            let trait_methods_before = self.collection.trait_methods_by_receiver.clone();

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
                    if binding.compile_groups.is_empty() {
                        self.lower_source_type(&binding.ty);
                    }
                }
                if matches!(&predicate.trait_ref, Type::Named(name, arguments)
                    if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty())
                    && subject != Ty::Error
                {
                    self.collection.copy_nominals.insert(subject);
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
            let unsafety = self.function_effects_unsafe(&function.effects);
            let failure_error = function
                .effects
                .failure
                .as_deref()
                .map(|error| self.lower_source_type(error));
            let custom_effects = self.function_effects_custom_identities(&function.effects);
            self.collection
                .functions
                .insert(validation_name.clone(), function);
            self.collection.function_origins.insert(
                validation_name.clone(),
                self.collection.function_template_origins[&template_name].clone(),
            );
            self.lowering.signatures.insert(
                validation_name.clone(),
                FunctionSig {
                    groups,
                    unsafety,
                    failure_error,
                    custom_effects,
                    result,
                },
            );
            self.collection
                .function_type_substitutions
                .insert(validation_name.clone(), substitutions);
            self.lower_function(&validation_name);
            self.collection.functions = functions_before;
            self.collection.function_origins = function_origins_before;
            self.collection.function_accesses = function_accesses_before;
            self.collection.function_order = function_order_before;
            self.lowering.signatures = signatures_before;
            self.lowering.function_states = function_states_before;
            self.lowering.hir_functions = hir_functions_before;
            self.lowering.global_states = global_states_before;
            self.lowering.hir_globals = hir_globals_before;
            self.restore_nominals(nominals_before);
            self.collection.function_instance_names = instance_names_before;
            self.collection.function_instances = instances_before;
            self.collection.function_type_substitutions = type_substitutions_before;
            self.lowering.lifted_functions = lifted_functions_before;
            self.lowering.handler_frame_parameter_modes = handler_frame_parameter_modes_before;
            self.lowering.continuation_adapters = continuation_adapters_before;
            self.lowering.effect_callable_adapters = effect_callable_adapters_before;
            self.lowering.callable_bridge_specializations = callable_bridge_specializations_before;
            self.lowering.next_closure = next_closure;
            self.collection.inherent_members = inherent_members_before;
            self.collection.instantiated_pointer_extensions =
                instantiated_pointer_extensions_before;
            self.collection.instantiated_slice_extensions = instantiated_slice_extensions_before;
            self.collection.copy_nominals = copy_nominals_before;
            self.collection.trait_impl_headers = trait_impl_headers_before;
            self.collection.trait_impls = trait_impls_before;
            self.collection.trait_methods_by_receiver = trait_methods_before;
        }
        self.current_origin = None;
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
            let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
                continue;
            };
            let self_ty = self.lower_source_type(&predicate.subject);
            let arguments = source_arguments
                .iter()
                .map(|argument| self.lower_source_type(argument))
                .collect::<Vec<_>>();
            let mut equations = HashMap::new();
            let mut associated_types = HashMap::new();
            let mut associated_type_sources = HashMap::new();
            for binding in &predicate.associated_types {
                let Some((parameters, source)) =
                    self.normalized_associated_type_equation(&schema, binding)
                else {
                    continue;
                };
                if parameters.is_empty() {
                    associated_types.insert(binding.name.clone(), self.lower_source_type(&source));
                }
                associated_type_sources.insert(binding.name.clone(), source.clone());
                equations.insert(binding.name.clone(), (parameters, source));
            }
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
            self.collection.trait_impl_headers.insert(key.clone());
            if self.collection.trait_impls.contains_key(&key) {
                continue;
            }

            let mut substitutions = HashMap::new();
            substitutions.insert("self".to_owned(), predicate.subject.clone());
            for (parameter, argument) in schema.compile_parameters.iter().zip(source_arguments) {
                substitutions.insert(parameter.name.clone(), argument.clone());
            }
            for binding in &predicate.associated_types {
                if binding.compile_groups.is_empty() {
                    substitutions.insert(binding.name.clone(), binding.ty.clone());
                }
            }
            let mut methods = HashMap::new();
            let associated_types_complete = schema
                .associated_types
                .iter()
                .all(|name| associated_type_sources.contains_key(name));
            for method_id in schema
                .method_order
                .iter()
                .filter(|_| associated_types_complete)
            {
                let declaration = &schema.methods[method_id];
                let mut method = declaration.clone();
                substitute_function_types(&mut method, &substitutions);
                let mut method_substitutions = HashMap::new();
                for (index, parameter) in method.compile_groups.iter().flatten().enumerate() {
                    let value = match parameter.kind.clone() {
                        Sort::Universe(_) => continue,
                        Sort::Type => {
                            let marker = generic_parameter_marker(
                                &format!("{function}${method_id}"),
                                index,
                                &parameter.name,
                            );
                            self.collection
                                .abstract_type_parameters
                                .insert(marker.clone(), parameter.name.clone());
                            Type::Named(marker, Vec::new())
                        }
                        Sort::USize => Type::CompileUSize(0),
                        Sort::Effect | Sort::Effects => {
                            Type::Named(EFFECT_UNSAFE_MARKER.to_owned(), Vec::new())
                        }
                        Sort::ParameterModifier => {
                            Type::Named(PARAMETER_MODIFIER_MOVE_MARKER.to_owned(), Vec::new())
                        }
                        Sort::Named(compile_type) => {
                            let Some(member) = self
                                .collection
                                .closed_type_values
                                .get(&compile_type)
                                .and_then(|members| members.first())
                            else {
                                continue;
                            };
                            Type::Named(closed_value_marker(&compile_type, member), Vec::new())
                        }
                        Sort::Region
                        | Sort::Parameters
                        | Sort::ParameterPack
                        | Sort::TypeConstructor { .. }
                        | Sort::EffectConstructor { .. } => continue,
                    };
                    method_substitutions.insert(parameter.name.clone(), value);
                }
                substitute_function_types(&mut method, &method_substitutions);
                method.compile_groups.clear();
                if let Err(diagnostic) =
                    substitute_associated_type_equations(&mut method, &equations)
                {
                    self.error(format!(
                        "invalid associated type equation in where predicate of `{function}`: {diagnostic}"
                    ));
                    continue;
                }
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
                let failure_error = method
                    .effects
                    .failure
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.lowering.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafety: self.function_effects_unsafe(&method.effects),
                        failure_error,
                        custom_effects: self.function_effects_custom_identities(&method.effects),
                        result,
                    },
                );
                methods.insert(method_id.clone(), canonical);
                if schema_function_has_receiver(declaration) {
                    let candidates = self
                        .collection
                        .trait_methods_by_receiver
                        .entry((key.self_ty.clone(), declaration.name.clone()))
                        .or_default();
                    if !candidates.contains(&key) {
                        candidates.push(key.clone());
                    }
                }
            }
            self.collection.trait_impls.insert(
                key.clone(),
                TraitImplInfo {
                    key,
                    associated_types,
                    associated_type_sources,
                    methods,
                    access: schema.access,
                },
            );
        }
    }

    fn normalized_associated_type_equation(
        &self,
        schema: &TraitSchema,
        binding: &crate::ast::AssociatedTypeBinding,
    ) -> Option<(Vec<CompileParam>, Type)> {
        let expected = schema
            .associated_type_parameters
            .get(&binding.name)
            .cloned()
            .unwrap_or_default();
        let actual = binding
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if expected.len() != actual.len()
            || expected
                .iter()
                .zip(&actual)
                .any(|(expected, actual)| expected.kind != actual.kind)
        {
            return None;
        }
        let substitutions = actual
            .iter()
            .zip(&expected)
            .map(|(actual, expected)| {
                (
                    actual.name.clone(),
                    Type::Named(expected.name.clone(), Vec::new()),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut source = binding.ty.clone();
        substitute_type_parameters(&mut source, &substitutions);
        Some((expected, source))
    }

    fn validate_where_predicate_shapes(
        &mut self,
        owner: &str,
        predicates: &[crate::ast::WherePredicate],
        _compile_parameter_sorts: &HashMap<String, Sort>,
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
            let Some(schema) = self.collection.traits.get(name).cloned() else {
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
                } else if !associated.insert(binding.name.clone()) {
                    self.error(format!(
                        "duplicate associated type equality `{name}.{}` in where predicate of {owner}",
                        binding.name
                    ));
                    valid = false;
                } else {
                    let expected_groups = schema
                        .associated_type_parameter_groups
                        .get(&binding.name)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    if expected_groups.len() != binding.compile_groups.len()
                        || expected_groups.iter().zip(&binding.compile_groups).any(
                            |(expected, actual)| {
                                expected.len() != actual.len()
                                    || expected
                                        .iter()
                                        .zip(actual)
                                        .any(|(expected, actual)| expected.kind != actual.kind)
                            },
                        )
                    {
                        self.error(format!(
                            "associated type equality `{name}.{}` in where predicate of {owner} has a parameter-group shape that does not match its declaration",
                            binding.name
                        ));
                        valid = false;
                    }
                    let mut parameter_names = HashSet::new();
                    for parameter in binding.compile_groups.iter().flatten() {
                        if parameter.default.is_some() {
                            self.error(format!(
                                "associated type equality `{name}.{}` parameter `{}` cannot have a default",
                                binding.name, parameter.name
                            ));
                            valid = false;
                        }
                        if !parameter_names.insert(parameter.name.clone()) {
                            self.error(format!(
                                "duplicate parameter `{}` in associated type equality `{name}.{}`",
                                parameter.name, binding.name
                            ));
                            valid = false;
                        }
                    }
                }
            }
        }
        valid
    }

    fn validate_trait_inheritance_implementations(&mut self) {
        let trait_impl_headers = self
            .collection
            .trait_impl_headers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in trait_impl_headers {
            let Some(schema) = self.collection.traits.get(&key.trait_ref.name).cloned() else {
                continue;
            };
            let Some(predicates) = self.substituted_trait_where_predicates(&schema, &key) else {
                continue;
            };
            for predicate in predicates {
                if let Some(required) = self.constructor_trait_impl_key_from_predicate(&predicate) {
                    if !self
                        .collection
                        .constructor_trait_impl_headers
                        .contains(&required)
                    {
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
            .collection
            .constructor_trait_impl_headers
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in constructor_headers {
            let Some(schema) = self.collection.traits.get(&key.trait_ref.name).cloned() else {
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
                    if !self
                        .collection
                        .constructor_trait_impl_headers
                        .contains(&required)
                    {
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
        substitutions.insert("self".to_owned(), self.source_type_for_ty(&key.self_ty)?);
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
            "self".to_owned(),
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
        let schema = self.collection.traits.get(trait_name).cloned()?;
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
        let function = self.diagnostic_function_name(function);
        let mut valid = true;
        for predicate in predicates {
            let goal = Goal::new([], Constraint::from(predicate));
            if self.solve_concrete_goal(&goal) == GoalResult::Proven {
                continue;
            }
            let Type::Named(name, _) = &predicate.trait_ref else {
                valid = false;
                continue;
            };
            self.error(format!(
                "where predicate `{}: {}` is not satisfied while instantiating `{function}`",
                source_effect_identity(&predicate.subject),
                name
            ));
            valid = false;
        }
        valid
    }

    fn concrete_where_predicate_holds(&mut self, predicate: &crate::ast::WherePredicate) -> bool {
        self.solve_concrete_goal(&Goal::new([], Constraint::from(predicate))) == GoalResult::Proven
    }

    fn solve_concrete_goal(&mut self, goal: &Goal) -> GoalResult {
        if goal.assumptions.contains(&goal.conclusion) {
            return GoalResult::Proven;
        }
        let Constraint::Implements {
            subject,
            trait_ref,
            projections,
        } = &goal.conclusion
        else {
            return GoalResult::NoSolution;
        };
        let predicate = WherePredicate {
            subject: subject.clone(),
            trait_ref: trait_ref.clone(),
            associated_types: projections.iter().map(Into::into).collect(),
        };
        if let Some(required) = self.constructor_trait_impl_key_from_predicate(&predicate) {
            return if self
                .collection
                .constructor_trait_impl_headers
                .contains(&required)
            {
                GoalResult::Proven
            } else {
                GoalResult::NoSolution
            };
        }
        let subject = self.lower_source_type(subject);
        let Type::Named(name, source_arguments) = trait_ref else {
            return GoalResult::NoSolution;
        };
        let arguments = source_arguments
            .iter()
            .map(|argument| self.lower_source_type(argument))
            .collect::<Vec<_>>();
        let associated_types = projections
            .iter()
            .filter(|binding| binding.parameter_groups.is_empty())
            .map(|binding| (binding.name.clone(), self.lower_source_type(&binding.value)))
            .collect::<HashMap<_, _>>();
        if subject == Ty::Error
            || arguments.contains(&Ty::Error)
            || associated_types.values().any(|ty| *ty == Ty::Error)
        {
            return GoalResult::NoSolution;
        }
        if name == self.lang_item_name(LangItemKind::Move) && arguments.is_empty() {
            return if self.is_move_type(&subject) {
                GoalResult::Proven
            } else {
                GoalResult::NoSolution
            };
        }
        if name == self.lang_item_name(LangItemKind::Copy) && arguments.is_empty() {
            return if self.is_copy_type(&subject) {
                GoalResult::Proven
            } else {
                GoalResult::NoSolution
            };
        }
        let Some(schema) = self.collection.traits.get(name).cloned() else {
            return GoalResult::NoSolution;
        };
        if self
            .collection
            .trait_impls
            .get(&TraitImplKey {
                self_ty: subject,
                trait_ref: TraitRefKey {
                    name: name.clone(),
                    arguments,
                },
            })
            .cloned()
            .is_some_and(|implementation| {
                associated_types.iter().all(|(name, expected)| {
                    implementation.associated_types.get(name) == Some(expected)
                }) && projections.iter().all(|equation| {
                    self.concrete_associated_equation_holds(
                        &implementation,
                        &schema,
                        &equation.into(),
                    )
                })
            })
        {
            GoalResult::Proven
        } else {
            GoalResult::NoSolution
        }
    }

    fn concrete_associated_equation_holds(
        &mut self,
        implementation: &TraitImplInfo,
        schema: &TraitSchema,
        binding: &crate::ast::AssociatedTypeBinding,
    ) -> bool {
        if binding.compile_groups.is_empty() {
            return true;
        }
        let Some((parameters, mut expected)) =
            self.normalized_associated_type_equation(schema, binding)
        else {
            return false;
        };
        let Some(mut actual) = implementation
            .associated_type_sources
            .get(&binding.name)
            .cloned()
        else {
            return false;
        };
        let Type::Named(_, arguments) = &mut actual else {
            return false;
        };
        arguments.extend(
            parameters
                .iter()
                .map(|parameter| Type::Named(parameter.name.clone(), Vec::new())),
        );
        let mut diagnostics = Vec::new();
        expand_alias_type(
            &mut actual,
            &self.collection.type_aliases,
            &mut Vec::new(),
            &mut diagnostics,
        );
        expand_alias_type(
            &mut expected,
            &self.collection.type_aliases,
            &mut Vec::new(),
            &mut diagnostics,
        );
        diagnostics.is_empty() && actual == expected
    }

    fn error(&mut self, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::at_origin(
            message,
            self.current_origin.as_deref().cloned(),
        ));
    }
}

fn remove_nonruntime_syntax_contracts(program: &mut Program, lang_items: &LangItems) {
    let names = [
        LangItemKind::Builtin,
        LangItemKind::Foreign,
        LangItemKind::Test,
    ]
    .map(|kind| lang_items.get(kind).canonical_name());
    let introspection = [
        "core::sorts::sort",
        "core::sorts::sort_of",
        "core::sorts::type_of",
    ];
    let mut items = Vec::with_capacity(program.items.len().saturating_sub(names.len()));
    let mut visibilities =
        Vec::with_capacity(program.item_visibilities.len().saturating_sub(names.len()));
    let mut origins = Vec::with_capacity(program.item_origins.len().saturating_sub(names.len()));
    for ((item, visibility), origin) in program
        .items
        .drain(..)
        .zip(program.item_visibilities.drain(..))
        .zip(program.item_origins.drain(..))
    {
        let name = match &item {
            Item::Function(function) => Some(function.name.as_str()),
            _ => None,
        };
        if name.is_some_and(|name| names.contains(&name) || introspection.contains(&name)) {
            continue;
        }
        items.push(item);
        visibilities.push(visibility);
        origins.push(origin);
    }
    program.items = items;
    program.item_visibilities = visibilities;
    program.item_origins = origins;
}

fn compile_parameter_sort_label(kind: &Sort) -> String {
    match kind {
        Sort::Universe(level) => match level {
            crate::ast::SortLevel::Literal(level) => format!("sort({level})"),
            crate::ast::SortLevel::Parameter(level) => format!("sort({level})"),
        },
        Sort::Type => "type".to_owned(),
        Sort::Region => "region".to_owned(),
        Sort::USize => "usize".to_owned(),
        Sort::Effect => "effect".to_owned(),
        Sort::Effects => "effects".to_owned(),
        Sort::Parameters => "parameters".to_owned(),
        Sort::ParameterPack => "parameter pack".to_owned(),
        Sort::ParameterModifier => "parameter modifier".to_owned(),
        Sort::TypeConstructor { parameter_groups } => {
            describe_compile_sort(Sort::TypeConstructor {
                parameter_groups: parameter_groups.clone(),
            })
        }
        Sort::EffectConstructor { parameter_groups } => {
            describe_compile_sort(Sort::EffectConstructor {
                parameter_groups: parameter_groups.clone(),
            })
        }
        Sort::Named(name) => name.clone(),
    }
}

#[cfg(test)]
mod tests;
