use crate::ast::{PassMode, Type};
use crate::core::LangItemKind;

use super::effects::standard_throws_error_source;
use super::flow::{LocalInfo, LowerCtx};
use super::hir::{HirFunction, HirGlobal, HirParam, LocalCapability, ParamSig, Ty};
use super::registry::ResolutionState;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_function(&mut self, name: &str) -> Ty {
        if self.function_states.get(name) == Some(&ResolutionState::Resolved) {
            return self.signatures[name].result.clone().unwrap_or(Ty::Error);
        }
        if self.function_states.get(name) == Some(&ResolutionState::Resolving) {
            if let Some(result) = self.signatures[name].result.clone() {
                return result;
            }
            self.error(format!(
                "cannot infer recursive function `{name}`; add a return type"
            ));
            return Ty::Error;
        }

        self.function_states
            .insert(name.to_owned(), ResolutionState::Resolving);
        let function = self.functions[name].clone();
        let signature = self.signatures[name].clone();
        let mut context = LowerCtx::for_function(
            name,
            signature.result.clone(),
            self.function_origins
                .get(name)
                .cloned()
                .expect("every registered function has source provenance"),
        );
        context.unsafe_depth = usize::from(self.function_effects_unsafe(&function.effects));
        context.active_throws_error = signature.throws_error.clone();
        context.active_custom_effect_sources =
            self.function_effects_custom_source_map(&function.effects);
        context.active_custom_effects = context
            .active_custom_effect_sources
            .keys()
            .cloned()
            .collect();
        if function.return_type.is_some() {
            context.return_boundary = signature.throws_error.as_ref().and_then(|error| {
                signature
                    .result
                    .as_ref()
                    .and_then(|result| self.throws_boundary_for_ty(result, error))
            });
        }
        context.type_substitutions = self
            .function_type_substitutions
            .get(name)
            .cloned()
            .unwrap_or_default();
        let mut params = Vec::new();

        for (group_index, group) in signature.groups.iter().enumerate() {
            for (parameter_index, param) in group.iter().enumerate() {
                self.validate_parameter_mode(name, param);
                let runtime_ty = self
                    .runtime_handler_actions
                    .get(&(name.to_owned(), group_index, parameter_index))
                    .map(|action| Ty::EffectCallable {
                        input: Box::new(action.input.clone()),
                        output: Box::new(action.output.clone()),
                        answer: Box::new(action.answer.clone()),
                    })
                    .unwrap_or_else(|| param.ty.clone());
                if context.scopes[0].names.contains_key(&param.name) {
                    self.error(format!(
                        "duplicate parameter `{}` in function `{name}`",
                        param.name
                    ));
                    continue;
                }
                let id = context.fresh_local();
                let source_parameter = &function.groups[group_index][parameter_index];
                if matches!(param.mode, PassMode::Borrow | PassMode::MutBorrow) {
                    context.borrowed_parameter_regions.insert(
                        id,
                        (
                            source_parameter.region.clone(),
                            param.mode == PassMode::MutBorrow,
                        ),
                    );
                } else if let Ty::Reference {
                    mutable, region, ..
                } = &runtime_ty
                {
                    context
                        .borrowed_parameter_regions
                        .insert(id, (region.clone(), *mutable));
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
                        ty: runtime_ty.clone(),
                        mutable: param.mode == PassMode::MutBorrow,
                        capability,
                        alias: None,
                        partial: None,
                        closure: None,
                    },
                );
                params.push(HirParam {
                    id,
                    name: param.name.clone(),
                    ty: runtime_ty,
                    mode: param.mode,
                });
            }
        }

        if let Some(Ty::Reference { region, .. }) = &signature.result {
            if let Some(region) = region {
                if !context
                    .borrowed_parameter_regions
                    .values()
                    .any(|(parameter_region, _)| parameter_region.as_ref() == Some(region))
                {
                    self.error(format!(
                        "function `{name}` returns region `'{region}` but has no borrow parameter with that region"
                    ));
                }
            } else if context.borrowed_parameter_regions.len() != 1 {
                self.error(format!(
                    "cannot infer the returned borrow region of function `{name}`; expected exactly one borrow parameter, found {}",
                    context.borrowed_parameter_regions.len()
                ));
            }
        }

        let Some(body) = function.body.as_ref() else {
            self.error(format!("function `{name}` has no body"));
            self.function_states
                .insert(name.to_owned(), ResolutionState::Resolved);
            self.set_function_result(name, Ty::Error);
            return Ty::Error;
        };

        let requires_resumable_lowering = function.effects.custom.iter().any(|effect| {
            let Type::Named(effect_name, _) = effect else {
                return false;
            };
            self.effect_defs
                .get(effect_name)
                .is_some_and(|definition| !definition.operations.is_empty())
        });
        let standard_throws_requires_handler_lowering =
            function.effects.custom.iter().any(|effect| {
                standard_throws_error_source(
                    effect,
                    self.lang_item_name(LangItemKind::ThrowsEffect),
                )
                .is_some()
            });
        let lifted_functions_before = self.lifted_functions.len();
        let continuation_adapters_before = self.continuation_adapters.len();
        let effect_callable_adapters_before = self.effect_callable_adapters.len();
        let handler_frame_parameter_modes_before = self.handler_frame_parameter_modes.clone();
        let boundary = context.return_boundary.clone();
        let lowered_body = if let Some(boundary) = &boundary {
            self.lower_return_value(body, boundary, &mut context)
        } else {
            self.lower_expr(body, signature.result.as_ref(), &mut context)
        };
        if let Some(expected @ Ty::Reference { .. }) = signature.result.as_ref() {
            self.validate_reference_escape_value(&lowered_body, expected, &context);
            self.validate_explicit_reference_returns(&lowered_body, expected, &context);
        }
        let result = if let Some(declared) = signature.result {
            for returned in &context.returned_types {
                self.require_same_type(
                    returned,
                    &declared,
                    format!("return value in function `{name}`"),
                );
            }
            declared
        } else {
            let mut inferred = if self.is_uninhabited_type(&lowered_body.ty) {
                None
            } else {
                Some(lowered_body.ty.clone())
            };
            for returned in &context.returned_types {
                inferred = Some(match inferred {
                    Some(current) => self.unify_types(
                        &current,
                        returned,
                        format!("return values in function `{name}`"),
                    ),
                    None => returned.clone(),
                });
            }
            inferred.unwrap_or(Ty::Unit)
        };

        self.set_function_result(name, result.clone());
        if !requires_resumable_lowering {
            self.hir_functions.insert(
                name.to_owned(),
                HirFunction {
                    name: name.to_owned(),
                    params,
                    result: result.clone(),
                    body: lowered_body,
                },
            );
        } else if standard_throws_requires_handler_lowering {
            self.lifted_functions.truncate(lifted_functions_before);
            self.continuation_adapters
                .truncate(continuation_adapters_before);
            self.effect_callable_adapters
                .truncate(effect_callable_adapters_before);
            self.handler_frame_parameter_modes = handler_frame_parameter_modes_before;
        }
        self.function_states
            .insert(name.to_owned(), ResolutionState::Resolved);
        result
    }

    fn validate_parameter_mode(&mut self, function: &str, param: &ParamSig) {
        if param.mode == PassMode::Copy && !self.is_copy_type(&param.ty) {
            let ty = self.diagnostic_type_name(&param.ty);
            self.error(format!(
                "parameter `{}` in function `{function}` requires `Copy`, but nominal type `{}` does not implement Copy",
                param.name, ty
            ));
        }
    }

    fn set_function_result(&mut self, name: &str, result: Ty) {
        if let Some(signature) = self.signatures.get_mut(name) {
            signature.result = Some(result);
        }
    }

    pub(super) fn function_type(&mut self, name: &str) -> Ty {
        let Some(signature) = self.signatures.get(name) else {
            self.error(format!("unknown function `{name}`"));
            return Ty::Error;
        };
        if signature.result.is_none() {
            self.lower_function(name);
        }
        self.signatures[name].function_ty().unwrap_or(Ty::Error)
    }

    pub(super) fn lower_global(&mut self, name: &str) -> Ty {
        if self.global_states.get(name) == Some(&ResolutionState::Resolved) {
            return self.hir_globals[name].ty.clone();
        }
        if self.global_states.get(name) == Some(&ResolutionState::Resolving) {
            self.error(format!("cyclic global constant involving `{name}`"));
            return Ty::Error;
        }
        self.global_states
            .insert(name.to_owned(), ResolutionState::Resolving);

        let binding = self.globals[name].clone();
        let expected = self.global_annotations[name].clone();
        let mut context = LowerCtx::for_global(
            self.global_origins
                .get(name)
                .cloned()
                .expect("every registered global has source provenance"),
        );
        let value = self.lower_expr(&binding.value, expected.as_ref(), &mut context);
        if !context.returned_types.is_empty() {
            self.error(format!("`return` is not allowed in global `{name}`"));
        }
        let ty = expected.unwrap_or_else(|| value.ty.clone());
        if matches!(ty, Ty::Function(_)) {
            self.error(format!(
                "global function values are not supported in M0 (`{name}`)"
            ));
        }
        self.hir_globals.insert(
            name.to_owned(),
            HirGlobal {
                name: name.to_owned(),
                ty: ty.clone(),
                value,
            },
        );
        self.global_states
            .insert(name.to_owned(), ResolutionState::Resolved);
        ty
    }

    pub(super) fn global_type(&mut self, name: &str) -> Ty {
        self.lower_global(name)
    }

    pub(super) fn validate_entry_point(&mut self) {
        let Some(signature) = self.signatures.get("main").cloned() else {
            self.error("binary program has no `main` function");
            return;
        };
        let result = match signature.result {
            Some(result) => result,
            None => self.lower_function("main"),
        };
        let signature = self.signatures["main"].clone();
        if signature.groups.len() != 1 || !signature.groups[0].is_empty() {
            self.error("`main` must have exactly one empty parameter group: `main()`");
        }
        if self.function_effects_unsafe(&self.functions["main"].effects) {
            self.error("`main` cannot expose an unhandled `unsafe` effect");
        }
        if !self
            .function_effects_custom_identities(&self.functions["main"].effects)
            .is_empty()
        {
            self.error("`main` cannot expose unhandled custom effects");
        }
        if let Some(error) = &signature.throws_error {
            self.error(format!(
                "`main` cannot expose unhandled `Throws({error})`; handle it with `try {{ ... }}`"
            ));
        }
        if !matches!(result, Ty::Unit | Ty::I32 | Ty::Error) {
            self.error(format!(
                "M0 `main` must return `()` or `i32`, found `{result}`"
            ));
        }
    }
}
