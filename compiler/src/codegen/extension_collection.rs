use std::collections::{HashMap, HashSet};

use crate::ast::{CompileParam, Expr, ExtendDef, ExtendMember, ItemOrigin, Sort, Type, Visibility};
use crate::core::LangItemKind;
use crate::modules::PackageId;

use super::compile_time::{
    self, closed_value_marker, compile_parameter_sorts, describe_compile_sort,
};
use super::hir::{AccessBoundary, FunctionSig, ParamSig, Ty};
use super::lower::nominal_name;
use super::names::{
    associated_constant_name, associated_function_name, constructor_trait_method_name,
    generic_inherent_function_name, hex_name, inherent_method_name, trait_method_name,
};
use super::registry::{
    display_parameter_label_shape, function_parameter_labels,
    generic_trait_pattern_overlaps_concrete, overloaded_function_name,
    schema_function_has_receiver, substitute_self_type, trait_method_identity,
    trait_reference_patterns_overlap, ArrayInherentExtension, ArrayTraitExtension,
    ConstructorTraitImplKey, ConstructorTraitRefKey, GenericInherentExtension,
    GenericTraitExtension, NominalKind, PointerInherentExtension, SliceInherentExtension,
    SliceTraitExtension, TraitImplInfo, TraitImplKey, TraitRefKey, TraitSchema,
    TypeConstructorImplTarget,
};
use super::source_rewrite::{
    substitute_expr_types, substitute_function_types, substitute_self_expression_target,
    substitute_type_expression_parameters, substitute_type_parameters, substitute_where_predicate,
};
use super::{
    generic_method_contracts_match, method_compile_parameter_groups_match, primitive_scalar_type,
    Analyzer,
};

