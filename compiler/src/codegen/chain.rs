use crate::ast::{
    CallArg, Expr, ItemOrigin, MatchArm, Param, PassMode, Pattern, PatternFields, Type,
};
use crate::core::LangItemKind;

use super::fallible::{StandardFallibleInfo, StandardFallibleKind};
use super::flow::LowerCtx;
use super::handlers::rewrite_handler_chain_wrappers;
use super::hir::{HirExpr, LocalCapability, Ty};
use super::lower::{error_expr, place_root_name, CustomChainPlan, TypeProbe};
use super::names::nominal_instance_name;
use super::registry::{NominalInstanceKey, NominalKind};
use super::source_rewrite::{expand_alias_type, source_type_expression};
use super::Analyzer;

impl Analyzer {
    pub(super) fn probe_chain_access_ty(
        &self,
        payload: &Ty,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        origin: &ItemOrigin,
    ) -> Option<Ty> {
        let target = match payload {
            Ty::Struct(name) | Ty::Enum(name) => name,
            _ => return None,
        };
        if groups.is_none() {
            return self
                .struct_layouts
                .get(target)?
                .fields
                .iter()
                .find(|field| field.name == member)
                .filter(|field| Self::access_boundary_allows(origin, &field.access))
                .map(|field| field.ty.clone());
        }
        let groups = groups.expect("method chain has call groups");
        let overload_key = (target.clone(), member.to_owned(), true);
        let inherent = if let Some(candidates) = self.inherent_overloads.get(&overload_key) {
            if !groups
                .iter()
                .flat_map(|group| group.iter())
                .any(|argument| argument.label.is_some())
            {
                return None;
            }
            let matches = self.matching_function_overloads(candidates, groups, 1);
            let [selected] = matches.as_slice() else {
                return None;
            };
            Some(selected.clone())
        } else {
            self.inherent_members
                .get(target)
                .and_then(|members| members.methods.get(member))
                .cloned()
        };
        let canonical = if let Some(canonical) = inherent {
            canonical
        } else {
            let candidates = self.trait_method_function_candidates(payload, member, origin);
            match candidates.as_slice() {
                [(_, canonical)] => canonical.clone(),
                [_, _, ..] => {
                    if !groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .any(|argument| argument.label.is_some())
                    {
                        return None;
                    }
                    let canonicals = candidates
                        .iter()
                        .map(|(_, canonical)| canonical.clone())
                        .collect::<Vec<_>>();
                    let matches = self.matching_function_overloads(&canonicals, groups, 1);
                    let [selected] = matches.as_slice() else {
                        return None;
                    };
                    selected.clone()
                }
                [] => return None,
            }
        };
        let signature = self.signatures.get(&canonical)?;
        if signature.groups.len() != groups.len() + 1
            || signature.groups.first()?.len() != 1
            || signature.groups[0][0].mode == PassMode::MutBorrow
            || groups
                .iter()
                .zip(signature.groups.iter().skip(1))
                .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            return None;
        }
        signature.result.clone()
    }

    pub(super) fn custom_chain_plan_for_ty(
        &self,
        container: &Ty,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        origin: &ItemOrigin,
    ) -> Option<CustomChainPlan> {
        let chain_trait = self.lang_item_name(LangItemKind::Chain);
        let candidates = self
            .trait_method_candidates(container, "chain", origin)
            .into_iter()
            .filter(|key| key.trait_ref.name == chain_trait)
            .collect::<Vec<_>>();
        let [key] = candidates.as_slice() else {
            return None;
        };
        let implementation = self.trait_impls.get(key)?;
        let item_ty = implementation.associated_types.get("Item")?;
        let item_source = implementation
            .associated_type_sources
            .get("Item")
            .cloned()
            .or_else(|| self.source_type_for_ty(item_ty))?;
        let output_ty = self.probe_chain_access_ty(item_ty, member, groups, origin)?;
        if matches!(output_ty, Ty::Function(_)) {
            return None;
        }
        let output_source = self.source_type_for_ty(&output_ty)?;
        let Type::Named(rebind, arguments) =
            implementation.associated_type_sources.get("Rebind")?
        else {
            return None;
        };
        let mut result_arguments = arguments.clone();
        result_arguments.push(output_source.clone());
        let mut result_source = Type::Named(rebind.clone(), result_arguments);
        let mut alias_diagnostics = Vec::new();
        expand_alias_type(
            &mut result_source,
            &self.type_aliases,
            &mut Vec::new(),
            &mut alias_diagnostics,
        );
        if !alias_diagnostics.is_empty() {
            return None;
        }
        let result_ty = self.probe_source_ty(&result_source)?;
        Some(CustomChainPlan {
            item_source,
            output_source,
            result_ty,
            result_source,
        })
    }

    pub(super) fn probe_chain_ty(
        &self,
        base: &Expr,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        if matches!(base, Expr::Borrow { .. }) {
            return TypeProbe::Unsupported;
        }
        if place_root_name(base)
            .and_then(|name| context.lookup(name))
            .is_some_and(|local| local.capability != LocalCapability::Owned)
        {
            return TypeProbe::Unsupported;
        }
        let base_probe = self.probe_expr_ty(base, None, context);
        if self.standard_fallible_info_for_probe(&base_probe).is_none() {
            let base_ty = match &base_probe {
                TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => Some(ty),
                TypeProbe::Defaultable(_) | TypeProbe::Unsupported => None,
            };
            if let Some(plan) = base_ty
                .and_then(|ty| self.custom_chain_plan_for_ty(ty, member, groups, &context.origin))
            {
                return TypeProbe::KnownSource(plan.result_ty, plan.result_source);
            }
        }
        let info = self
            .standard_fallible_info_for_probe(&base_probe)
            .or_else(|| {
                let expected = self.standard_fallible_info_for_ty(expected?)?;
                let inferred = self.inferred_standard_coalesce_lhs(base, context)?;
                if inferred.kind != expected.kind {
                    return None;
                }
                Some(StandardFallibleInfo {
                    kind: inferred.kind,
                    payload: self.inferred_try_payload(&inferred, context)?,
                    payload_source: None,
                    error: expected.error,
                })
            });
        let Some(info) = info else {
            return TypeProbe::Unsupported;
        };
        let Some(output) =
            self.probe_chain_access_ty(&info.payload, member, groups, &context.origin)
        else {
            return TypeProbe::Unsupported;
        };
        let mut arguments = Vec::new();
        if info.kind == StandardFallibleKind::Result {
            arguments.push(info.error.expect("Result probe has an error type"));
        }
        arguments.push(output);
        let Some(source_arguments) = arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>()
        else {
            return TypeProbe::Unsupported;
        };
        let template = self.fallible_type_name(info.kind).to_owned();
        let key = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: template.to_owned(),
            arguments,
        };
        let canonical = self
            .nominal_instance_names
            .get(&key)
            .cloned()
            .unwrap_or_else(|| nominal_instance_name(&key));
        TypeProbe::KnownSource(
            Ty::Enum(canonical),
            Type::Named(template.to_owned(), source_arguments),
        )
    }

    pub(super) fn chain_access_ty(
        &mut self,
        payload: &Ty,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        origin: &ItemOrigin,
    ) -> Option<Ty> {
        let target = match payload {
            Ty::Struct(name) | Ty::Enum(name) => name.clone(),
            ty => {
                self.error(format!(
                    "optional chaining requires a nominal payload, found `{ty}`"
                ));
                return None;
            }
        };
        let Some(groups) = groups else {
            let Some(layout) = self.struct_layouts.get(&target) else {
                self.error(format!(
                    "optional field access requires a struct payload, found `{payload}`"
                ));
                return None;
            };
            let Some(field) = layout
                .fields
                .iter()
                .find(|field| field.name == member)
                .cloned()
            else {
                if self
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.methods.contains_key(member))
                {
                    self.error(format!(
                        "optional method `{target}.{member}` must be fully called"
                    ));
                } else {
                    self.error(format!(
                        "unknown field `{member}` on optional payload `{target}`"
                    ));
                }
                return None;
            };
            if !self.require_field_access(&target, &field, origin) {
                return None;
            }
            return Some(field.ty.clone());
        };

        let overload_key = (target.clone(), member.to_owned(), true);
        let inherent = if self.inherent_overloads.contains_key(&overload_key) {
            self.resolve_inherent_overload(&target, member, true, groups)
        } else {
            self.inherent_members
                .get(&target)
                .and_then(|members| members.methods.get(member))
                .cloned()
        };
        if self.inherent_overloads.contains_key(&overload_key) && inherent.is_none() {
            return None;
        }
        let canonical = if let Some(canonical) = inherent {
            canonical
        } else {
            let candidates = self.trait_method_function_candidates(payload, member, origin);
            match candidates.as_slice() {
                [(candidate, _)] if self.is_drop_impl(candidate) => {
                    self.error("`Drop.drop` cannot be called directly; destruction is automatic");
                    return None;
                }
                [(_, canonical)] => canonical.clone(),
                [] => {
                    if self.struct_layouts.get(&target).is_some_and(|layout| {
                        layout.fields.iter().any(|field| field.name == member)
                    }) {
                        self.error(format!(
                            "optional chaining cannot call field `{target}.{member}`"
                        ));
                    } else if self.has_inaccessible_trait_method(payload, member, origin) {
                        self.error(format!(
                            "trait method `{member}` on optional payload `{target}` is private or package-visible from another package"
                        ));
                    } else {
                        self.error(format!(
                            "unknown method `{member}` on optional payload `{target}`"
                        ));
                    }
                    return None;
                }
                [_, _, ..] => {
                    if !groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .any(|argument| argument.label.is_some())
                    {
                        self.error(format!(
                            "ambiguous trait method `{member}` on optional payload `{target}`; named arguments are required to select an overload"
                        ));
                        return None;
                    }
                    let canonicals = candidates
                        .iter()
                        .map(|(_, canonical)| canonical.clone())
                        .collect::<Vec<_>>();
                    let matches = self.matching_function_overloads(&canonicals, groups, 1);
                    let [selected] = matches.as_slice() else {
                        self.error(format!(
                            "trait method overload `{member}` on optional payload `{target}` is not uniquely selected"
                        ));
                        return None;
                    };
                    selected.clone()
                }
            }
        };
        let function_ty = self.function_type(&canonical);
        let signature = self.signatures[&canonical].clone();
        let Some(receiver) = signature.groups.first().and_then(|group| group.first()) else {
            self.error(format!(
                "internal error: optional method `{target}.{member}` has no receiver"
            ));
            return None;
        };
        if receiver.mode == PassMode::MutBorrow {
            self.error(format!(
                "optional chaining does not support mutable-borrow receiver `{target}.{member}`"
            ));
            return None;
        }
        let supplied = groups.len() + 1;
        if supplied != signature.groups.len() {
            if supplied < signature.groups.len() {
                self.error(format!(
                    "optional method `{target}.{member}` must be fully applied"
                ));
            } else {
                self.error(format!(
                    "too many parameter groups in optional method call `{target}.{member}`"
                ));
            }
            return None;
        }
        let Ty::Function(function_ty) = function_ty else {
            return None;
        };
        Some((*function_ty.result).clone())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_handler_chain_call(
        &mut self,
        source_scrutinee: &Expr,
        payload: &str,
        error: &str,
        member: &str,
        source_groups: &[Vec<CallArg>],
        source_success: &Expr,
        source_residual: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let borrowed = matches!(source_scrutinee, Expr::Borrow { .. })
            || place_root_name(source_scrutinee)
                .and_then(|name| context.lookup(name))
                .is_some_and(|local| local.capability != LocalCapability::Owned);
        let scrutinee = self.lower_expr(source_scrutinee, None, context);
        if borrowed {
            self.error("optional chaining requires an owned `Option` or `Result` value");
            return error_expr();
        }
        if scrutinee.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.standard_fallible_info_for_ty(&scrutinee.ty) else {
            self.error(format!(
                "operator `?.` requires an owned `Option(T)` or `Result(E)(T)`, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let groups = source_groups.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let Some(output) =
            self.chain_access_ty(&info.payload, member, Some(&groups), &context.origin)
        else {
            return error_expr();
        };
        if matches!(output, Ty::Function(_)) {
            self.error(
                "optional chaining does not support partial applications or callable fields",
            );
            return error_expr();
        }
        let template = self.fallible_type_name(info.kind).to_owned();
        let mut arguments = Vec::new();
        if info.kind == StandardFallibleKind::Result {
            arguments.push(info.error.clone().expect("Result has an error type"));
        }
        arguments.push(output);
        let Some(source_arguments) = arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>()
        else {
            self.error("optional chaining result cannot be represented as a standard container");
            return error_expr();
        };
        let Some(canonical) =
            self.ensure_nominal_instance(NominalKind::Enum, &template, source_arguments, arguments)
        else {
            return error_expr();
        };
        let success_variant = match info.kind {
            StandardFallibleKind::Option => "Some",
            StandardFallibleKind::Result => "Ok",
        };
        let mut success = source_success.clone();
        rewrite_handler_chain_wrappers(
            &mut success,
            &canonical,
            success_variant,
            match info.kind {
                StandardFallibleKind::Option => "None",
                StandardFallibleKind::Result => "Err",
            },
        );
        let mut residual = source_residual.clone();
        rewrite_handler_chain_wrappers(
            &mut residual,
            &canonical,
            success_variant,
            match info.kind {
                StandardFallibleKind::Option => "None",
                StandardFallibleKind::Result => "Err",
            },
        );
        let mut arms = vec![MatchArm {
            pattern: Pattern::Constructor {
                path: vec![success_variant.to_owned()],
                fields: PatternFields::Positional(vec![Pattern::Binding(payload.to_owned())]),
            },
            guard: None,
            body: success,
        }];
        arms.push(match info.kind {
            StandardFallibleKind::Option => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["None".to_owned()],
                    fields: PatternFields::Unit,
                },
                guard: None,
                body: residual,
            },
            StandardFallibleKind::Result => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["Err".to_owned()],
                    fields: PatternFields::Positional(vec![Pattern::Binding(error.to_owned())]),
                },
                guard: None,
                body: residual,
            },
        });
        self.lower_match_with_scrutinee(scrutinee, &arms, expected, context)
    }

    pub(super) fn lower_chain(
        &mut self,
        base: &Expr,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let borrowed = matches!(base, Expr::Borrow { .. })
            || place_root_name(base)
                .and_then(|name| context.lookup(name))
                .is_some_and(|local| local.capability != LocalCapability::Owned);
        if borrowed {
            self.error("optional chaining requires an owned `Option`, `Result`, or `Chain` value");
            return error_expr();
        }
        let base_probe = self.probe_expr_ty(base, None, context);
        if self.standard_fallible_info_for_probe(&base_probe).is_none() {
            let materialized;
            let base_ty = match &base_probe {
                TypeProbe::Known(ty) => Some(ty),
                TypeProbe::KnownSource(ty, source) => {
                    materialized = self.lower_source_type(source);
                    if materialized == Ty::Error {
                        Some(ty)
                    } else {
                        Some(&materialized)
                    }
                }
                TypeProbe::Defaultable(_) | TypeProbe::Unsupported => None,
            };
            if let Some(plan) = base_ty
                .and_then(|ty| self.custom_chain_plan_for_ty(ty, member, groups, &context.origin))
            {
                return self.lower_custom_chain_call(base, member, groups, plan, expected, context);
            }
        }
        let base_expected = expected.and_then(|expected| {
            let expected = self.standard_fallible_info_for_ty(expected)?;
            let inferred = self.inferred_standard_coalesce_lhs(base, context)?;
            if inferred.kind != expected.kind {
                return None;
            }
            let payload = self.inferred_try_payload(&inferred, context)?;
            let mut arguments = Vec::new();
            if inferred.kind == StandardFallibleKind::Result {
                arguments.push(expected.error?);
            }
            arguments.push(payload);
            let source_arguments = arguments
                .iter()
                .map(|argument| self.source_type_for_ty(argument))
                .collect::<Option<Vec<_>>>()?;
            let canonical = self.ensure_nominal_instance(
                NominalKind::Enum,
                &inferred.name,
                source_arguments,
                arguments,
            )?;
            Some(Ty::Enum(canonical))
        });
        let scrutinee = self.lower_expr(base, base_expected.as_ref(), context);
        if scrutinee.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.standard_fallible_info_for_ty(&scrutinee.ty) else {
            self.error(format!(
                "operator `?.` requires an owned `Option(T)`, `Result(E)(T)`, or `Chain` value, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let Some(output) = self.chain_access_ty(&info.payload, member, groups, &context.origin)
        else {
            return error_expr();
        };
        if matches!(output, Ty::Function(_)) {
            self.error(
                "optional chaining does not support partial applications or callable fields",
            );
            return error_expr();
        }
        let template = self.fallible_type_name(info.kind).to_owned();
        let mut arguments = Vec::new();
        if info.kind == StandardFallibleKind::Result {
            arguments.push(info.error.clone().expect("Result has an error type"));
        }
        arguments.push(output);
        let Some(source_arguments) = arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>()
        else {
            self.error("optional chaining result cannot be represented as a standard container");
            return error_expr();
        };
        let Some(canonical) =
            self.ensure_nominal_instance(NominalKind::Enum, &template, source_arguments, arguments)
        else {
            return error_expr();
        };

        const PAYLOAD_BINDING: &str = "$chain$payload";
        const ERROR_BINDING: &str = "$chain$error";
        let mut access = Expr::Member(
            Box::new(Expr::Name(PAYLOAD_BINDING.to_owned())),
            member.to_owned(),
        );
        if let Some(groups) = groups {
            for arguments in groups {
                access = Expr::Call(Box::new(access), arguments.to_vec());
            }
        }
        let wrap = |variant: &str, value: Option<Expr>| {
            let member = Expr::Member(Box::new(Expr::Name(canonical.clone())), variant.to_owned());
            value.map_or(member.clone(), |value| {
                Expr::Call(Box::new(member), vec![CallArg { label: None, value }])
            })
        };
        let success_variant = match info.kind {
            StandardFallibleKind::Option => "Some",
            StandardFallibleKind::Result => "Ok",
        };
        let mut arms = vec![MatchArm {
            pattern: Pattern::Constructor {
                path: vec![success_variant.to_owned()],
                fields: PatternFields::Positional(vec![Pattern::Binding(
                    PAYLOAD_BINDING.to_owned(),
                )]),
            },
            guard: None,
            body: wrap(success_variant, Some(access)),
        }];
        arms.push(match info.kind {
            StandardFallibleKind::Option => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["None".to_owned()],
                    fields: PatternFields::Unit,
                },
                guard: None,
                body: wrap("None", None),
            },
            StandardFallibleKind::Result => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["Err".to_owned()],
                    fields: PatternFields::Positional(vec![Pattern::Binding(
                        ERROR_BINDING.to_owned(),
                    )]),
                },
                guard: None,
                body: wrap("Err", Some(Expr::Name(ERROR_BINDING.to_owned()))),
            },
        });
        self.lower_match_with_scrutinee(scrutinee, &arms, Some(&Ty::Enum(canonical)), context)
    }

    pub(super) fn lower_custom_chain_call(
        &mut self,
        base: &Expr,
        member: &str,
        groups: Option<&[&[CallArg]]>,
        plan: CustomChainPlan,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        const PAYLOAD_BINDING: &str = "$chain$payload";
        let mut access = Expr::Member(
            Box::new(Expr::Name(PAYLOAD_BINDING.to_owned())),
            member.to_owned(),
        );
        if let Some(groups) = groups {
            for arguments in groups {
                access = Expr::Call(Box::new(access), arguments.to_vec());
            }
        }
        let transform = Expr::Closure(
            vec![Param {
                mode: PassMode::Inferred,
                access: None,
                passing: None,
                region: None,
                name: PAYLOAD_BINDING.to_owned(),
                ty: plan.item_source,
            }],
            Box::new(access),
        );
        let callee = Expr::Call(
            Box::new(Expr::Member(
                Box::new(base.clone()),
                "$lang$chain".to_owned(),
            )),
            vec![
                CallArg {
                    label: None,
                    value: Expr::Name("pure".to_owned()),
                },
                CallArg {
                    label: None,
                    value: source_type_expression(&plan.output_source),
                },
            ],
        );
        let call = Expr::Call(
            Box::new(callee),
            vec![CallArg {
                label: None,
                value: transform,
            }],
        );
        self.lower_expr(&call, expected, context)
    }
}
