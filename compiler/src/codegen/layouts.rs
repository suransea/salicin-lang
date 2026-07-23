use std::collections::{HashMap, HashSet};

use crate::ast::{EnumDef, StructDef, VariantFields};

use super::hir::{EnumLayout, FieldLayout, StructLayout, Ty, VariantLayout};
use super::lower::nominal_name;
use super::registry::NominalInstanceState;
use super::Analyzer;

impl Analyzer {
    pub(super) fn collect_nominal_layouts(&mut self) {
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

    pub(super) fn build_struct_layout(&mut self, name: &str, definition: StructDef) {
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

    pub(super) fn build_enum_layout(&mut self, name: &str, definition: EnumDef) {
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

    pub(super) fn validate_nominal_layouts(&mut self) {
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

    pub(super) fn visit_nominal_layout(
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

    pub(super) fn struct_layout_or_diagnostic(&mut self, name: &str) -> Option<StructLayout> {
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

    pub(super) fn enum_layout_or_diagnostic(&mut self, name: &str) -> Option<EnumLayout> {
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
}