impl Analyzer {
    pub(super) fn collect_trait_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
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
        let array_intrinsic_target = matches!(
            &target_source,
            Type::Array(_, _) | Type::ArrayApplication { .. }
        );
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
        let target_nominal = nominal_name(&target);
        if (is_copy || is_drop)
            && target_nominal
                .and_then(|name| self.collection.nominal_accesses.get(name))
                .is_some_and(|access| access.origin.package != origin.package)
        {
            let target = self.diagnostic_type_name(&target);
            let trait_name = if is_copy { "copyable" } else { "droppable" };
            self.error(format!(
                "`{trait_name}` for `{target}` must be implemented in the package that defines the type"
            ));
            return;
        }
        let target_package = target_nominal
            .and_then(|name| self.collection.nominal_accesses.get(name))
            .map(|access| access.origin.package)
            .or_else(|| primitive_scalar_type(&target).then_some(PackageId::CORE.0));
        if target_package != Some(origin.package) && schema.access.origin.package != origin.package
        {
            let target = self.diagnostic_type_name(&target);
            self.error(format!(
                "trait implementation of `{}` for `{target}` must be declared in the package that defines the trait or the type",
                key.trait_ref.name
            ));
            return;
        }
        if is_drop && self.collection.copy_nominals.contains(&target) {
            let target = self.diagnostic_type_name(&target);
            self.error(format!(
                "`{target}` cannot implement both `copyable` and `droppable`"
            ));
            return;
        }
        if self.collection.instantiating_generic_trait_extension == 0
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
        if !self.collection.trait_impl_headers.insert(key.clone()) {
            self.error(format!(
                "duplicate trait implementation of `{}` for `{target}`",
                key.trait_ref.name
            ));
            return;
        }
        substitutions.insert("self".to_owned(), target_source);

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
            match schema.associated_type_kinds[associated].clone() {
                Sort::Type => {
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
                Sort::TypeConstructor { .. } => {}
                Sort::Parameters => {
                    self.error(format!(
                        "associated parameter schema `{}.{associated}` implementations are compiler-derived only",
                        key.trait_ref.name
                    ));
                    valid = false;
                }
                Sort::ParameterPack => {
                    unreachable!("associated types cannot be parameter-group packs")
                }
                Sort::ParameterModifier => {
                    unreachable!("associated types cannot be parameter modifiers")
                }
                Sort::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{}.{associated}` implementations are not supported yet",
                        key.trait_ref.name
                    ));
                    valid = false;
                }
                Sort::Universe(_)
                | Sort::Region
                | Sort::USize
                | Sort::Effect
                | Sort::Effects
                | Sort::Named(_) => {
                    unreachable!("associated types only store type sorts")
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
            let Sort::TypeConstructor { .. } = schema.associated_type_kinds[associated].clone()
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
                &schema.associated_type_parameters[associated],
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
            if let Some(referenced) = self.collection.nominal_accesses.get(constructor) {
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
            let primitive_intrinsic = function.builtin
                && ((origin.package == PackageId::CORE.0 && primitive_scalar_type(&target))
                    || (function_origin.package == PackageId::CORE.0
                        && (method_name == "index"
                            || matches!(
                                key.trait_ref.name.as_str(),
                                "core::literal::array_literal" | "core::literal::string_literal"
                            ))));
            if function.body.is_none() && !primitive_intrinsic {
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
            let has_receiver = schema_function_has_receiver(&function);
            if let Some(body) = &mut function.body {
                let mut body_substitutions = substitutions.clone();
                if has_receiver {
                    body_substitutions.remove("self");
                }
                substitute_type_expression_parameters(body, &body_substitutions);
                if !has_receiver {
                    if let Some(target_name) = nominal_name(&target) {
                        substitute_self_expression_target(body, target_name);
                    }
                }
            }
            if !method_compile_parameter_groups_match(&expected, &function) {
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
            } else if !generic_method_contracts_match(&expected, &function) {
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
            let primitive_intrinsic = function.builtin
                && (primitive_scalar_type(&target)
                    || array_intrinsic_target
                    || self.collection.instantiating_array_trait_extension > 0
                    || function_origin.package == PackageId::CORE.0);
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
                let failure_error = function
                    .effects
                    .failure
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.lowering.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafety: self.function_effects_unsafe(&function.effects),
                        failure_error,
                        custom_effects: self.function_effects_custom_identities(&function.effects),
                        result,
                    },
                );
                if !primitive_intrinsic {
                    self.collection.function_order.push(canonical.clone());
                    self.collection
                        .functions
                        .insert(canonical.clone(), function);
                    self.collection
                        .function_origins
                        .insert(canonical.clone(), function_origin);
                }
            } else {
                self.collection
                    .function_template_order
                    .push(canonical.clone());
                self.collection
                    .function_templates
                    .insert(canonical.clone(), function);
                self.collection
                    .function_template_origins
                    .insert(canonical.clone(), function_origin);
            }
            self.collection
                .function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            if !primitive_intrinsic {
                self.collection
                    .function_type_substitutions
                    .insert(canonical.clone(), substitutions.clone());
            }
            methods.insert(method_id.clone(), canonical);
            let declaration = &schema.methods[&method_id];
            if schema_function_has_receiver(declaration) {
                let candidates = self
                    .collection
                    .trait_methods_by_receiver
                    .entry((target.clone(), declaration.name.clone()))
                    .or_default();
                if !candidates.contains(&key) {
                    candidates.push(key.clone());
                }
            }
        }
        self.collection.trait_impls.insert(
            key.clone(),
            TraitImplInfo {
                key: key.clone(),
                associated_types,
                associated_type_sources,
                methods,
                access: implementation_access,
            },
        );
        if is_copy && self.collection.copy_impls_finalized {
            self.validate_dynamic_copy_implementation(&key);
        }
    }

    pub(super) fn collect_constructor_trait_extension(
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
        let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid {
            return;
        }
        let Sort::TypeConstructor {
            ref parameter_groups,
        } = schema.self_parameter.kind
        else {
            self.error(format!(
                "trait `{trait_name}` does not accept a type-constructor implementation target"
            ));
            return;
        };
        if *parameter_groups != target.parameter_groups {
            self.error(format!(
                "type constructor `{}` has sort {}, but trait `{trait_name}` expects sort {}",
                target.name,
                describe_compile_sort(Sort::TypeConstructor {
                    parameter_groups: target.parameter_groups.clone(),
                }),
                describe_compile_sort(Sort::TypeConstructor {
                    parameter_groups: parameter_groups.clone(),
                }),
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
            if parameter.kind != Sort::Type {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` has unsupported compile-time sort {}",
                    parameter.name,
                    describe_compile_sort(parameter.kind.clone())
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
            .collection
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
        if !self
            .collection
            .constructor_trait_impl_headers
            .insert(key.clone())
        {
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
            "self".to_owned(),
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
                || !method_compile_parameter_groups_match(&expected, &function)
                || !generic_method_contracts_match(&expected, &function)
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
                let failure_error = function
                    .effects
                    .failure
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.lowering.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafety: self.function_effects_unsafe(&function.effects),
                        failure_error,
                        custom_effects: self.function_effects_custom_identities(&function.effects),
                        result,
                    },
                );
                self.collection.function_order.push(canonical.clone());
                self.collection
                    .functions
                    .insert(canonical.clone(), function);
                self.collection
                    .function_origins
                    .insert(canonical.clone(), function_origin);
            } else {
                self.collection
                    .function_template_order
                    .push(canonical.clone());
                self.collection
                    .function_templates
                    .insert(canonical.clone(), function);
                self.collection
                    .function_template_origins
                    .insert(canonical.clone(), function_origin);
            }
            self.collection
                .function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            registered_methods.insert(method_id.clone(), canonical);
        }
        if !valid {
            return;
        }
        self.collection
            .constructor_trait_impl_methods
            .insert(key, registered_methods);
    }

    pub(super) fn collect_extension(&mut self, extension: ExtendDef, origin: ItemOrigin) {
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
        let target_source = extension.target.clone();
        let probed_target = self.probe_source_ty(&target_source);
        let primitive_target = probed_target.clone().filter(primitive_scalar_type);
        let str_target = probed_target.as_ref() == Some(&Ty::Str);
        let target = match extension.target {
            Type::Named(name, arguments) if arguments.is_empty() => name,
            Type::Named(name, _) => {
                self.error(format!(
                    "generic extend target `{name}` is not supported in M1"
                ));
                return;
            }
            _ if primitive_target.is_some() => primitive_target
                .as_ref()
                .expect("primitive target was detected")
                .to_string(),
            _ => {
                self.error("extend target must be a non-generic nominal type in M1");
                return;
            }
        };
        if (primitive_target.is_some() || str_target) && origin.package != PackageId::CORE.0 {
            self.error(format!(
                "inherent extension for built-in `{target}` must be declared in core"
            ));
            return;
        }
        if self.collection.struct_templates.contains_key(&target)
            || self.collection.enum_templates.contains_key(&target)
        {
            self.error(format!(
                "generic extend target `{target}` is not supported in the first generic slice"
            ));
            return;
        }
        if primitive_target.is_none()
            && !str_target
            && !self.collection.struct_defs.contains_key(&target)
            && !self.collection.enum_defs.contains_key(&target)
        {
            self.error(format!("unknown extension target `{target}`"));
            return;
        }
        if primitive_target.is_none()
            && !str_target
            && self
                .collection
                .nominal_accesses
                .get(&target)
                .is_some_and(|access| access.origin.package != origin.package)
        {
            self.error(format!(
                "inherent extension for `{target}` must be declared in the package that defines the type"
            ));
            return;
        }
        let mut member_access = (primitive_target.is_some() || str_target)
            .then_some(())
            .map_or_else(
                || self.nominal_access_or_internal(&target),
                |_| AccessBoundary {
                    visibility: Visibility::Public,
                    origin: origin.clone(),
                },
            );
        let target_ty = if let Some(target) = primitive_target {
            target
        } else if str_target {
            Ty::Str
        } else if self.collection.struct_defs.contains_key(&target) {
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
                        .collection
                        .inherent_overload_counts
                        .get(&overload_key)
                        .cloned()
                        .unwrap_or_default()
                        > 1;
                    if is_method
                        && self
                            .collection
                            .struct_layouts
                            .get(&target)
                            .is_some_and(|layout| {
                                layout.fields.iter().any(|field| field.name == short_name)
                            })
                    {
                        self.error(format!(
                            "inherent method `{target}.{short_name}` conflicts with field `{short_name}`"
                        ));
                        continue;
                    }
                    if !is_method
                        && self
                            .collection
                            .enum_layouts
                            .get(&target)
                            .is_some_and(|layout| {
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
                        let members = self
                            .collection
                            .inherent_members
                            .entry(target.clone())
                            .or_default();
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
                            .collection
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
                    self_substitution.insert("self".to_owned(), target_source.clone());
                    substitute_function_types(&mut function, &self_substitution);
                    if !is_method {
                        if let Some(body) = &mut function.body {
                            substitute_self_expression_target(body, &target);
                        }
                    }
                    let mut canonical = if is_method {
                        inherent_method_name(&target, &short_name)
                    } else {
                        associated_function_name(&target, &short_name)
                    };
                    if let Some(shape) = &overload_shape {
                        canonical = overloaded_function_name(&canonical, shape);
                        self.collection
                            .inherent_overloads
                            .entry(overload_key)
                            .or_default()
                            .push(canonical.clone());
                    }
                    function.name = canonical.clone();
                    let checked_integer_conversion =
                        target_ty.is_integer() && short_name == "checked_into" && function.builtin;
                    let integer_magnitude =
                        target_ty.is_signed() && short_name == "magnitude" && function.builtin;
                    if generic_member {
                        if checked_integer_conversion {
                            self.collection
                                .integer_conversion_templates
                                .insert(canonical.clone(), target_ty.clone());
                        }
                        self.collection
                            .function_template_order
                            .push(canonical.clone());
                        self.collection
                            .function_templates
                            .insert(canonical.clone(), function);
                        self.collection
                            .function_template_origins
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
                        if integer_magnitude {
                            let Some(result) = result.as_ref().filter(|result| {
                                result.is_integer()
                                    && !result.is_signed()
                                    && super::target::NATIVE_TARGET.integer_width(result)
                                        == super::target::NATIVE_TARGET.integer_width(&target_ty)
                            }) else {
                                self.error(format!(
                                    "primitive `{target}.magnitude` must return the same-width unsigned integer"
                                ));
                                continue;
                            };
                            self.collection
                                .integer_magnitude_intrinsics
                                .insert(canonical.clone(), (target_ty.clone(), result.clone()));
                        }
                        let failure_error = function
                            .effects
                            .failure
                            .as_deref()
                            .map(|error| self.lower_source_type(error));
                        self.lowering.signatures.insert(
                            canonical.clone(),
                            FunctionSig {
                                groups,
                                unsafety: self.function_effects_unsafe(&function.effects),
                                failure_error,
                                custom_effects: self
                                    .function_effects_custom_identities(&function.effects),
                                result,
                            },
                        );
                        if !integer_magnitude {
                            self.collection.function_order.push(canonical.clone());
                        }
                        self.collection
                            .functions
                            .insert(canonical.clone(), function);
                        self.collection
                            .function_origins
                            .insert(canonical.clone(), origin.clone());
                    }
                    self.collection
                        .function_accesses
                        .insert(canonical.clone(), member_access.clone());
                    let members = self
                        .collection
                        .inherent_members
                        .entry(target.clone())
                        .or_default();
                    if is_method {
                        members.methods.entry(short_name).or_insert(canonical);
                    } else {
                        members.functions.entry(short_name).or_insert(canonical);
                    }
                }
                ExtendMember::Const(mut binding) => {
                    let short_name = binding.name.clone();
                    if self
                        .collection
                        .enum_layouts
                        .get(&target)
                        .is_some_and(|layout| {
                            layout
                                .variants
                                .iter()
                                .any(|variant| variant.name == short_name)
                        })
                    {
                        self.error(format!(
                            "associated constant `{target}.{short_name}` conflicts with variant `{short_name}`"
                        ));
                        continue;
                    }
                    let duplicate = self
                        .collection
                        .inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .contains_key(&short_name)
                        || self
                            .collection
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
                    self.collection.global_order.push(canonical.clone());
                    self.collection.globals.insert(canonical.clone(), binding);
                    self.collection
                        .global_origins
                        .insert(canonical.clone(), origin.clone());
                    self.collection
                        .global_accesses
                        .insert(canonical.clone(), member_access.clone());
                    self.collection
                        .inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .insert(short_name, canonical);
                }
            }
        }
    }

    pub(super) fn concrete_trait_impl_overlaps_generic(
        &self,
        target: &Ty,
        trait_ref: &TraitRefKey,
    ) -> bool {
        let Some(name) = nominal_name(target) else {
            return false;
        };
        let Some(instance) = self.collection.nominal_instances.get(name) else {
            return false;
        };
        if instance.key.arguments.is_empty() {
            return false;
        }
        let Some(extensions) = self
            .collection
            .generic_trait_extensions
            .get(&instance.key.template)
        else {
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

    pub(super) fn generic_trait_extension_overlaps_concrete(
        &self,
        target_template: &str,
        extension: &GenericTraitExtension,
    ) -> bool {
        self.collection.trait_impls.keys().any(|key| {
            let Some(name) = nominal_name(&key.self_ty) else {
                return false;
            };
            let Some(instance) = self.collection.nominal_instances.get(name) else {
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

    pub(super) fn collect_generic_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
    ) {
        let compile_parameter_sorts = compile_parameter_sorts(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic trait extension",
            &extension.where_predicates,
            &compile_parameter_sorts,
        ) {
            return;
        }
        let parameters = extension
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if parameters.is_empty() || extension.compile_groups.iter().any(Vec::is_empty) {
            self.error("generic trait extend requires non-empty compile-time parameter groups");
            return;
        }
        let mut declared = HashSet::new();
        for parameter in &parameters {
            if parameter.name == "self" || !declared.insert(parameter.name.clone()) {
                self.error(format!(
                    "invalid or duplicate generic extend parameter `{}`",
                    parameter.name
                ));
                return;
            }
        }
        match extension.target.clone() {
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } if self.is_lang_item_name(&constructor, LangItemKind::ArrayTypeForm) => {
                self.collect_array_trait_extension(
                    extension, origin, parameters, declared, *element, length,
                );
                return;
            }
            _ => {}
        }
        if let Type::Named(target, arguments) = extension.target.clone() {
            if self.is_lang_item_name(&target, LangItemKind::SliceTypeForm) {
                self.collect_slice_trait_extension(
                    extension, origin, parameters, declared, arguments,
                );
                return;
            }
        }
        let Type::Named(target_template, target_sources) = &extension.target else {
            self.error("generic trait extend target must be a generic nominal type");
            return;
        };
        let expected = self
            .collection
            .struct_templates
            .get(target_template)
            .map(|definition| definition.compile_groups.iter().flatten().count())
            .or_else(|| {
                self.collection
                    .enum_templates
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
        let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
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
            .collection
            .nominal_accesses
            .get(target_template)
            .map(|access| access.origin.package);
        if (is_copy || is_drop) && target_package != Some(origin.package) {
            let trait_name = if is_copy { "copyable" } else { "droppable" };
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
            .collection
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
        self.collection
            .generic_trait_extensions
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
            .collection
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

    pub(super) fn collect_generic_constructor_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
    ) {
        let compile_parameter_sorts = compile_parameter_sorts(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic constructor trait extension",
            &extension.where_predicates,
            &compile_parameter_sorts,
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
            if parameter.kind != Sort::Type
                || parameter.name == "self"
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
        let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid {
            return;
        }
        let Sort::TypeConstructor {
            ref parameter_groups,
        } = schema.self_parameter.kind
        else {
            self.error(format!(
                "trait `{trait_name}` does not accept a type-constructor implementation target"
            ));
            return;
        };
        if *parameter_groups != target.target.parameter_groups {
            self.error(format!(
                "type constructor `{}` has sort {}, but trait `{trait_name}` expects sort {}",
                target.target.name,
                describe_compile_sort(Sort::TypeConstructor {
                    parameter_groups: target.target.parameter_groups.clone(),
                }),
                describe_compile_sort(Sort::TypeConstructor {
                    parameter_groups: parameter_groups.clone(),
                }),
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
            if parameter.kind != Sort::Type {
                self.error(format!(
                    "constructor trait implementation argument `{}` for `{trait_name}` has unsupported compile-time sort {}",
                    parameter.name,
                    describe_compile_sort(parameter.kind.clone())
                ));
                return;
            }
        }
        if !self.validate_generic_trait_members(trait_name, &schema, &extension.members) {
            return;
        }

        let target_package = self
            .collection
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
        if !self
            .collection
            .constructor_trait_impl_headers
            .insert(key.clone())
        {
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
        substitutions.insert("self".to_owned(), target.self_constructor.clone());
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
                || !method_compile_parameter_groups_match(&expected, &function)
                || !generic_method_contracts_match(&expected, &function)
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
            self.collection
                .function_template_order
                .push(canonical.clone());
            self.collection
                .function_templates
                .insert(canonical.clone(), function);
            self.collection
                .function_template_origins
                .insert(canonical.clone(), function_origin);
            self.collection
                .function_accesses
                .insert(canonical.clone(), implementation_access.clone());
            registered_methods.insert(method_id.clone(), canonical);
        }
        if !valid {
            return;
        }
        self.collection
            .constructor_trait_impl_methods
            .insert(key, registered_methods);
    }

    pub(super) fn validate_generic_trait_members(
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
                        Sort::EffectConstructor { .. }
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
                    if function.body.is_none() && !function.builtin {
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
            match schema.associated_type_kinds[name].clone() {
                Sort::Type | Sort::TypeConstructor { .. } => {}
                Sort::Parameters => {
                    self.error(format!(
                        "associated parameter schema `{trait_name}.{name}` implementations are compiler-derived only"
                    ));
                    valid = false;
                    continue;
                }
                Sort::ParameterPack => {
                    unreachable!("associated types cannot be parameter-group packs")
                }
                Sort::ParameterModifier => {
                    unreachable!("associated types cannot be parameter modifiers")
                }
                Sort::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{trait_name}.{name}` implementations are not supported yet"
                    ));
                    valid = false;
                    continue;
                }
                Sort::Universe(_)
                | Sort::Region
                | Sort::USize
                | Sort::Effect
                | Sort::Effects
                | Sort::Named(_) => {
                    unreachable!("associated types only store type sorts")
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

    pub(super) fn validate_generic_trait_method_shapes(
        &mut self,
        trait_name: &str,
        schema: &TraitSchema,
        trait_arguments: &[Type],
        extension: &ExtendDef,
    ) -> bool {
        let outer_binders = extension
            .compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.as_str())
            .collect::<HashSet<_>>();
        let mut binders_valid = true;
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                continue;
            };
            if let Some(parameter) = function
                .compile_groups
                .iter()
                .flatten()
                .find(|parameter| outer_binders.contains(parameter.name.as_str()))
            {
                self.error(format!(
                    "trait method `{trait_name}.{}` redeclares implementation compile-time parameter `{}`",
                    function.name, parameter.name
                ));
                binders_valid = false;
            }
        }
        if !binders_valid {
            return false;
        }

        let mut expected_substitutions = schema
            .compile_parameters
            .iter()
            .zip(trait_arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        expected_substitutions.insert("self".to_owned(), extension.target.clone());
        for parameter in extension.compile_groups.iter().flatten() {
            match &parameter.kind {
                Sort::USize => {
                    expected_substitutions
                        .entry(parameter.name.clone())
                        .or_insert(Type::CompileUSize(0));
                }
                Sort::Named(compile_type) => {
                    let Some(member) = self
                        .collection
                        .closed_type_values
                        .get(compile_type)
                        .and_then(|members| members.first())
                    else {
                        self.error(format!(
                            "generic trait implementation parameter `{}` uses unknown or empty closed type `{compile_type}`",
                            parameter.name
                        ));
                        return false;
                    };
                    expected_substitutions
                        .entry(parameter.name.clone())
                        .or_insert_with(|| {
                            Type::Named(closed_value_marker(compile_type, member), Vec::new())
                        });
                }
                _ => {}
            }
        }
        let mut raw_associated = HashMap::new();
        let mut valid = true;
        for member in &extension.members {
            let ExtendMember::Const(binding) = member else {
                continue;
            };
            if matches!(
                schema.associated_type_kinds.get(&binding.name),
                Some(Sort::EffectConstructor { .. })
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
            match schema.associated_type_kinds[associated].clone() {
                Sort::Type => {
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
                Sort::TypeConstructor { .. } => {
                    let Some(source) = raw_associated.get(associated) else {
                        valid = false;
                        continue;
                    };
                    if self.validate_associated_type_constructor(
                        trait_name,
                        associated,
                        source,
                        &schema.associated_type_parameters[associated],
                    ) {
                        expected_substitutions.insert(associated.clone(), source.clone());
                    } else {
                        valid = false;
                    }
                }
                Sort::Parameters => {
                    self.error(format!(
                        "associated parameter schema `{trait_name}.{associated}` implementations are compiler-derived only"
                    ));
                    valid = false;
                }
                Sort::ParameterPack => {
                    unreachable!("associated types cannot be parameter-group packs")
                }
                Sort::ParameterModifier => {
                    unreachable!("associated types cannot be parameter modifiers")
                }
                Sort::EffectConstructor { .. } => {
                    self.error(format!(
                        "effect associated constructor `{trait_name}.{associated}` implementations are not supported yet"
                    ));
                    valid = false;
                }
                Sort::Universe(_)
                | Sort::Region
                | Sort::USize
                | Sort::Effect
                | Sort::Effects
                | Sort::Named(_) => {
                    unreachable!("associated types only store type sorts")
                }
            }
        }
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
            if !method_compile_parameter_groups_match(&expected, &actual) {
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
            substitute_function_types(&mut actual, &expected_substitutions);
            if !self.expand_function_aliases_after_substitution(
                &mut actual,
                "generic trait implementation signature",
            ) {
                valid = false;
                continue;
            }
            if !generic_method_contracts_match(&expected, &actual) {
                self.error(format!(
                    "trait method `{trait_name}.{method_name}` signature mismatch in generic implementation"
                ));
                valid = false;
            }
        }
        valid
    }

    pub(super) fn instantiate_generic_trait_extensions_for_instance(
        &mut self,
        target_template: &str,
        canonical: &str,
        source_arguments: &[Type],
    ) {
        if self.collection.suppress_generic_inherent_instantiation != 0 {
            return;
        }
        if !source_arguments
            .iter()
            .all(|source| self.source_type_is_concrete(source))
        {
            return;
        }
        let extensions = self
            .collection
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

    pub(super) fn generic_trait_extension_impl_exists(
        &mut self,
        canonical: &str,
        source_arguments: &[Type],
        extension: &GenericTraitExtension,
    ) -> bool {
        let Some(instance) = self.collection.nominal_instances.get(canonical).cloned() else {
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
        self.collection.trait_impl_headers.contains(&key)
            || self.collection.trait_impls.contains_key(&key)
    }

    pub(super) fn register_generic_trait_validation_templates(
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
        self_substitution.insert("self".to_owned(), extension.target.clone());
        let target_access = self.nominal_access_or_internal(target_template);
        let validation_access = Self::intersect_access_boundaries(access, &target_access, origin);
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                continue;
            };
            let canonical = format!(
                "$generic$trait$validation${target_template}${trait_name}${}${}",
                function.name,
                self.collection.function_template_order.len()
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
            if !schema_function_has_receiver(&template) {
                if let Some(body) = &mut template.body {
                    substitute_self_expression_target(body, target_template);
                }
            }
            self.collection
                .function_template_order
                .push(canonical.clone());
            self.collection
                .function_templates
                .insert(canonical.clone(), template);
            self.collection
                .function_template_origins
                .insert(canonical.clone(), origin.clone());
            self.collection
                .function_accesses
                .insert(canonical, validation_access.clone());
        }
    }

    pub(super) fn instantiate_generic_trait_extension(
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
            substitute_where_predicate(predicate, &substitutions);
        }
        if predicates
            .iter()
            .any(|predicate| !self.concrete_where_predicate_holds(predicate))
        {
            return;
        }
        let mut trait_ref = extension.trait_ref.clone();
        substitute_type_parameters(&mut trait_ref, &substitutions);
        self.collection.instantiating_generic_trait_extension += 1;
        let already_registered = self
            .instantiated_generic_trait_key(canonical, &trait_ref)
            .is_some_and(|key| self.collection.trait_impl_headers.contains(&key));
        if already_registered {
            self.collection.instantiating_generic_trait_extension -= 1;
            return;
        }
        let mut members = extension.members.clone();
        for member in &mut members {
            match member {
                ExtendMember::Function(function) => {
                    substitute_function_types(function, &substitutions);
                    if let Some(body) = &mut function.body {
                        substitute_type_expression_parameters(body, &substitutions);
                    }
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
        self.collection.instantiating_generic_trait_extension -= 1;
    }

    pub(super) fn instantiated_generic_trait_key(
        &mut self,
        canonical: &str,
        trait_ref: &Type,
    ) -> Option<TraitImplKey> {
        let instance = self.collection.nominal_instances.get(canonical).cloned()?;
        let (trait_ref, _, _) = self.resolve_trait_impl_ref(trait_ref)?;
        let self_ty = match instance.key.kind {
            NominalKind::Struct => Ty::Struct(canonical.to_owned()),
            NominalKind::Enum => Ty::Enum(canonical.to_owned()),
        };
        Some(TraitImplKey { self_ty, trait_ref })
    }

    pub(super) fn collect_generic_inherent_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
    ) {
        let compile_parameter_sorts = compile_parameter_sorts(&extension.compile_groups);
        if !self.validate_where_predicate_shapes(
            "generic inherent extension",
            &extension.where_predicates,
            &compile_parameter_sorts,
        ) {
            return;
        }
        let parameters = extension
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        if parameters.is_empty() || extension.compile_groups.iter().any(Vec::is_empty) {
            self.error("generic inherent extend requires non-empty compile-time parameter groups");
            return;
        }
        let mut declared = HashSet::new();
        for parameter in &parameters {
            if parameter.name == "self" || !declared.insert(parameter.name.clone()) {
                self.error(format!(
                    "invalid or duplicate generic extend parameter `{}`",
                    parameter.name
                ));
                return;
            }
        }
        let extension_target = extension.target.clone();
        if let Type::ArrayApplication {
            constructor,
            element,
            length,
        } = &extension_target
        {
            if self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm) {
                self.collect_array_inherent_extension(
                    extension,
                    origin,
                    parameters,
                    declared,
                    element.as_ref().clone(),
                    length.clone(),
                );
                return;
            }
        }
        let Type::Named(target_template, target_sources) = &extension_target else {
            self.error("generic inherent extend target must be a generic nominal type");
            return;
        };
        if self.is_lang_item_name(target_template, LangItemKind::PtrTypeForm) {
            self.collect_pointer_inherent_extension(
                extension,
                origin,
                parameters,
                declared,
                target_sources.clone(),
            );
            return;
        }
        if self.is_lang_item_name(target_template, LangItemKind::SliceTypeForm) {
            self.collect_slice_inherent_extension(
                extension,
                origin,
                parameters,
                declared,
                target_sources.clone(),
            );
            return;
        }
        let expected = self
            .collection
            .struct_templates
            .get(target_template)
            .map(|definition| definition.compile_groups.iter().flatten().count())
            .or_else(|| {
                self.collection
                    .enum_templates
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
            .collection
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
                if let Some(schema) = self.collection.traits.get(trait_name) {
                    extension_access = Self::intersect_access_boundaries(
                        &extension_access,
                        &schema.access,
                        &origin,
                    );
                }
            }
            for binding in &predicate.associated_types {
                if binding.compile_groups.is_empty() && self.source_type_is_concrete(&binding.ty) {
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
                .collection
                .inherent_overload_counts
                .get(&overload_key)
                .cloned()
                .unwrap_or_default()
                > 1;
            if overloaded {
                let shape = function_parameter_labels(function);
                if !self
                    .collection
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
                .collection
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
                .collection
                .inherent_overload_counts
                .get(&overload_key)
                .cloned()
                .unwrap_or_default()
                > 1;
            if self
                .collection
                .generic_inherent_functions
                .contains_key(&key)
                && !overloaded
            {
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
                self.collection
                    .inherent_overloads
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
            self_substitution.insert("self".to_owned(), extension.target.clone());
            substitute_function_types(&mut generic, &self_substitution);
            if let Some(body) = &mut generic.body {
                substitute_self_expression_target(body, target_template);
            }
            self.collection
                .function_template_order
                .push(canonical.clone());
            self.collection
                .function_templates
                .insert(canonical.clone(), generic);
            self.collection
                .function_template_origins
                .insert(canonical.clone(), origin.clone());
            self.collection
                .function_accesses
                .insert(canonical.clone(), extension_access.clone());
            self.collection
                .generic_inherent_functions
                .entry(key)
                .or_insert(canonical);
        }

        self.collection
            .generic_inherent_extensions
            .entry(target_template.clone())
            .or_default()
            .push(template.clone());

        let existing = self
            .collection
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

    pub(super) fn collect_pointer_inherent_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        parameters: Vec<CompileParam>,
        declared: HashSet<String>,
        target_sources: Vec<Type>,
    ) {
        if origin.package != PackageId::CORE.0 {
            self.error(
                "inherent extension for `ptr` must be declared in the package that defines the type",
            );
            return;
        }
        let (access_parameter, mutable, pointee_parameter) = match target_sources.as_slice() {
            [Type::Named(access, access_arguments), Type::Named(pointee, pointee_arguments)]
                if access_arguments.is_empty()
                    && pointee_arguments.is_empty()
                    && declared.contains(pointee) =>
            {
                let pointee_is_type = parameters
                    .iter()
                    .any(|parameter| parameter.name == *pointee && parameter.kind == Sort::Type);
                if !pointee_is_type {
                    self.error("`ptr` extension pointee must be determined by a `type` parameter");
                    return;
                }
                if let Some(mutable) = compile_time::access_mutability(access) {
                    (None, Some(mutable), pointee.clone())
                } else if declared.contains(access)
                    && parameters.iter().any(|parameter| {
                        parameter.name == *access
                            && matches!(
                                &parameter.kind,
                                Sort::Named(kind) if kind == "access"
                            )
                    })
                {
                    (Some(access.clone()), None, pointee.clone())
                } else {
                    self.error(
                        "`ptr` extension access must be `shared`, `mut`, or a declared `access` parameter",
                    );
                    return;
                }
            }
            [Type::Named(pointee, pointee_arguments)]
                if pointee_arguments.is_empty() && declared.contains(pointee) =>
            {
                if !parameters
                    .iter()
                    .any(|parameter| parameter.name == *pointee && parameter.kind == Sort::Type)
                {
                    self.error("`ptr` extension pointee must be determined by a `type` parameter");
                    return;
                }
                (None, Some(false), pointee.clone())
            }
            _ => {
                self.error(
                    "generic `ptr` extend target must be `ptr(A)(T)`, `ptr(T)`, or `ptr(mut)(T)`",
                );
                return;
            }
        };
        let determined = access_parameter
            .iter()
            .chain(std::iter::once(&pointee_parameter))
            .cloned()
            .collect::<HashSet<_>>();
        if determined != declared {
            self.error(
                "every generic `ptr` extend parameter must be determined by the target type",
            );
            return;
        }
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                self.error("generic `ptr` associated constants are not supported");
                return;
            };
            if !function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self")
            {
                self.error("generic `ptr` extensions currently support methods only");
                return;
            }
            if let Some(parameter) = function
                .compile_groups
                .iter()
                .flatten()
                .find(|parameter| declared.contains(&parameter.name))
            {
                self.error(format!(
                    "generic `ptr` method `{}` redeclares outer compile-time parameter `{}`",
                    function.name, parameter.name
                ));
                return;
            }
        }
        let member_access = AccessBoundary {
            visibility: Visibility::Public,
            origin: origin.clone(),
        };
        self.collection
            .pointer_inherent_extensions
            .push(PointerInherentExtension {
                access_parameter,
                mutable,
                pointee_parameter,
                where_predicates: extension.where_predicates,
                members: extension.members,
                access: member_access,
                origin,
            });
    }

    pub(super) fn collect_slice_inherent_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        parameters: Vec<CompileParam>,
        declared: HashSet<String>,
        target_sources: Vec<Type>,
    ) {
        if origin.package != PackageId::CORE.0 {
            self.error(
                "inherent extension for `slice` must be declared in the package that defines the type",
            );
            return;
        }
        let [Type::Named(element, arguments)] = target_sources.as_slice() else {
            self.error("generic `slice` extend target must be `slice(T)`");
            return;
        };
        if !arguments.is_empty()
            || !declared.contains(element)
            || !parameters
                .iter()
                .any(|parameter| parameter.name == *element && parameter.kind == Sort::Type)
        {
            self.error("`slice` extension element must be determined by a `type` parameter");
            return;
        }
        if declared != HashSet::from([element.clone()]) {
            self.error(
                "every generic `slice` extend parameter must be determined by the target type",
            );
            return;
        }
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                self.error("generic `slice` associated constants are not supported");
                return;
            };
            if !function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self")
            {
                self.error("generic `slice` extensions currently support methods only");
                return;
            }
            if let Some(parameter) = function
                .compile_groups
                .iter()
                .flatten()
                .find(|parameter| declared.contains(&parameter.name))
            {
                self.error(format!(
                    "generic `slice` method `{}` redeclares outer compile-time parameter `{}`",
                    function.name, parameter.name
                ));
                return;
            }
        }
        let member_access = AccessBoundary {
            visibility: Visibility::Public,
            origin: origin.clone(),
        };
        self.collection
            .slice_inherent_extensions
            .push(SliceInherentExtension {
                element_parameter: element.clone(),
                where_predicates: extension.where_predicates,
                members: extension.members,
                access: member_access,
                origin,
            });
    }

    pub(super) fn collect_slice_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        parameters: Vec<CompileParam>,
        declared: HashSet<String>,
        target_sources: Vec<Type>,
    ) {
        if origin.package != PackageId::CORE.0 {
            self.error(
                "trait extension for `slice` must be declared in the package that defines the type or trait",
            );
            return;
        }
        let [Type::Named(element, arguments)] = target_sources.as_slice() else {
            self.error("generic `slice` trait target must be `slice(T)`");
            return;
        };
        if !arguments.is_empty()
            || declared != HashSet::from([element.clone()])
            || !parameters
                .iter()
                .any(|parameter| parameter.name == *element && parameter.kind == Sort::Type)
        {
            self.error("`slice` trait element must be determined by one `type` parameter");
            return;
        }
        let Some(Type::Named(trait_name, trait_arguments)) = extension.trait_ref.as_ref() else {
            self.error("generic `slice` extension must reference a named trait");
            return;
        };
        let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        if !schema.valid
            || trait_arguments.len() != schema.compile_parameters.len()
            || !self.validate_generic_trait_members(trait_name, &schema, &extension.members)
            || !self.validate_generic_trait_method_shapes(
                trait_name,
                &schema,
                trait_arguments,
                &extension,
            )
        {
            return;
        }
        self.collection
            .slice_trait_extensions
            .push(SliceTraitExtension {
                element_parameter: element.clone(),
                extension,
                origin,
            });
    }

    pub(super) fn collect_array_inherent_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        parameters: Vec<CompileParam>,
        declared: HashSet<String>,
        element: Type,
        length: crate::ast::USizeConst,
    ) {
        if origin.package != PackageId::CORE.0 {
            self.error("inherent extension for `array` must be declared in core");
            return;
        }
        let crate::ast::USizeConst::Parameter(length_parameter) = length else {
            self.error("generic `array` extend target length must be a usize parameter");
            return;
        };
        let Type::Named(element_parameter, element_arguments) = element else {
            self.error("generic `array` extension element must be a declared type parameter");
            return;
        };
        if !element_arguments.is_empty()
            || !parameters.iter().any(|parameter| {
                parameter.name == element_parameter && parameter.kind == Sort::Type
            })
        {
            self.error("`array` extension element must be determined by a `type` parameter");
            return;
        }
        let expected = HashSet::from([element_parameter.clone(), length_parameter.clone()]);
        if declared != expected {
            self.error(
                "every generic `array` extend parameter must be determined by its element and length",
            );
            return;
        }
        for member in &extension.members {
            let ExtendMember::Function(function) = member else {
                self.error("generic `array` associated constants are not supported");
                return;
            };
            if !function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self")
            {
                self.error("generic `array` extensions currently support methods only");
                return;
            }
            if let Some(parameter) = function
                .compile_groups
                .iter()
                .flatten()
                .find(|parameter| declared.contains(&parameter.name))
            {
                self.error(format!(
                    "generic `array` method `{}` redeclares outer compile-time parameter `{}`",
                    function.name, parameter.name
                ));
                return;
            }
        }
        self.collection
            .array_inherent_extensions
            .push(ArrayInherentExtension {
                element_parameter,
                length_parameter,
                where_predicates: extension.where_predicates,
                members: extension.members,
                access: AccessBoundary {
                    visibility: Visibility::Public,
                    origin: origin.clone(),
                },
                origin,
            });
    }

    pub(super) fn collect_array_trait_extension(
        &mut self,
        extension: ExtendDef,
        origin: ItemOrigin,
        parameters: Vec<CompileParam>,
        declared: HashSet<String>,
        element: Type,
        length: crate::ast::USizeConst,
    ) {
        if origin.package != PackageId::CORE.0 {
            self.error("trait extension for `array` must be declared in core");
            return;
        }
        let crate::ast::USizeConst::Parameter(length_parameter) = length else {
            self.error("generic `array` trait target length must be a usize parameter");
            return;
        };
        let (element_parameter, required_element, expected_declared) = match element {
            Type::Named(element_parameter, element_arguments)
                if element_arguments.is_empty()
                    && parameters.iter().any(|parameter| {
                        parameter.name == element_parameter && parameter.kind == Sort::Type
                    }) =>
            {
                (
                    Some(element_parameter.clone()),
                    None,
                    HashSet::from([element_parameter, length_parameter.clone()]),
                )
            }
            Type::U8 => (
                None,
                Some(Ty::U8),
                HashSet::from([length_parameter.clone()]),
            ),
            _ => {
                self.error("generic `array` trait target element must be a type parameter or `u8`");
                return;
            }
        };
        if declared != expected_declared
            || !parameters.iter().any(|parameter| {
                parameter.name == length_parameter && parameter.kind == Sort::USize
            })
        {
            self.error("array trait parameters must be determined by its target");
            return;
        }
        let Some(Type::Named(trait_name, trait_arguments)) = extension.trait_ref.as_ref() else {
            self.error("generic `array` extension must reference a named trait");
            return;
        };
        let Some(schema) = self.collection.traits.get(trait_name).cloned() else {
            self.error(format!("unknown trait `{trait_name}`"));
            return;
        };
        let mut validation_extension = extension.clone();
        for member in &mut validation_extension.members {
            if let ExtendMember::Function(function) = member {
                if function.builtin {
                    function.body = Some(Expr::Unit);
                    function.builtin = false;
                }
            }
        }
        if !schema.valid
            || trait_arguments.len() != schema.compile_parameters.len()
            || !self.validate_generic_trait_members(
                trait_name,
                &schema,
                &validation_extension.members,
            )
            || !self.validate_generic_trait_method_shapes(
                trait_name,
                &schema,
                trait_arguments,
                &validation_extension,
            )
        {
            return;
        }
        self.collection
            .array_trait_extensions
            .push(ArrayTraitExtension {
                element_parameter,
                required_element,
                length_parameter,
                extension,
                origin,
            });
    }

    pub(super) fn ensure_array_trait_extensions(&mut self, array: &Ty) {
        let Ty::Array(element, length) = array else {
            return;
        };
        let key = array.to_string();
        if !self
            .collection
            .instantiated_array_trait_extensions
            .insert(key.clone())
        {
            return;
        }
        let Some(element_source) = self.source_type_for_ty(element) else {
            self.error(format!(
                "cannot preserve element type `{element}` while instantiating `array` traits"
            ));
            return;
        };
        let array_source = Type::Array(Box::new(element_source.clone()), *length);
        for template in self.collection.array_inherent_extensions.clone() {
            let member_access =
                self.restrict_access_boundary_to_type(&template.access, element, &template.origin);
            let mut substitutions = HashMap::new();
            substitutions.insert(template.element_parameter, element_source.clone());
            substitutions.insert(template.length_parameter, Type::CompileUSize(*length));
            let mut predicates = template.where_predicates;
            for predicate in &mut predicates {
                substitute_where_predicate(predicate, &substitutions);
            }
            if predicates
                .iter()
                .any(|predicate| !self.concrete_where_predicate_holds(predicate))
            {
                continue;
            }
            let mut members = template.members;
            for member in &mut members {
                let ExtendMember::Function(function) = member else {
                    unreachable!("array associated constants were rejected")
                };
                substitute_function_types(function, &substitutions);
                if let Some(body) = &mut function.body {
                    substitute_type_expression_parameters(body, &substitutions);
                }
                let mut self_substitution = HashMap::new();
                self_substitution.insert("self".to_owned(), array_source.clone());
                substitute_function_types(function, &self_substitution);
            }
            self.register_builtin_extension_methods(
                &key,
                array,
                members,
                &member_access,
                &template.origin,
            );
        }
        for template in self.collection.array_trait_extensions.clone() {
            if template
                .required_element
                .as_ref()
                .is_some_and(|required| required != element.as_ref())
            {
                continue;
            }
            let mut substitutions = HashMap::new();
            if let Some(parameter) = template.element_parameter {
                substitutions.insert(parameter, element_source.clone());
            }
            substitutions.insert(template.length_parameter, Type::CompileUSize(*length));
            let mut extension = template.extension;
            substitute_type_parameters(&mut extension.target, &substitutions);
            if let Some(trait_ref) = &mut extension.trait_ref {
                substitute_type_parameters(trait_ref, &substitutions);
            }
            for member in &mut extension.members {
                match member {
                    ExtendMember::Const(binding) => {
                        substitute_type_expression_parameters(&mut binding.value, &substitutions);
                    }
                    ExtendMember::Function(function) => {
                        substitute_function_types(function, &substitutions);
                        if let Some(body) = &mut function.body {
                            substitute_type_expression_parameters(body, &substitutions);
                        }
                    }
                }
            }
            extension.compile_groups.clear();
            self.collection.instantiating_array_trait_extension += 1;
            self.collect_trait_extension(extension, template.origin);
            self.collection.instantiating_array_trait_extension -= 1;
        }
    }

    pub(super) fn pointer_inherent_owner(pointer: &Ty) -> String {
        format!("$pointer${}", hex_name(&pointer.to_string()))
    }

    pub(super) fn ensure_pointer_inherent_extensions(&mut self, pointer: &Ty) -> Option<String> {
        let Ty::Pointer { pointee, mutable } = pointer else {
            return None;
        };
        let owner = Self::pointer_inherent_owner(pointer);
        if !self
            .collection
            .instantiated_pointer_extensions
            .insert(owner.clone())
        {
            return Some(owner);
        }
        let Some(pointee_source) = self.source_type_for_ty(pointee) else {
            self.error(format!(
                "cannot preserve pointee type `{pointee}` while instantiating `ptr` extensions"
            ));
            return Some(owner);
        };
        let pointer_source = Type::Named(
            self.lang_item_name(LangItemKind::PtrTypeForm).to_owned(),
            vec![
                Type::Named(
                    if *mutable { "mut" } else { "shared" }.to_owned(),
                    Vec::new(),
                ),
                pointee_source.clone(),
            ],
        );
        for extension in self.collection.pointer_inherent_extensions.clone() {
            if extension
                .mutable
                .is_some_and(|required| required != *mutable)
            {
                continue;
            }
            let mut substitutions = HashMap::new();
            substitutions.insert(extension.pointee_parameter.clone(), pointee_source.clone());
            if let Some(access) = &extension.access_parameter {
                substitutions.insert(
                    access.clone(),
                    Type::Named(
                        if *mutable { "mut" } else { "shared" }.to_owned(),
                        Vec::new(),
                    ),
                );
            }
            let mut predicates = extension.where_predicates.clone();
            for predicate in &mut predicates {
                substitute_where_predicate(predicate, &substitutions);
            }
            if predicates
                .iter()
                .any(|predicate| !self.concrete_where_predicate_holds(predicate))
            {
                continue;
            }
            let mut members = extension.members.clone();
            for member in &mut members {
                let ExtendMember::Function(function) = member else {
                    unreachable!("pointer associated constants were rejected")
                };
                substitute_function_types(function, &substitutions);
                let mut self_substitution = HashMap::new();
                self_substitution.insert("self".to_owned(), pointer_source.clone());
                substitute_function_types(function, &self_substitution);
            }
            self.register_builtin_extension_methods(
                &owner,
                pointer,
                members,
                &extension.access,
                &extension.origin,
            );
        }
        Some(owner)
    }

    pub(super) fn slice_inherent_owner(slice: &Ty) -> String {
        format!("$slice${}", hex_name(&slice.to_string()))
    }

    pub(super) fn ensure_slice_inherent_extensions(&mut self, slice: &Ty) -> Option<String> {
        let Ty::Slice(element) = slice else {
            return None;
        };
        let owner = Self::slice_inherent_owner(slice);
        if !self
            .collection
            .instantiated_slice_extensions
            .insert(owner.clone())
        {
            return Some(owner);
        }
        let Some(element_source) = self.source_type_for_ty(element) else {
            self.error(format!(
                "cannot preserve element type `{element}` while instantiating `slice` extensions"
            ));
            return Some(owner);
        };
        let slice_source = Type::Named(
            self.lang_item_name(LangItemKind::SliceTypeForm).to_owned(),
            vec![element_source.clone()],
        );
        for extension in self.collection.slice_inherent_extensions.clone() {
            let member_access = self.restrict_access_boundary_to_type(
                &extension.access,
                element,
                &extension.origin,
            );
            let mut substitutions = HashMap::new();
            substitutions.insert(extension.element_parameter.clone(), element_source.clone());
            let mut predicates = extension.where_predicates.clone();
            for predicate in &mut predicates {
                substitute_where_predicate(predicate, &substitutions);
            }
            if predicates
                .iter()
                .any(|predicate| !self.concrete_where_predicate_holds(predicate))
            {
                continue;
            }
            let mut members = extension.members.clone();
            for member in &mut members {
                let ExtendMember::Function(function) = member else {
                    unreachable!("slice associated constants were rejected")
                };
                substitute_function_types(function, &substitutions);
                let mut self_substitution = HashMap::new();
                self_substitution.insert("self".to_owned(), slice_source.clone());
                substitute_function_types(function, &self_substitution);
            }
            self.register_builtin_extension_methods(
                &owner,
                slice,
                members,
                &member_access,
                &extension.origin,
            );
        }
        for template in self.collection.slice_trait_extensions.clone() {
            let mut extension = template.extension;
            let mut substitutions = HashMap::new();
            substitutions.insert(template.element_parameter, element_source.clone());
            substitute_type_parameters(&mut extension.target, &substitutions);
            if let Some(trait_ref) = &mut extension.trait_ref {
                substitute_type_parameters(trait_ref, &substitutions);
            }
            for predicate in &mut extension.where_predicates {
                substitute_where_predicate(predicate, &substitutions);
            }
            for member in &mut extension.members {
                match member {
                    ExtendMember::Const(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            substitute_type_parameters(annotation, &substitutions);
                        }
                        substitute_type_expression_parameters(&mut binding.value, &substitutions);
                    }
                    ExtendMember::Function(function) => {
                        substitute_function_types(function, &substitutions);
                        let mut self_substitution = HashMap::new();
                        self_substitution.insert("self".to_owned(), slice_source.clone());
                        substitute_function_types(function, &self_substitution);
                    }
                }
            }
            extension.compile_groups.clear();
            self.collect_trait_extension(extension, template.origin);
        }
        Some(owner)
    }

    pub(super) fn register_builtin_extension_methods(
        &mut self,
        owner: &str,
        receiver: &Ty,
        members: Vec<ExtendMember>,
        member_access: &AccessBoundary,
        origin: &ItemOrigin,
    ) {
        for member in members {
            let ExtendMember::Function(mut function) = member else {
                unreachable!("pointer associated constants were rejected")
            };
            let short_name = function.name.clone();
            if self
                .collection
                .inherent_members
                .entry(owner.to_owned())
                .or_default()
                .methods
                .contains_key(&short_name)
            {
                self.error(format!(
                    "overlapping inherent method `{short_name}` for `{}`",
                    self.diagnostic_type_name(receiver)
                ));
                continue;
            }
            let canonical = inherent_method_name(owner, &short_name);
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
                let failure_error = function
                    .effects
                    .failure
                    .as_deref()
                    .map(|error| self.lower_source_type(error));
                self.lowering.signatures.insert(
                    canonical.clone(),
                    FunctionSig {
                        groups,
                        unsafety: self.function_effects_unsafe(&function.effects),
                        failure_error,
                        custom_effects: self.function_effects_custom_identities(&function.effects),
                        result,
                    },
                );
                self.collection.function_order.push(canonical.clone());
                self.collection
                    .functions
                    .insert(canonical.clone(), function);
                self.collection
                    .function_origins
                    .insert(canonical.clone(), origin.clone());
            } else {
                self.collection
                    .function_template_order
                    .push(canonical.clone());
                self.collection
                    .function_templates
                    .insert(canonical.clone(), function);
                self.collection
                    .function_template_origins
                    .insert(canonical.clone(), origin.clone());
            }
            self.collection
                .function_accesses
                .insert(canonical.clone(), member_access.clone());
            self.collection
                .inherent_members
                .entry(owner.to_owned())
                .or_default()
                .methods
                .insert(short_name, canonical);
        }
    }

    pub(super) fn instantiate_generic_inherent_extension(
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
            substitute_where_predicate(predicate, &substitutions);
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
                .collection
                .inherent_overload_counts
                .get(&(target_template.to_owned(), member.clone(), *is_method))
                .cloned()
                .unwrap_or(1);
            self.collection
                .inherent_overload_counts
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
        let option_extension = extension.origin.package == PackageId::CORE.0
            && extension
                .origin
                .module_path
                .last()
                .is_some_and(|module| module == "option");
        // A borrowed payload makes these fallback methods choose between two
        // anonymous input regions. Until source types can state their equality,
        // do not materialize a signature with no sound result-region inference.
        members.retain(|member| {
            let ExtendMember::Function(function) = member else {
                return true;
            };
            !option_extension
                || !matches!(function.name.as_str(), "unwrap_or" | "unwrap_or_else")
                || !matches!(function.return_type, Some(Type::Borrow { .. }))
        });
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
                .collection
                .inherent_overload_counts
                .get(&(canonical.to_owned(), member.clone(), is_method))
                .cloned()
                .unwrap_or_default()
                > 1
            {
                name = overloaded_function_name(&name, &shape);
            }
            if let Some(access) = self.collection.function_accesses.get(&name).cloned() {
                self.collection.function_accesses.insert(
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
}
