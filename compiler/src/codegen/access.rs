use std::collections::HashSet;

use crate::ast::{Field, ItemOrigin, Type, VariantFields, Visibility};

use super::hir::{AccessBoundary, FieldLayout, Ty};
use super::Analyzer;

impl Analyzer {
    pub(super) fn access_boundary_allows(origin: &ItemOrigin, access: &AccessBoundary) -> bool {
        match access.visibility {
            Visibility::Public => true,
            Visibility::Package => origin.package == access.origin.package,
            Visibility::Private => {
                origin.package == access.origin.package
                    && origin
                        .module_path
                        .starts_with(access.origin.module_path.as_slice())
            }
        }
    }

    pub(super) fn effective_member_access(
        owner: &AccessBoundary,
        member_visibility: Visibility,
    ) -> AccessBoundary {
        let owner_rank = match owner.visibility {
            Visibility::Private => 0,
            Visibility::Package => 1,
            Visibility::Public => 2,
        };
        let member_rank = match member_visibility {
            Visibility::Private => 0,
            Visibility::Package => 1,
            Visibility::Public => 2,
        };
        AccessBoundary {
            visibility: if owner_rank <= member_rank {
                owner.visibility
            } else {
                member_visibility
            },
            origin: owner.origin.clone(),
        }
    }

    pub(super) fn nominal_access_or_internal(&mut self, name: &str) -> AccessBoundary {
        self.nominal_accesses.get(name).cloned().unwrap_or_else(|| {
            self.error(format!(
                "internal error: nominal type `{name}` has no visibility metadata"
            ));
            AccessBoundary {
                visibility: Visibility::Private,
                origin: ItemOrigin::default(),
            }
        })
    }

    pub(super) fn require_field_access(
        &mut self,
        owner: &str,
        field: &FieldLayout,
        origin: &ItemOrigin,
    ) -> bool {
        let display_owner = self
            .nominal_instances
            .get(owner)
            .map(|instance| instance.key.template.clone())
            .or_else(|| {
                self.nominal_instances
                    .iter()
                    .find_map(|(canonical, instance)| {
                        owner.strip_prefix(canonical).and_then(|suffix| {
                            suffix
                                .starts_with('.')
                                .then(|| format!("{}{suffix}", instance.key.template))
                        })
                    })
            })
            .unwrap_or_else(|| owner.to_owned());
        self.require_named_field_access(&display_owner, &field.name, &field.access, origin)
    }

    fn require_named_field_access(
        &mut self,
        owner: &str,
        field: &str,
        access: &AccessBoundary,
        origin: &ItemOrigin,
    ) -> bool {
        if Self::access_boundary_allows(origin, access) {
            return true;
        }
        let visibility = match access.visibility {
            Visibility::Private => "private",
            Visibility::Package => "pub(package)",
            Visibility::Public => "public",
        };
        self.error(format!(
            "field `{owner}.{field}` is {visibility} and is not accessible from this module"
        ));
        false
    }

    pub(super) fn require_source_fields_access(
        &mut self,
        owner: &str,
        fields: &[Field],
        origin: &ItemOrigin,
    ) -> bool {
        let owner_access = self.nominal_access_or_internal(owner);
        let mut accessible = true;
        for field in fields {
            let access = Self::effective_member_access(&owner_access, field.visibility);
            accessible &= self.require_named_field_access(owner, &field.name, &access, origin);
        }
        accessible
    }

    pub(super) fn require_source_variant_fields_access(
        &mut self,
        owner: &str,
        variant: &str,
        fields: &VariantFields,
        origin: &ItemOrigin,
    ) -> bool {
        let owner_access = self.nominal_access_or_internal(owner);
        let display_owner = format!("{owner}.{variant}");
        match fields {
            VariantFields::Unit => true,
            VariantFields::Positional(fields) => {
                let mut accessible = true;
                for index in 0..fields.len() {
                    accessible &= self.require_named_field_access(
                        &display_owner,
                        &index.to_string(),
                        &owner_access,
                        origin,
                    );
                }
                accessible
            }
            VariantFields::Named(fields) => {
                let mut accessible = true;
                for field in fields {
                    let access = Self::effective_member_access(&owner_access, field.visibility);
                    accessible &= self.require_named_field_access(
                        &display_owner,
                        &field.name,
                        &access,
                        origin,
                    );
                }
                accessible
            }
        }
    }

    pub(super) fn source_fields_are_accessible(
        &self,
        owner: &str,
        fields: &[Field],
        origin: &ItemOrigin,
    ) -> bool {
        let Some(owner_access) = self.nominal_accesses.get(owner) else {
            return false;
        };
        fields.iter().all(|field| {
            Self::access_boundary_allows(
                origin,
                &Self::effective_member_access(owner_access, field.visibility),
            )
        })
    }

