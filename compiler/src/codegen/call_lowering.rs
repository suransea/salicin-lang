use std::collections::{HashMap, HashSet};

use crate::ast::{
    Binding, CallArg, Expr, MatchArm, Param, PassMode, Pattern, Stmt, Type, Visibility,
};
use crate::core::LangItemKind;

use super::calls::{
    rewrite_callable_bridge_groups, CallableBridgeKey, CallableBridgeSpecialization,
};
use super::compile_time::{
    effect_identity_sources, source_effect_identity, source_effect_source_map,
};
use super::flow::{InitializationStatus, LoanKind, LocalInfo, LowerCtx};
use super::handlers::{inject_handler_action_binding, internal_stored_callable_capture};
use super::hir::{
    type_is_assignable, AccessBoundary, AccessKind, CallableKind, ClosureCaptureMode,
    ClosureCapturePolicy, ClosureCaptureUse, ClosureEffectContext, FunctionSig, FunctionTy,
    HirArgument, HirBinding, HirExpr, HirExprKind, HirPlace, HirReadKind, HirStmt, LoanId,
    LocalCapability, ParamSig, Ty, CHECKED_INTEGER_CONVERSION_INTRINSIC,
    INTEGER_MAGNITUDE_INTRINSIC,
};
use super::lower::{
    contextual_reference_result, error_expr, flatten_call, partial_callable_ty,
    BoundMethodConstraint, TypeProbe,
};
use super::names::hex_name;
use super::registry::NominalKind;
use super::source_rewrite::{erase_expr_locations, rewrite_static_function_values};
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_nominal_type_member_call(
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
            if self
                .collection
                .inherent_overloads
                .contains_key(&overload_key)
            {
                let Some(canonical) = self.resolve_inherent_overload(target, member, false, groups)
                else {
                    return error_expr();
                };
                if self.collection.function_templates.contains_key(&canonical) {
                    return self.lower_generic_function_call(&canonical, groups, expected, context);
                }
                return self.lower_named_function_call(&canonical, groups, expected, context);
            }
            if let Some(canonical) = self
                .collection
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
                    if self.collection.function_templates.contains_key(canonical) {
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
                            if self.collection.function_templates.contains_key(canonical) {
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
                .collection
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
            .collection
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

    pub(super) fn lower_constructor_trait_associated_function_call(
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
        if self.collection.function_templates.contains_key(&canonical) {
            Some(self.lower_generic_function_call(&canonical, groups, expected, context))
        } else {
            Some(self.lower_named_function_call(&canonical, groups, expected, context))
        }
    }

    pub(super) fn lower_generic_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if self.is_lang_item_name(name, LangItemKind::Match) {
            return self.lower_pattern_match_call(groups, expected, context);
        }
        let Some((canonical, runtime_start)) =
            self.resolve_inferred_generic_function_instance(name, groups, expected, context)
        else {
            return error_expr();
        };
        self.lower_named_function_call(&canonical, &groups[runtime_start..], expected, context)
    }

    pub(super) fn lower_pattern_match_call(
        &mut self,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some((input_group, case_groups)) = groups.split_first() else {
            self.error("`match` requires an input group");
            return error_expr();
        };
        let [input] = *input_group else {
            self.error("`match` input group requires exactly one unlabeled argument");
            return error_expr();
        };
        if input.label.is_some() {
            self.error("`match` input must be unlabeled");
        }
        let mut arms = Vec::with_capacity(case_groups.len());
        for (index, group) in case_groups.iter().enumerate() {
            let [case] = *group else {
                self.error(format!(
                    "`match` case group {} requires exactly one pattern closure",
                    index + 1
                ));
                return error_expr();
            };
            if case.label.is_some() {
                self.error(format!(
                    "`match` case group {} must be unlabeled",
                    index + 1
                ));
            }
            let Expr::PatternClosure {
                pattern,
                guard,
                body,
            } = &case.value
            else {
                self.error(format!(
                    "`match` case group {} requires `{{ Pattern [if guard] -> body }}`",
                    index + 1
                ));
                return error_expr();
            };
            arms.push(MatchArm {
                pattern: pattern.clone(),
                guard: guard.as_deref().cloned(),
                body: (**body).clone(),
            });
        }
        self.lower_match(&input.value, &arms, expected, context)
    }

    pub(super) fn lower_if_match_call(
        &mut self,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(Expr::Match { scrutinee, arms }) = self.if_match_call_expression(groups) else {
            self.error(
                "`if` requires `(condition)(then closure)(else closure)` with zero-parameter branches",
            );
            return error_expr();
        };
        self.lower_match(&scrutinee, &arms, expected, context)
    }

    pub(super) fn if_match_call_expression(&self, groups: &[&[CallArg]]) -> Option<Expr> {
        let [condition_group, then_group, else_group] = groups else {
            return None;
        };
        let [condition] = *condition_group else {
            return None;
        };
        if !matches!(condition.label.as_deref(), None | Some("condition")) {
            return None;
        }
        let branch = |group: &[CallArg], label: &str| {
            let [argument] = group else {
                return None;
            };
            if argument
                .label
                .as_deref()
                .is_some_and(|found| found != label)
            {
                return None;
            }
            let Expr::Closure(parameters, body) = &argument.value else {
                return None;
            };
            if !parameters.is_empty() {
                return None;
            }
            Some((**body).clone())
        };
        Some(Expr::Match {
            scrutinee: Box::new(condition.value.clone()),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Bool(true),
                    guard: None,
                    body: branch(then_group, "then")?,
                },
                MatchArm {
                    pattern: Pattern::Bool(false),
                    guard: None,
                    body: branch(else_group, "else")?,
                },
            ],
        })
    }

    pub(super) fn pattern_match_call_expression(&self, expression: &Expr) -> Option<Expr> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        if !self.is_lang_item_name(name, LangItemKind::Match) {
            return None;
        }
        let (input_group, case_groups) = groups.split_first()?;
        let [CallArg {
            label: None,
            value: input,
        }] = *input_group
        else {
            return None;
        };
        let arms = case_groups
            .iter()
            .map(|group| {
                let [CallArg {
                    label: None,
                    value:
                        Expr::PatternClosure {
                            pattern,
                            guard,
                            body,
                        },
                }] = *group
                else {
                    return None;
                };
                Some(MatchArm {
                    pattern: pattern.clone(),
                    guard: guard.as_deref().cloned(),
                    body: (**body).clone(),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Expr::Match {
            scrutinee: Box::new(input.clone()),
            arms,
        })
    }

    pub(super) fn if_call_expression_for_transform(&self, expression: &Expr) -> Option<Expr> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        if !self.is_lang_item_name(name, LangItemKind::If) {
            return None;
        }
        let Expr::Match { scrutinee, arms } = self.if_match_call_expression(&groups)? else {
            return None;
        };
        let [then_arm, else_arm] = arms.as_slice() else {
            return None;
        };
        Some(Expr::If {
            condition: scrutinee,
            then_branch: Box::new(then_arm.body.clone()),
            else_branch: Some(Box::new(else_arm.body.clone())),
        })
    }

    pub(super) fn lower_bound_method_call(
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
                        dynamic_index: None,
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
        let receiver_ty = receiver_place.ty.clone();
        let target = match &receiver_ty {
            Ty::Struct(name) | Ty::Enum(name) => name.clone(),
            Ty::Pointer { .. } => self
                .ensure_pointer_inherent_extensions(&receiver_ty)
                .expect("pointer extension owner exists for pointer receiver"),
            Ty::Slice(_) => self
                .ensure_slice_inherent_extensions(&receiver_ty)
                .expect("slice extension owner exists for slice receiver"),
            Ty::Array(_, _) => {
                self.ensure_array_trait_extensions(&receiver_ty);
                receiver_ty.to_string()
            }
            ty if ty.is_integer() => ty.to_string(),
            ty => {
                self.error(format!(
                    "method call requires an extendable receiver, found `{ty}`"
                ));
                return error_expr();
            }
        };
        let target_display = self.diagnostic_type_name(&receiver_ty);
        if let Some(qualified_target) = qualified_target {
            if qualified_target != target {
                self.error(format!(
                    "qualified method `{qualified_target}.{member}` requires receiver `{qualified_target}`, found `{target_display}`"
                ));
                return error_expr();
            }
        }
        let overload_key = (target.clone(), member.to_owned(), true);
        let inherent = if forced_trait.is_some() {
            None
        } else if self
            .collection
            .inherent_overloads
            .contains_key(&overload_key)
        {
            self.resolve_inherent_overload(&target, member, true, groups)
        } else {
            self.collection
                .inherent_members
                .get(&target)
                .and_then(|members| members.methods.get(member))
                .cloned()
        };
        if forced_trait.is_none()
            && self
                .collection
                .inherent_overloads
                .contains_key(&overload_key)
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
                    self.error(
                        "`droppable.drop` cannot be called directly; destruction is automatic",
                    );
                    return error_expr();
                }
                if let Some((key, canonical)) = candidates.first() {
                    let implementation = &self.collection.trait_impls[key];
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
                        "ambiguous trait method `{member}` on `{target_display}` requires named arguments to select an overload"
                    ));
                    return error_expr();
                }
                let matches = self.matching_function_overloads(&canonicals, groups, 1);
                match matches.as_slice() {
                    [selected] => selected.clone(),
                    [] => {
                        self.error(format!(
                            "no trait method overload `{member}` on `{target_display}` matches the supplied named parameter groups"
                        ));
                        return error_expr();
                    }
                    _ => {
                        self.error(format!(
                            "trait method overload `{member}` on `{target_display}` remains ambiguous"
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
                    .collection
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target_display}.{member}` must be called on the type"
                    ));
                } else if self
                    .collection
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.constants.contains_key(member))
                {
                    self.error(format!(
                        "associated constant `{target_display}.{member}` must be accessed on the type"
                    ));
                } else if self.has_inaccessible_trait_method(
                    &receiver_place.ty,
                    member,
                    &context.origin,
                ) {
                    self.error(format!(
                        "trait method `{member}` on `{target_display}` is private or package-visible from another package"
                    ));
                } else {
                    self.error(format!("unknown method `{member}` on `{target_display}`"));
                }
                return error_expr();
            }
        };

        let mut runtime_groups = groups;
        if let Some(template) = self.collection.function_templates.get(&canonical).cloned() {
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

        let receiver_group = [CallArg {
            label: None,
            value: receiver.clone(),
        }];
        let mut full_runtime_groups = Vec::with_capacity(runtime_groups.len() + 1);
        full_runtime_groups.push(receiver_group.as_slice());
        full_runtime_groups.extend_from_slice(runtime_groups);
        let specialized_call =
            self.specialize_capturing_callable_call(&canonical, &full_runtime_groups, context);
        let specialized_runtime_groups = specialized_call
            .as_ref()
            .map(|(_, rewritten)| rewritten[1..].to_vec())
            .unwrap_or_default();
        let specialized_runtime_group_refs = specialized_runtime_groups
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        if let Some((specialized, _)) = specialized_call {
            canonical = specialized;
            runtime_groups = &specialized_runtime_group_refs;
        }

        let function_ty = self.function_type(&canonical);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.lowering.signatures[&canonical].clone();
        if signature.groups.len() == 1 && matches!(runtime_groups, [group] if group.is_empty()) {
            runtime_groups = &runtime_groups[..0];
        }
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
            let receiver_mode =
                self.effective_pass_mode(receiver_parameter.mode, &receiver_parameter.ty);
            if matches!(receiver_parameter.ty, Ty::Reference { .. }) {
                if receiver_mode == PassMode::MutBorrow || receiver_mode == PassMode::Move {
                    receiver_place.root_mutable = true;
                    if let Some(binding) = temporary_binding.as_mut() {
                        binding.mutable = true;
                    }
                }
                let Some(argument) = self.lower_reference_place_call_argument(
                    receiver_place.clone(),
                    receiver_parameter,
                    &receiver_parameter.ty,
                    context,
                    &mut temporary_loans,
                ) else {
                    self.require_same_type(
                        &receiver_place.ty,
                        &receiver_parameter.ty,
                        format!("receiver for method `{target}.{member}`"),
                    );
                    return error_expr();
                };
                argument
            } else {
                self.require_same_type(
                    &receiver_place.ty,
                    &receiver_parameter.ty,
                    format!("receiver for method `{target}.{member}`"),
                );
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
                            "receiver for method `{target}.{member}` requires copyable, but `{ty}` does not implement copyable"
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
            let lowered_function = if self
                .collection
                .integer_conversion_intrinsics
                .contains_key(&canonical)
            {
                CHECKED_INTEGER_CONVERSION_INTRINSIC.to_owned()
            } else if self
                .collection
                .integer_magnitude_intrinsics
                .contains_key(&canonical)
            {
                INTEGER_MAGNITUDE_INTRINSIC.to_owned()
            } else {
                canonical.clone()
            };
            let call = HirExpr {
                ty: if function_ty.failure_error.is_some() {
                    (*function_ty.result).clone()
                } else {
                    contextual_reference_result(&function_ty.result, expected)
                },
                kind: HirExprKind::Call {
                    function: lowered_function,
                    arguments: arguments.clone(),
                    consumed_callable: None,
                    diverges: self.is_uninhabited_type(&function_ty.result),
                },
            };
            if let Some(error) = function_ty.failure_error.as_deref() {
                self.lower_automatic_throwing(call, error, expected, context)
            } else {
                call
            }
        } else {
            let callable_ty = partial_callable_ty(
                canonical.clone(),
                consumed_groups,
                FunctionTy {
                    groups: function_ty.groups[consumed_groups..].to_vec(),
                    unsafety: function_ty.unsafety,
                    failure_error: function_ty.failure_error.clone(),
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

    pub(super) fn lower_local_closure_call(
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
        let complete = groups.len() == closure.groups.len();
        if complete {
            let sources =
                source_effect_source_map(&effect_identity_sources(&closure.custom_effects));
            self.require_callable_effects(
                if closure.unsafety {
                    format!("call to unsafe closure `{local_name}`")
                } else {
                    format!("call to closure `{local_name}`")
                },
                closure.unsafety,
                &closure.custom_effects,
                &sources,
                context,
            );
        }

        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
            ty: local.ty.clone(),
            capability: LocalCapability::Owned,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        let leaves = self.place_leaf_keys(&callable);
        let callable_kind = if closure.is_fn_once {
            "fn_once closure"
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
        let closure_capture_count = lowered_arguments.len();
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
        if !complete
            && lowered_arguments[closure_capture_count..]
                .iter()
                .any(|argument| {
                    matches!(
                        argument,
                        HirArgument::SharedBorrow(_) | HirArgument::MutBorrow(_)
                    ) || matches!(argument, HirArgument::Copy(value) | HirArgument::Move(value) if matches!(value.ty, Ty::Reference { .. }))
                })
        {
            self.error("partial application cannot capture borrowed arguments");
        }
        self.release_loans(&temporary_loans, context);
        let call = if complete {
            HirExpr {
                ty: closure.result.clone(),
                kind: HirExprKind::Call {
                    function: closure.function.clone(),
                    arguments: lowered_arguments.clone(),
                    consumed_callable: closure.is_fn_once.then_some(local.id),
                    diverges: self.is_uninhabited_type(&closure.result),
                },
            }
        } else {
            let consumed_groups = groups.len();
            self.lowering.partial_parameter_shapes.insert(
                (closure.function.clone(), consumed_groups),
                closure.groups[consumed_groups..].to_vec(),
            );
            let callable_ty = partial_callable_ty(
                closure.function.clone(),
                consumed_groups,
                FunctionTy {
                    groups: closure.groups[consumed_groups..]
                        .iter()
                        .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                        .collect(),
                    unsafety: closure.unsafety,
                    failure_error: closure.failure_error.clone().map(Box::new),
                    custom_effects: closure.custom_effects.clone(),
                    result: Box::new(closure.result.clone()),
                },
                &lowered_arguments,
            );
            HirExpr {
                ty: callable_ty,
                kind: HirExprKind::Partial {
                    function: closure.function.clone(),
                    consumed_groups,
                    captures: lowered_arguments.clone(),
                },
            }
        };
        let call = if complete {
            if let Some(error) = closure.failure_error.as_ref() {
                self.lower_automatic_throwing(call, error, expected, context)
            } else {
                call
            }
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

    pub(super) fn lower_indirect_function_call(
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
        let sources =
            source_effect_source_map(&effect_identity_sources(&function_ty.custom_effects));
        self.require_callable_effects(
            if function_ty.unsafety {
                format!("indirect call to unsafe callable `{local_name}`")
            } else {
                format!("indirect call `{local_name}`")
            },
            function_ty.unsafety,
            &function_ty.custom_effects,
            &sources,
            context,
        );

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
            dynamic_index: None,
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
        let call = if let Some(error) = function_ty.failure_error.as_deref() {
            self.lower_automatic_throwing(call, error, expected, context)
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

    pub(super) fn lower_named_function_call(
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
        if let Some((specialized, specialized_groups)) =
            self.specialize_capturing_callable_call(name, groups, context)
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
        let signature = self.lowering.signatures[name].clone();
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
                if let Some(action) = self.lowering.runtime_handler_actions.get(&(
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
                    if let Expr::Name(local_name) = &argument.value {
                        if context
                            .lookup(local_name)
                            .is_some_and(|local| local.ty == expected_action)
                        {
                            let place = self
                                .lower_place(&argument.value, context)
                                .expect("a resolved erased action local is a place");
                            let value = self.access_place(place, AccessKind::Move, context);
                            arguments.push(HirArgument::Move(value));
                            continue;
                        }
                    }
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
                ty: if function_ty.failure_error.is_some() {
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
            if let Some(error) = function_ty.failure_error.as_deref() {
                self.lower_automatic_throwing(call, error, expected, context)
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
                    unsafety: function_ty.unsafety,
                    failure_error: function_ty.failure_error.clone(),
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

    pub(super) fn specialize_capturing_callable_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<(String, Vec<Vec<CallArg>>)> {
        let function = self.collection.functions.get(name)?.clone();
        if function.body.is_none() || groups.len() > function.groups.len() {
            return None;
        }
        for (group_index, (arguments, parameters)) in
            groups.iter().zip(&function.groups).enumerate()
        {
            if arguments.len() != parameters.len() {
                continue;
            }
            for (parameter_index, (argument, parameter)) in
                arguments.iter().zip(parameters).enumerate()
            {
                let Type::Function { .. } = &parameter.ty else {
                    continue;
                };
                let Expr::Closure(closure_parameters, closure_body) = &argument.value else {
                    continue;
                };
                let captures =
                    self.closure_literal_capture_uses(closure_parameters, closure_body, context)?;
                if captures.is_empty() {
                    continue;
                }

                let mut capture_parameters = Vec::with_capacity(captures.len());
                let mut shape_replacements = HashMap::new();
                for (offset, capture) in captures.iter().enumerate() {
                    let local = context
                        .lookup(&capture.name)
                        .expect("capture scanner records visible locals");
                    let source_ty = self.source_type_for_ty(&local.ty)?;
                    let mode = match capture.mode {
                        ClosureCaptureMode::Shared => PassMode::Borrow,
                        ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                        ClosureCaptureMode::Move => PassMode::Move,
                    };
                    shape_replacements.insert(
                        capture.name.clone(),
                        format!("$callable$shape$capture${offset}"),
                    );
                    capture_parameters.push((capture.name.clone(), mode, source_ty));
                }
                let mut closure_shape = argument.value.clone();
                rewrite_static_function_values(&mut closure_shape, &shape_replacements);
                erase_expr_locations(&mut closure_shape);
                let key = CallableBridgeKey {
                    callee: name.to_owned(),
                    group: group_index,
                    parameter: parameter_index,
                    closure_shape: format!("{closure_shape:?}"),
                    captures: capture_parameters
                        .iter()
                        .map(|(_, mode, ty)| (*mode, ty.clone()))
                        .collect(),
                };
                if let Some(cached) = self.lowering.callable_bridge_specializations.get(&key) {
                    let rewritten = rewrite_callable_bridge_groups(
                        groups,
                        group_index,
                        parameter_index,
                        &cached.lifted_parameters,
                        &capture_parameters,
                    );
                    return Some((cached.canonical.clone(), rewritten));
                }

                let specialization = self.lowering.next_closure;
                self.lowering.next_closure += 1;
                let canonical = format!("{name}$callable$bridge${specialization}");
                let mut specialized = function.clone();
                specialized.name = canonical.clone();
                let callable = specialized.groups[group_index].remove(parameter_index);
                let mut replacements = HashMap::new();
                let mut lifted_parameters = Vec::new();
                for (offset, (source, mode, source_ty)) in capture_parameters.iter().enumerate() {
                    let lifted = format!("$callable$capture${specialization}${offset}");
                    replacements.insert(source.clone(), lifted.clone());
                    specialized.groups[group_index].insert(
                        parameter_index + offset,
                        Param {
                            mode: *mode,
                            access: None,
                            modifiers: Vec::new(),
                            region: None,
                            name: lifted.clone(),
                            ty: source_ty.clone(),
                        },
                    );
                    lifted_parameters.push(lifted);
                }

                let mut closure = argument.value.clone();
                rewrite_static_function_values(&mut closure, &replacements);
                let body = specialized
                    .body
                    .take()
                    .expect("bodyless functions are not specialized");
                specialized.body = Some(Expr::Block(
                    vec![Stmt::Let(Binding {
                        value_source: None,
                        mutable: captures
                            .iter()
                            .any(|capture| capture.mode == ClosureCaptureMode::Mutable),
                        name: callable.name,
                        annotation: Some(callable.ty),
                        value: closure,
                    })],
                    Some(Box::new(body)),
                ));

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
                    unsafety: self.function_effects_unsafe(&specialized.effects),
                    failure_error: specialized
                        .effects
                        .failure
                        .as_deref()
                        .map(|error| self.lower_source_type(error)),
                    custom_effects: self.function_effects_custom_identities(&specialized.effects),
                    result: specialized
                        .return_type
                        .as_ref()
                        .map(|result| self.lower_source_type(result)),
                };
                self.collection
                    .functions
                    .insert(canonical.clone(), specialized);
                self.lowering
                    .signatures
                    .insert(canonical.clone(), signature);
                if let Some(origin) = self.collection.function_origins.get(name).cloned() {
                    self.collection
                        .function_origins
                        .insert(canonical.clone(), origin);
                }
                if let Some(access) = self.collection.function_accesses.get(name).cloned() {
                    self.collection
                        .function_accesses
                        .insert(canonical.clone(), access);
                }
                self.collection.function_order.push(canonical.clone());
                self.lowering.callable_bridge_specializations.insert(
                    key,
                    CallableBridgeSpecialization {
                        canonical: canonical.clone(),
                        lifted_parameters: lifted_parameters.clone(),
                    },
                );
                let rewritten_groups = rewrite_callable_bridge_groups(
                    groups,
                    group_index,
                    parameter_index,
                    &lifted_parameters,
                    &capture_parameters,
                );
                return Some((canonical, rewritten_groups));
            }
        }
        None
    }

    pub(super) fn specialize_static_handler_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> Option<(String, Vec<Vec<CallArg>>)> {
        if let Some(specialized) = self.specialize_stored_handler_action_call(name, groups, context)
        {
            return Some(specialized);
        }
        let mut function = self.collection.functions.get(name)?.clone();
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
                    self.collection
                        .effect_defs
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
                let target = if self.collection.functions.contains_key(source_target) {
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
                let Some(target_function) = self.collection.functions.get(&target) else {
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
        if !self.collection.functions.contains_key(&canonical) {
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
                unsafety: self.function_effects_unsafe(&function.effects),
                failure_error: function
                    .effects
                    .failure
                    .as_deref()
                    .map(|error| self.lower_source_type(error)),
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result: function
                    .return_type
                    .as_ref()
                    .map(|result| self.lower_source_type(result)),
            };
            self.collection
                .functions
                .insert(canonical.clone(), function);
            self.lowering
                .signatures
                .insert(canonical.clone(), signature);
            self.collection.function_origins.insert(
                canonical.clone(),
                self.collection.function_origins[name].clone(),
            );
            let origin = self
                .collection
                .function_origins
                .get(name)
                .cloned()
                .unwrap_or_default();
            let access = self
                .collection
                .function_accesses
                .get(name)
                .cloned()
                .unwrap_or(AccessBoundary {
                    visibility: Visibility::Private,
                    origin,
                });
            self.collection
                .function_accesses
                .insert(canonical.clone(), access);
            self.collection.function_order.push(canonical.clone());
        }
        Some((canonical, specialized_groups))
    }

    pub(super) fn specialize_stored_handler_action_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> Option<(String, Vec<Vec<CallArg>>)> {
        let function = self.collection.functions.get(name)?.clone();
        if groups.len() != function.groups.len() {
            return None;
        }
        let mut selected = None;
        let mut action_positions = self
            .lowering
            .runtime_handler_actions
            .iter()
            .map(|(position, action)| (position.clone(), action.clone()))
            .collect::<Vec<_>>();
        action_positions.sort_by(|left, right| left.0.cmp(&right.0));
        for ((candidate, group_index, parameter_index), action) in action_positions {
            if candidate.as_str() != name {
                continue;
            }
            let arguments = groups.get(group_index).cloned()?;
            let parameter = function.groups.get(group_index)?.get(parameter_index)?;
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                parameter_index
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
                group_index,
                parameter_index,
                argument_index,
                action,
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

        let specialization = self.lowering.next_closure;
        self.lowering.next_closure += 1;
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
                    modifiers: Vec::new(),
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
            unsafety: self.function_effects_unsafe(&specialized.effects),
            failure_error: specialized
                .effects
                .failure
                .as_deref()
                .map(|error| self.lower_source_type(error)),
            custom_effects: self.function_effects_custom_identities(&specialized.effects),
            result: specialized
                .return_type
                .as_ref()
                .map(|result| self.lower_source_type(result)),
        };
        self.collection
            .functions
            .insert(canonical.clone(), specialized);
        self.lowering
            .signatures
            .insert(canonical.clone(), signature);
        self.collection.function_origins.insert(
            canonical.clone(),
            self.collection.function_origins[name].clone(),
        );
        let origin = self
            .collection
            .function_origins
            .get(name)
            .cloned()
            .unwrap_or_default();
        let access = self
            .collection
            .function_accesses
            .get(name)
            .cloned()
            .unwrap_or(AccessBoundary {
                visibility: Visibility::Private,
                origin,
            });
        self.collection
            .function_accesses
            .insert(canonical.clone(), access);
        self.collection.function_order.push(canonical.clone());
        self.lowering
            .lifted_functions
            .retain(|function| function.name != closure.function);
        self.lowering
            .continuation_adapters
            .retain(|adapter| adapter.function != closure.function);
        self.lowering
            .effect_callable_adapters
            .retain(|adapter| adapter.function != closure.function);

        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
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

    pub(super) fn lower_local_partial_call(
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
        let (function_ty, callable_captures, remaining_parameters) = match &local.ty {
            Ty::Callable(callable_ty) => {
                if !matches!(callable_ty.kind, CallableKind::Partial { .. }) {
                    self.error(format!(
                        "internal error: partial `{local_name}` has a non-partial callable type"
                    ));
                    return error_expr();
                }
                let parameters = self
                    .lowering
                    .signatures
                    .get(&partial.function)
                    .map(|signature| signature.groups[partial.consumed_groups..].to_vec())
                    .or_else(|| {
                        self.lowering
                            .partial_parameter_shapes
                            .get(&(partial.function.clone(), partial.consumed_groups))
                            .cloned()
                    });
                let Some(parameters) = parameters else {
                    self.error(format!(
                        "internal error: partial `{local_name}` has no remaining parameter metadata"
                    ));
                    return error_expr();
                };
                (
                    callable_ty.signature.clone(),
                    callable_ty.captures.clone(),
                    parameters,
                )
            }
            Ty::Function(function_ty) if partial.capture_count == 0 => (
                function_ty.clone(),
                Vec::new(),
                self.lowering.signatures[&partial.function].groups.clone(),
            ),
            _ => {
                self.error(format!(
                    "internal error: partial `{local_name}` has no callable type"
                ));
                return error_expr();
            }
        };
        let remaining_groups = remaining_parameters.len();
        if groups.len() > remaining_groups {
            self.error(format!(
                "too many parameter groups in call to `{local_name}`: expected at most {remaining_groups}, found {}",
                groups.len()
            ));
            return error_expr();
        }

        if callable_captures.len() != partial.capture_count {
            self.error(format!(
                "internal error: invalid capture count for partial `{local_name}`"
            ));
            return error_expr();
        }
        let callable = HirPlace {
            local: local.id,
            root_ty: local.ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
            ty: local.ty.clone(),
            capability: LocalCapability::Owned,
            root_mutable: local.mutable,
            loan: None,
            indirect: false,
        };
        let leaves = self.place_leaf_keys(&callable);
        let callable_kind = if partial.is_fn_once {
            "fn_once partial application"
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
        let mut arguments: Vec<_> = callable_captures
            .into_iter()
            .enumerate()
            .map(|(index, capture_ty)| {
                let capture = HirExpr {
                    ty: capture_ty.ty.clone(),
                    kind: HirExprKind::PartialCapture {
                        binding: local.id,
                        index,
                        moves: capture_ty.mode == PassMode::Move,
                        callable_ty: local.ty.clone(),
                    },
                };
                match capture_ty.mode {
                    PassMode::Copy => HirArgument::Copy(capture),
                    PassMode::Move => HirArgument::Move(capture),
                    PassMode::Borrow | PassMode::MutBorrow => HirArgument::CallableCaptureBorrow {
                        binding: local.id,
                        index,
                        callable_ty: local.ty.clone(),
                        capture_ty: capture_ty.ty,
                        mutable: capture_ty.mode == PassMode::MutBorrow,
                    },
                    PassMode::Inferred => {
                        unreachable!("callable capture mode is always explicit")
                    }
                }
            })
            .collect();
        let captured_argument_count = arguments.len();

        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        for (relative_group, arguments_ast) in groups.iter().enumerate() {
            let params = &remaining_parameters[relative_group];
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
        let complete = groups.len() == remaining_groups;
        if complete {
            let sources =
                source_effect_source_map(&effect_identity_sources(&function_ty.custom_effects));
            self.require_callable_effects(
                if function_ty.unsafety {
                    format!("call to unsafe partial `{local_name}`")
                } else {
                    format!("call to partial `{local_name}`")
                },
                function_ty.unsafety,
                &function_ty.custom_effects,
                &sources,
                context,
            );
        }
        if !complete
            && arguments[captured_argument_count..]
                .iter()
                .any(|argument| {
                matches!(
                    argument,
                    HirArgument::SharedBorrow(_) | HirArgument::MutBorrow(_)
                ) || matches!(argument, HirArgument::Copy(value) | HirArgument::Move(value) if matches!(value.ty, Ty::Reference { .. }))
            })
        {
            self.error("partial application cannot capture borrowed arguments");
        }
        self.release_loans(&temporary_loans, context);
        let call = if complete {
            let call = HirExpr {
                ty: (*function_ty.result).clone(),
                kind: HirExprKind::Call {
                    function: partial.function.clone(),
                    arguments: arguments.clone(),
                    consumed_callable: partial.is_fn_once.then_some(local.id),
                    diverges: self.is_uninhabited_type(&function_ty.result),
                },
            };
            if let Some(error) = function_ty.failure_error.as_deref() {
                self.lower_automatic_throwing(call, error, expected, context)
            } else {
                call
            }
        } else {
            self.lowering.partial_parameter_shapes.insert(
                (partial.function.clone(), consumed_groups),
                remaining_parameters[groups.len()..].to_vec(),
            );
            let callable_ty = partial_callable_ty(
                partial.function.clone(),
                consumed_groups,
                FunctionTy {
                    groups: function_ty.groups[groups.len()..].to_vec(),
                    unsafety: function_ty.unsafety,
                    failure_error: function_ty.failure_error.clone(),
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

    pub(super) fn lower_call_argument(
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
        if let Some(argument) =
            self.lower_internal_async_stored_borrow_argument(argument, parameter, context)
        {
            return argument;
        }
        let mode = self.effective_pass_mode(parameter.mode, &parameter.ty);
        match mode {
            PassMode::Copy | PassMode::Move => {
                if matches!(parameter.ty, Ty::Reference { .. }) {
                    return self.lower_reference_call_argument(
                        argument,
                        parameter,
                        context,
                        temporary_loans,
                        temporary_bindings,
                    );
                }
                let value = if let (Ty::Function(function_ty), Expr::Closure(params, body)) =
                    (&parameter.ty, argument)
                {
                    self.lower_noncapturing_closure_argument_as_function(
                        params,
                        body,
                        function_ty,
                        &parameter.name,
                        context,
                    )
                } else if let (
                    Ty::Function(function_ty),
                    Expr::PatternClosure {
                        pattern,
                        guard,
                        body,
                    },
                ) = (&parameter.ty, argument)
                {
                    self.lower_noncapturing_pattern_closure_argument_as_function(
                        pattern,
                        guard.as_deref(),
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
                            "parameter `{}` requires copyable, but `{}` does not implement copyable",
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
                            dynamic_index: None,
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

    pub(super) fn lower_noncapturing_closure_argument_as_function(
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
                unsafe_depth: usize::from(function_ty.unsafety),
                failure_error: function_ty.failure_error.as_deref().cloned(),
                custom_effects: function_ty.custom_effects.iter().cloned().collect(),
                custom_effect_sources,
                lexical_handler_effects: HashSet::new(),
                lexical_handler_effect_sources: HashMap::new(),
                infer_effects: false,
            },
            ClosureCapturePolicy::Lexical,
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
                unsafety: closure.unsafety,
                failure_error: closure.failure_error.clone().map(Box::new),
                custom_effects: closure.custom_effects.clone(),
                result: Box::new(closure.result.clone()),
            }),
            kind: HirExprKind::Function(closure.function),
        }
    }

    pub(super) fn lower_noncapturing_pattern_closure_argument_as_function(
        &mut self,
        pattern: &Pattern,
        guard: Option<&Expr>,
        body: &Expr,
        function_ty: &FunctionTy,
        parameter_name: &str,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let custom_effect_sources =
            source_effect_source_map(&effect_identity_sources(&function_ty.custom_effects));
        let lowered = self.lower_local_pattern_closure(
            pattern,
            guard,
            body,
            function_ty,
            custom_effect_sources,
            context,
        );
        let HirExprKind::LocalClosure(closure) = lowered.kind else {
            return error_expr();
        };
        if !closure.captures.is_empty() {
            self.error(format!(
                "capturing pattern closure cannot be passed to function-typed parameter `{parameter_name}` yet"
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
                unsafety: closure.unsafety,
                failure_error: closure.failure_error.clone().map(Box::new),
                custom_effects: closure.custom_effects.clone(),
                result: Box::new(closure.result.clone()),
            }),
            kind: HirExprKind::Function(closure.function),
        }
    }

    pub(super) fn closure_literal_capture_uses(
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

    pub(super) fn wrap_call_argument_temporaries(
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
                                dynamic_index: None,
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
}
