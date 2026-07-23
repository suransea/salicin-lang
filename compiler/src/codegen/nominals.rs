use std::collections::{HashMap, HashSet};

use crate::ast::{CallArg, Expr, ItemOrigin, Type, VariantFields, Visibility};

use super::flow::LowerCtx;
use super::hir::{AccessBoundary, EnumLayout, FunctionTy, StructLayout, Ty};
use super::lower::{flatten_call, ty_contains_nominal};
use super::names::{canonical_type_encoding, generic_parameter_marker, nominal_instance_name};
use super::registry::{
    collect_nominal_type_dependencies, NominalInstanceInfo, NominalInstanceKey,
    NominalInstanceState, NominalKind, NominalSnapshot, MAX_NOMINAL_INSTANCES,
};
use super::source_rewrite::{substitute_enum_types, substitute_struct_types};
use super::Analyzer;

impl Analyzer {
    pub(super) fn resolve_generic_nominal_instance(
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

    pub(super) fn resolve_nominal_type_head(
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

    pub(super) fn snapshot_nominals(&self) -> NominalSnapshot {
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

    pub(super) fn restore_nominals(&mut self, snapshot: NominalSnapshot) {
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

    pub(super) fn validate_generic_nominal_cycles(&mut self) {
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

    pub(super) fn validate_nominal_templates(&mut self) {
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

    pub(super) fn ensure_nominal_instance(
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
}