    pub(super) fn source_variant_fields_are_accessible(
        &self,
        source: &Type,
        fields: &VariantFields,
        origin: &ItemOrigin,
    ) -> bool {
        let Type::Named(owner, _) = source else {
            return false;
        };
        let Some(owner_access) = self.nominal_accesses.get(owner) else {
            return false;
        };
        match fields {
            VariantFields::Unit => true,
            VariantFields::Positional(fields) => {
                fields.is_empty() || Self::access_boundary_allows(origin, owner_access)
            }
            VariantFields::Named(fields) => {
                self.source_fields_are_accessible(owner, fields, origin)
            }
        }
    }

    pub(super) fn api_audience_is_contained(
        exposed: &AccessBoundary,
        referenced: &AccessBoundary,
    ) -> bool {
        match referenced.visibility {
            Visibility::Public => true,
            Visibility::Package => {
                exposed.visibility != Visibility::Public
                    && exposed.origin.package == referenced.origin.package
            }
            Visibility::Private => {
                (exposed.visibility == Visibility::Private
                    && exposed.origin.package == referenced.origin.package
                    && exposed
                        .origin
                        .module_path
                        .starts_with(&referenced.origin.module_path))
                    || (exposed.visibility == Visibility::Package
                        && exposed.origin.package == referenced.origin.package
                        && referenced.origin.module_path.is_empty())
            }
        }
    }

    pub(super) fn intersect_access_boundaries(
        left: &AccessBoundary,
        right: &AccessBoundary,
        fallback_origin: &ItemOrigin,
    ) -> AccessBoundary {
        if Self::api_audience_is_contained(left, right) {
            left.clone()
        } else if Self::api_audience_is_contained(right, left) {
            right.clone()
        } else {
            AccessBoundary {
                visibility: Visibility::Private,
                origin: fallback_origin.clone(),
            }
        }
    }

    pub(super) fn restrict_access_boundary_to_type(
        &self,
        access: &AccessBoundary,
        ty: &Ty,
        fallback_origin: &ItemOrigin,
    ) -> AccessBoundary {
        fn visit(
            analyzer: &Analyzer,
            access: AccessBoundary,
            ty: &Ty,
            fallback_origin: &ItemOrigin,
            visited: &mut HashSet<String>,
        ) -> AccessBoundary {
            match ty {
                Ty::Array(element, _) => visit(analyzer, access, element, fallback_origin, visited),
                Ty::Pointer { pointee, .. } => {
                    visit(analyzer, access, pointee, fallback_origin, visited)
                }
                Ty::Reference { pointee, .. } => {
                    visit(analyzer, access, pointee, fallback_origin, visited)
                }
                Ty::Function(function) => {
                    let mut restricted = access;
                    for parameter in function.groups.iter().flatten() {
                        restricted =
                            visit(analyzer, restricted, parameter, fallback_origin, visited);
                    }
                    visit(
                        analyzer,
                        restricted,
                        &function.result,
                        fallback_origin,
                        visited,
                    )
                }
                Ty::Callable(callable) => {
                    let mut restricted = visit(
                        analyzer,
                        access,
                        &Ty::Function(callable.signature.clone()),
                        fallback_origin,
                        visited,
                    );
                    for capture in &callable.captures {
                        restricted =
                            visit(analyzer, restricted, &capture.ty, fallback_origin, visited);
                    }
                    restricted
                }
                Ty::Continuation { input, output } => {
                    let restricted = visit(analyzer, access, input, fallback_origin, visited);
                    visit(analyzer, restricted, output, fallback_origin, visited)
                }
                Ty::EffectCallable {
                    input,
                    output,
                    answer,
                } => {
                    let restricted = visit(analyzer, access, input, fallback_origin, visited);
                    let restricted = visit(analyzer, restricted, output, fallback_origin, visited);
                    visit(analyzer, restricted, answer, fallback_origin, visited)
                }
                Ty::EffectRow { throws_error, .. } => {
                    throws_error.as_deref().map_or(access.clone(), |error| {
                        visit(analyzer, access, error, fallback_origin, visited)
                    })
                }
                Ty::Struct(name) | Ty::Enum(name) => {
                    let mut restricted =
                        analyzer
                            .nominal_accesses
                            .get(name)
                            .map_or(access.clone(), |nominal| {
                                Analyzer::intersect_access_boundaries(
                                    &access,
                                    nominal,
                                    fallback_origin,
                                )
                            });
                    if visited.insert(name.clone()) {
                        if let Some(instance) = analyzer.nominal_instances.get(name) {
                            for argument in &instance.key.arguments {
                                restricted =
                                    visit(analyzer, restricted, argument, fallback_origin, visited);
                            }
                        }
                    }
                    restricted
                }
                Ty::I32
                | Ty::I64
                | Ty::U32
                | Ty::U64
                | Ty::Bool
                | Ty::Unit
                | Ty::Never
                | Ty::Error => access,
            }
        }

        visit(
            self,
            access.clone(),
            ty,
            fallback_origin,
            &mut HashSet::new(),
        )
    }

