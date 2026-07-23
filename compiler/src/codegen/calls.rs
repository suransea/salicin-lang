use crate::ast::{CallArg, Expr};
use crate::core::LangItemKind;

use super::fallible::InferredEnumHints;
use super::flow::{LowerCtx, RecursiveFrameCall};
use super::hir::{
    ContinuationAdapter, EffectCallableAdapter, HirExpr, HirExprKind, HirPlace, LocalCapability, Ty,
};
use super::lower::{error_expr, flatten_call, BoundMethodConstraint, TypeProbe};
use super::registry::NominalKind;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_call(
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
                "$lang$unwrap" => Some(("unwrap", LangItemKind::Unwrap)),
                _ => None,
            } {
                let groups = if lang_item == LangItemKind::Unwrap
                    && matches!(groups.as_slice(), [group] if group.is_empty())
                {
                    &groups[..0]
                } else {
                    &groups
                };
                return self.lower_bound_method_call(
                    base,
                    member,
                    groups,
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

    pub(super) fn resolve_function_overload(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
    ) -> Option<String> {
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

    pub(super) fn resolve_inherent_overload(
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

    pub(super) fn matching_function_overloads(
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

    pub(super) fn ordered_call_arguments<'a>(
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
}
