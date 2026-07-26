use std::collections::HashSet;

use crate::ast::{
    BinaryOp, Binding, Expr, IntegerPattern, ItemOrigin, MatchArm, Pattern, PatternFields, Stmt,
    UnaryOp,
};

use super::flow::{FlowState, InspectionBinding, LocalInfo, LowerCtx};
use super::hir::{
    AccessKind, EnumLayout, FieldLayout, HirExpr, HirExprKind, HirMatchArm, HirMatcher,
    HirPatternBinding, HirReadKind, LocalCapability, Ty,
};
use super::lower::{error_expr, TypeProbe};
use super::Analyzer;

fn integer_pattern_expr(pattern: &IntegerPattern) -> Expr {
    let magnitude = Expr::Integer(pattern.magnitude);
    if pattern.negative {
        Expr::Unary(UnaryOp::Neg, Box::new(magnitude))
    } else {
        magnitude
    }
}

impl Analyzer {
    pub(super) fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if direct_boolean_branches(arms).is_some() {
            return self.lower_scalar_match(scrutinee, arms, expected, context, &Ty::Bool);
        }
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
        if *scalar_ty == Ty::Bool {
            if let Some((then_branch, else_branch)) = direct_boolean_branches(arms) {
                let lowered = Expr::Block(
                    vec![Stmt::Let(Binding {
                        value_source: None,
                        mutable: false,
                        name: hidden.clone(),
                        annotation: Some(crate::ast::Type::Bool),
                        value: scrutinee.clone(),
                    })],
                    Some(Box::new(Expr::If {
                        condition: Box::new(Expr::Name(hidden)),
                        then_branch: Box::new(then_branch.clone()),
                        else_branch: Some(Box::new(else_branch.clone())),
                    })),
                );
                return self.lower_expr(&lowered, expected, context);
            }
        }
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
                        Box::new(integer_pattern_expr(value)),
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
                Pattern::Tuple(_) => {
                    self.error(format!(
                        "tuple pattern cannot match scalar type `{scalar_ty}`"
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
                        value_source: None,
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
                value_source: None,
                mutable: false,
                name: hidden,
                annotation: self.source_type_for_ty(scalar_ty),
                value: scrutinee.clone(),
            })],
            Some(Box::new(fallback)),
        );
        self.lower_expr(&lowered, expected, context)
    }

    pub(super) fn lower_match_with_scrutinee(
        &mut self,
        scrutinee: HirExpr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if matches!(scrutinee.ty, Ty::Tuple(_)) {
            return self.lower_tuple_match_with_scrutinee(scrutinee, arms, expected, context);
        }
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
            if let Some(origin) = self.reference_origin_for_hir_expr(&scrutinee, context) {
                for binding in &bindings {
                    if !self.type_reference_requirements(&binding.ty).is_empty() {
                        context
                            .borrowed_parameter_regions
                            .insert(binding.id, origin.clone());
                    }
                }
            }
            if let Some(root) = &inspected_place {
                for binding in bindings.iter().filter(|binding| binding.moves) {
                    let mut alias = root.clone();
                    alias.projections.extend(binding.path.iter().copied());
                    alias.ty = binding.ty.clone();
                    let capability = match binding.ty {
                        Ty::Reference { mutable: true, .. } => LocalCapability::MutParam,
                        _ => LocalCapability::SharedParam,
                    };
                    alias.capability = capability;
                    let local = context
                        .scopes
                        .last_mut()
                        .expect("match arm scope")
                        .names
                        .get_mut(&binding.name)
                        .expect("pattern binding local exists");
                    local.capability = capability;
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
                    "cannot move a pattern binding out of `{}` because it implements `droppable`",
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
                if self.async_factory_depth > 0 {
                    return None;
                }
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

            result_ty = Some(if self.async_factory_depth > 0 {
                result_ty.unwrap_or_else(|| body.ty.clone())
            } else {
                match result_ty {
                    Some(current) => self.unify_types(&current, &body.ty, "match arm results"),
                    None => body.ty.clone(),
                }
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
        if self.async_factory_depth > 0 && !lowered_arms.is_empty() {
            let branch_types = lowered_arms
                .iter()
                .map(|arm| arm.body.ty.clone())
                .collect::<Vec<_>>();
            if branch_types.windows(2).any(|pair| pair[0] != pair[1]) {
                if let Some(branch_name) = self.register_async_branch_future(&branch_types) {
                    let branch_ty = Ty::Enum(branch_name.clone());
                    for (variant, arm) in lowered_arms.iter_mut().enumerate() {
                        arm.body = HirExpr {
                            ty: branch_ty.clone(),
                            kind: HirExprKind::ConstructEnum {
                                name: branch_name.clone(),
                                variant,
                                fields: vec![(0, arm.body.clone())],
                            },
                        };
                    }
                    result_ty = Some(branch_ty);
                } else {
                    result_ty =
                        Some(branch_types.iter().skip(1).fold(
                            branch_types[0].clone(),
                            |current, branch| {
                                self.unify_types(&current, branch, "match arm results")
                            },
                        ));
                }
            } else {
                result_ty = branch_types.first().cloned();
            }
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

    fn lower_tuple_match_with_scrutinee(
        &mut self,
        scrutinee: HirExpr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Ty::Tuple(field_types) = &scrutinee.ty else {
            unreachable!("tuple match requires tuple scrutinee")
        };
        let field_types = field_types.clone();
        let mut lowered_arms = Vec::new();
        let mut covered = false;
        let mut result_ty = None;
        let entry_flow = context.flow.clone();
        let mut fallthrough_flow = Some(entry_flow);
        let mut exit_flows = Vec::new();

        for arm in arms {
            context.push_scope();
            let mut bindings = Vec::new();
            let mut literal_conditions = Vec::new();
            match &arm.pattern {
                Pattern::Wildcard => {}
                Pattern::Binding(name) => self.bind_pattern(
                    name,
                    scrutinee.ty.clone(),
                    Vec::new(),
                    context,
                    &mut bindings,
                ),
                Pattern::Tuple(patterns) => {
                    if patterns.len() != field_types.len() {
                        self.error(format!(
                            "tuple pattern length mismatch: expected {}, found {}",
                            field_types.len(),
                            patterns.len()
                        ));
                    }
                    for (index, pattern) in patterns.iter().enumerate() {
                        let Some(field_ty) = field_types.get(index) else {
                            break;
                        };
                        self.lower_irrefutable_pattern(
                            pattern,
                            field_ty,
                            vec![index],
                            context,
                            &mut bindings,
                            &mut literal_conditions,
                        );
                    }
                }
                Pattern::Constructor { path, .. } => self.error(format!(
                    "constructor pattern `{}` cannot match tuple type `{}`",
                    path.join("."),
                    scrutinee.ty
                )),
                Pattern::Integer(_) | Pattern::Bool(_) => self.error(format!(
                    "pattern type mismatch: literal pattern cannot match `{}`",
                    scrutinee.ty
                )),
            }
            context.flow = fallthrough_flow.clone().unwrap_or_default();
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
            let guard_fallthrough = guard
                .as_ref()
                .map(|_| context.flow_without_current_scope(context.flow.clone()));
            let branch_expected = expected.or(result_ty.as_ref());
            let body = self.lower_expr(&arm.body, branch_expected, context);
            context.pop_scope();
            exit_flows.push(context.flow.clone());
            fallthrough_flow = guard_fallthrough;
            result_ty = Some(match result_ty {
                Some(current) => self.unify_types(&current, &body.ty, "match arm results"),
                None => body.ty.clone(),
            });
            if guard.is_none() {
                covered = true;
            }
            lowered_arms.push(HirMatchArm {
                matcher: HirMatcher::All,
                bindings,
                guard,
                body,
            });
        }
        if let Some(flow) = fallthrough_flow {
            exit_flows.push(flow);
        }
        context.flow = FlowState::join(&exit_flows);
        if !covered {
            self.error("match on tuple is not exhaustive; add an unguarded tuple, wildcard, or binding arm");
        }
        HirExpr {
            ty: result_ty.unwrap_or(Ty::Error),
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
                if let Some(variant_index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == *name && variant.fields.is_empty())
                {
                    return (
                        HirMatcher::Variant(variant_index),
                        bindings,
                        literal_conditions,
                    );
                }
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
                let layout_short_name = layout.name.rsplit("::").next().unwrap_or(&layout.name);
                let source_short_name = source_name.rsplit("::").next().unwrap_or(source_name);
                if path.len() > 2
                    || (path.len() == 2
                        && path[0] != layout.name
                        && path[0] != source_name
                        && path[0] != layout_short_name
                        && path[0] != source_short_name
                        && path[0] != "self")
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
            Pattern::Tuple(_) => {
                self.error(format!("tuple pattern cannot match enum `{}`", layout.name));
                (HirMatcher::All, bindings, literal_conditions)
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
                    Box::new(integer_pattern_expr(value)),
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
                        "cannot move a nested pattern binding through `{ty}` because it implements `droppable`"
                    ));
                }
            }
            Pattern::Tuple(patterns) => {
                let Ty::Tuple(fields) = ty else {
                    self.error(format!("tuple pattern cannot match `{ty}`"));
                    return;
                };
                if patterns.len() != fields.len() {
                    self.error(format!(
                        "tuple pattern length mismatch: expected {}, found {}",
                        fields.len(),
                        patterns.len()
                    ));
                }
                for (index, pattern) in patterns.iter().enumerate() {
                    let Some(field_ty) = fields.get(index) else {
                        break;
                    };
                    let mut nested_path = path.clone();
                    nested_path.push(index);
                    self.lower_irrefutable_pattern(
                        pattern,
                        field_ty,
                        nested_path,
                        context,
                        bindings,
                        literal_conditions,
                    );
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
}

fn direct_boolean_branches(arms: &[MatchArm]) -> Option<(&Expr, &Expr)> {
    let [first, second] = arms else {
        return None;
    };
    if first.guard.is_some() || second.guard.is_some() {
        return None;
    }
    match (&first.pattern, &second.pattern) {
        (Pattern::Bool(true), Pattern::Bool(false)) => Some((&first.body, &second.body)),
        (Pattern::Bool(false), Pattern::Bool(true)) => Some((&second.body, &first.body)),
        _ => None,
    }
}