    pub(super) fn collect_type_api_leaks(
        &self,
        ty: &Ty,
        exposed: &AccessBoundary,
        description: &str,
        visited: &mut HashSet<String>,
        diagnostics: &mut Vec<String>,
    ) {
        match ty {
            Ty::Array(element, _) => {
                self.collect_type_api_leaks(element, exposed, description, visited, diagnostics)
            }
            Ty::Pointer { pointee, .. } => {
                self.collect_type_api_leaks(pointee, exposed, description, visited, diagnostics)
            }
            Ty::Reference { pointee, .. } => {
                self.collect_type_api_leaks(pointee, exposed, description, visited, diagnostics)
            }
            Ty::Function(function) => {
                for parameter in function.groups.iter().flatten() {
                    self.collect_type_api_leaks(
                        parameter,
                        exposed,
                        description,
                        visited,
                        diagnostics,
                    );
                }
                self.collect_type_api_leaks(
                    &function.result,
                    exposed,
                    description,
                    visited,
                    diagnostics,
                );
            }
            Ty::Callable(callable) => {
                self.collect_type_api_leaks(
                    &Ty::Function(callable.signature.clone()),
                    exposed,
                    description,
                    visited,
                    diagnostics,
                );
                for capture in &callable.captures {
                    self.collect_type_api_leaks(
                        &capture.ty,
                        exposed,
                        description,
                        visited,
                        diagnostics,
                    );
                }
            }
            Ty::Continuation { input, output } => {
                self.collect_type_api_leaks(input, exposed, description, visited, diagnostics);
                self.collect_type_api_leaks(output, exposed, description, visited, diagnostics);
            }
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => {
                self.collect_type_api_leaks(input, exposed, description, visited, diagnostics);
                self.collect_type_api_leaks(output, exposed, description, visited, diagnostics);
                self.collect_type_api_leaks(answer, exposed, description, visited, diagnostics);
            }
            Ty::EffectRow { throws_error, .. } => {
                if let Some(error) = throws_error {
                    self.collect_type_api_leaks(error, exposed, description, visited, diagnostics);
                }
            }
            Ty::Struct(name) | Ty::Enum(name) => {
                if self.abstract_type_parameters.contains_key(name) {
                    return;
                }
                let display_name = self
                    .nominal_instances
                    .get(name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(name);
                if let Some(referenced) = self.nominal_accesses.get(name) {
                    if !Self::api_audience_is_contained(exposed, referenced) {
                        let exposed_visibility = match exposed.visibility {
                            Visibility::Private => "private",
                            Visibility::Package => "pub(package)",
                            Visibility::Public => "public",
                        };
                        let referenced_visibility = match referenced.visibility {
                            Visibility::Private => "private",
                            Visibility::Package => "pub(package)",
                            Visibility::Public => "public",
                        };
                        diagnostics.push(format!(
                            "{description} with {exposed_visibility} visibility exposes {referenced_visibility} type `{display_name}` beyond its access boundary"
                        ));
                    }
                }
                if visited.insert(name.clone()) {
                    if let Some(instance) = self.nominal_instances.get(name) {
                        for argument in &instance.key.arguments {
                            self.collect_type_api_leaks(
                                argument,
                                exposed,
                                description,
                                visited,
                                diagnostics,
                            );
                        }
                    }
                }
            }
            Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool | Ty::Unit | Ty::Never | Ty::Error => {
            }
        }
    }

    pub(super) fn validate_inferred_api_visibility(&mut self) {
        let mut diagnostics = Vec::new();
        for (name, access) in &self.function_accesses {
            let Some(function) = self.hir_functions.get(name) else {
                continue;
            };
            for parameter in &function.params {
                self.collect_type_api_leaks(
                    &parameter.ty,
                    access,
                    &format!("function `{name}` parameter `{}`", parameter.name),
                    &mut HashSet::new(),
                    &mut diagnostics,
                );
            }
            self.collect_type_api_leaks(
                &function.result,
                access,
                &format!("function `{name}` return type"),
                &mut HashSet::new(),
                &mut diagnostics,
            );
        }
        for (name, access) in &self.global_accesses {
            let Some(global) = self.hir_globals.get(name) else {
                continue;
            };
            self.collect_type_api_leaks(
                &global.ty,
                access,
                &format!("global `{name}` type"),
                &mut HashSet::new(),
                &mut diagnostics,
            );
        }
        diagnostics.sort();
        diagnostics.dedup();
        for diagnostic in diagnostics {
            self.error(diagnostic);
        }
    }
}
