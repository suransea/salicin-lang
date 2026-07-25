use std::collections::HashMap;

use crate::ast::{Expr, ItemOrigin, Param, PassMode, Stmt, Visibility};
use crate::core::LangItemKind;

use super::hir::{
    AccessBoundary, AssignmentKind, ClosureCapture, ClosureCaptureMode, ClosureCapturePolicy,
    ClosureEffectContext, EnumLayout, FieldLayout, FunctionSig, HirArgument, HirExpr, HirExprKind,
    HirFunction, HirMatchArm, HirMatcher, HirParam, HirPatternBinding, HirPlace, HirReadKind,
    HirStmt, LocalCapability, ParamSig, StructLayout, Ty, VariantLayout,
};
use super::names::trait_method_name;
use super::registry::{NominalKind, TraitImplInfo, TraitImplKey, TraitRefKey};
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_async_expression(
        &mut self,
        body: &Expr,
        context: &mut super::flow::LowerCtx,
    ) -> HirExpr {
        let source_plan = split_async_source(body);
        for retained in &source_plan.retained {
            if retained.borrowed
                && retained.referent.as_ref().is_some_and(|referent| {
                    source_plan
                        .retained
                        .iter()
                        .any(|candidate| &candidate.name == referent)
                })
            {
                self.error(format!(
                    "async local `{}` borrows `{}` stored in the same future across `await`; the generated state would be self-referential and cannot implement `Move`",
                    retained.name,
                    retained.referent.as_deref().expect("checked retained referent")
                ));
                return super::lower::error_expr();
            }
        }
        self.async_factory_depth += 1;
        let lowered = self.lower_local_closure(
            &[],
            &source_plan.factory_body,
            None,
            ClosureEffectContext {
                infer_effects: true,
                ..ClosureEffectContext::default()
            },
            ClosureCapturePolicy::AsyncOwned,
            context,
        );
        self.async_factory_depth -= 1;
        let HirExprKind::LocalClosure(closure) = lowered.kind else {
            return lowered;
        };
        let async_effect = self.lang_item_name(LangItemKind::AsyncEffect);
        let unsupported_effects = closure
            .custom_effects
            .iter()
            .filter(|effect| effect.as_str() != async_effect)
            .cloned()
            .collect::<Vec<_>>();
        if !unsupported_effects.is_empty() {
            self.error(format!(
                "async residual algebraic effect{} `{}` require poll/resume handler specialization, which is not implemented yet",
                if unsupported_effects.len() == 1 { "" } else { "s" },
                unsupported_effects.join(", ")
            ));
            return super::lower::error_expr();
        }
        let (awaited_ty, retained_types) = if source_plan.retained.is_empty() {
            (closure.result.clone(), Vec::new())
        } else {
            let Ty::Tuple(fields) = &closure.result else {
                self.error("internal async segment bundle did not produce a tuple");
                return super::lower::error_expr();
            };
            (fields[0].clone(), fields[1..].to_vec())
        };
        let mut awaited = if source_plan.has_await {
            let Some(mut awaited) = self.resolve_awaited_future(&awaited_ty) else {
                return super::lower::error_expr();
            };
            awaited.factory_output = closure.result.clone();
            awaited.retained_types = retained_types;
            awaited.retained_modes = awaited
                .retained_types
                .iter()
                .map(|ty| {
                    if self.is_copy_type(ty) {
                        PassMode::Copy
                    } else {
                        PassMode::Move
                    }
                })
                .collect();
            Some(awaited)
        } else {
            None
        };
        let mut continuation_captures = Vec::new();
        if let (Some(awaited), Some(continuation)) = (awaited.as_mut(), source_plan.continuation) {
            if continuation.mutable {
                self.error("an await result used after suspension cannot be mutable yet");
                return super::lower::error_expr();
            }
            let Some(source_ty) = self.source_type_for_ty(&awaited.output) else {
                self.error(format!(
                    "await output type `{}` cannot be represented in a continuation parameter",
                    awaited.output
                ));
                return super::lower::error_expr();
            };
            let mut parameters = Vec::new();
            for ((retained, retained_ty), mode) in source_plan
                .retained
                .iter()
                .zip(&awaited.retained_types)
                .zip(&awaited.retained_modes)
            {
                let Some(source_ty) = self.source_type_for_ty(retained_ty) else {
                    self.error(format!(
                        "async local `{}` of type `{retained_ty}` cannot be represented in a continuation parameter",
                        retained.name
                    ));
                    return super::lower::error_expr();
                };
                parameters.push(Param {
                    mode: *mode,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: retained.name.clone(),
                    ty: source_ty,
                });
            }
            parameters.push(Param {
                mode: PassMode::Inferred,
                access: None,
                modifiers: Vec::new(),
                region: None,
                name: continuation.name,
                ty: source_ty,
            });
            let continuation_has_await = split_async_source(&continuation.body).has_await;
            let continuation_body = if continuation_has_await {
                Expr::Async {
                    body: Box::new(continuation.body),
                }
            } else {
                continuation.body
            };
            let lowered = self.lower_local_closure(
                &parameters,
                &continuation_body,
                None,
                ClosureEffectContext {
                    infer_effects: true,
                    ..ClosureEffectContext::default()
                },
                ClosureCapturePolicy::AsyncOwned,
                context,
            );
            let HirExprKind::LocalClosure(closure) = lowered.kind else {
                return lowered;
            };
            let async_effect = self.lang_item_name(LangItemKind::AsyncEffect);
            if closure.throws_error.is_some()
                || closure
                    .custom_effects
                    .iter()
                    .any(|effect| effect.as_str() != async_effect)
            {
                self.error(
                    "async continuation residual Throws and algebraic effects require poll/resume handler specialization, which is not implemented yet",
                );
                return super::lower::error_expr();
            }
            awaited.continuation = Some(closure.function);
            awaited.continuation_output = Some(closure.result.clone());
            awaited.continuation_unsafe_effect = closure.unsafe_effect;
            if continuation_has_await {
                let Some(next) = self.resolve_awaited_future(&closure.result) else {
                    return super::lower::error_expr();
                };
                awaited.next = Some(Box::new(next));
            }
            continuation_captures = closure.captures;
        }

        let name = format!("$async$state${}", self.next_async_future);
        self.next_async_future += 1;
        let access = AccessBoundary {
            visibility: Visibility::Private,
            origin: self
                .current_origin
                .as_deref()
                .cloned()
                .unwrap_or_else(ItemOrigin::default),
        };
        let mut fields = vec![FieldLayout {
            name: "state".to_owned(),
            ty: Ty::I32,
            access: access.clone(),
        }];
        let mut values = vec![(
            0,
            HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(0),
            },
        )];

        for (index, capture) in closure.captures.iter().enumerate() {
            let (ty, value) = materialize_async_capture(capture);
            fields.push(FieldLayout {
                name: format!("capture.{index}"),
                ty,
                access: access.clone(),
            });
            values.push((index + 1, value));
        }
        let mut continuation_fields = Vec::new();
        let mut continuation_capture_types = Vec::new();
        for (index, capture) in continuation_captures.iter().enumerate() {
            let (ty, value) = materialize_async_capture(capture);
            let field = fields.len();
            fields.push(FieldLayout {
                name: format!("continuation.capture.{index}"),
                ty: ty.clone(),
                access: access.clone(),
            });
            values.push((field, value));
            continuation_fields.push(field);
            continuation_capture_types.push(ty);
        }
        if let Some(awaited) = awaited.as_mut() {
            awaited.continuation_capture_modes = continuation_captures
                .iter()
                .map(capture_pass_mode)
                .collect();
            awaited.continuation_fields = continuation_fields;
            awaited.continuation_capture_types = continuation_capture_types;
            for (index, ty) in awaited.retained_types.iter().enumerate() {
                let field = fields.len();
                fields.push(FieldLayout {
                    name: format!("retained.{index}"),
                    ty: ty.clone(),
                    access: access.clone(),
                });
                awaited.retained_fields.push(field);
            }
        }
        if let Some(next) = awaited.as_mut().and_then(|awaited| awaited.next.as_mut()) {
            let field = fields.len();
            fields.push(FieldLayout {
                name: "awaited.next".to_owned(),
                ty: next.ty.clone(),
                access: access.clone(),
            });
            next.field = field;
        }
        let awaited_field = awaited.as_ref().map(|awaited| {
            let field = fields.len();
            fields.push(FieldLayout {
                name: "awaited".to_owned(),
                ty: awaited.ty.clone(),
                access: access.clone(),
            });
            field
        });

        self.nominal_accesses.insert(name.clone(), access);
        self.struct_layouts.insert(
            name.clone(),
            StructLayout {
                name: name.clone(),
                fields,
            },
        );
        self.struct_order.push(name.clone());
        let output = awaited
            .as_ref()
            .map(|awaited| {
                awaited.next.as_ref().map_or_else(
                    || {
                        awaited
                            .continuation_output
                            .clone()
                            .unwrap_or_else(|| awaited.output.clone())
                    },
                    |next| next.output.clone(),
                )
            })
            .unwrap_or_else(|| closure.result.clone());
        let unsafe_effect = closure.unsafe_effect
            || awaited.as_ref().is_some_and(|awaited| {
                awaited.unsafe_effect
                    || awaited.continuation_unsafe_effect
                    || awaited.next.as_ref().is_some_and(|next| next.unsafe_effect)
            });
        let metadata = AsyncFutureInfo {
            resume: closure.function,
            output,
            unsafe_effect,
            throws_error: closure.throws_error,
            custom_effects: closure.custom_effects,
            capture_modes: closure.captures.iter().map(capture_pass_mode).collect(),
            awaited: awaited.map(|mut awaited| {
                awaited.field = awaited_field.expect("awaited state has a field");
                awaited
            }),
        };
        self.async_futures.insert(name.clone(), metadata);
        self.register_future_poll(&name);

        HirExpr {
            ty: Ty::Struct(name.clone()),
            kind: HirExprKind::ConstructStruct {
                name,
                fields: values,
            },
        }
    }

    fn resolve_awaited_future(&mut self, ty: &Ty) -> Option<AwaitedFutureInfo> {
        let origin = self
            .current_origin
            .as_deref()
            .cloned()
            .unwrap_or_else(ItemOrigin::default);
        let future_trait = self.lang_item_name(LangItemKind::Future);
        let mut candidates = self
            .trait_method_candidates(ty, "poll", &origin)
            .into_iter()
            .filter(|candidate| candidate.trait_ref.name == future_trait)
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            self.error(if candidates.is_empty() {
                format!("await operand of type `{ty}` does not implement `Future`")
            } else {
                format!(
                    "await operand of type `{ty}` has multiple `Future` implementations; the residual effect row is ambiguous"
                )
            });
            return None;
        }
        let candidate = candidates.pop().expect("one future candidate");
        let implementation = self
            .trait_impls
            .get(&candidate)
            .cloned()
            .expect("trait method candidate has an implementation");
        let output = implementation
            .associated_types
            .get("Output")
            .cloned()
            .expect("validated Future implementation has Output");
        let poll_function = implementation
            .methods
            .get("poll")
            .cloned()
            .expect("validated Future implementation has poll");
        let signature = self
            .signatures
            .get(&poll_function)
            .cloned()
            .expect("registered Future poll has a signature");
        if signature.throws_error.is_some() || !signature.custom_effects.is_empty() {
            self.error(
                "await residual Throws and algebraic effects require poll/resume handler specialization, which is not implemented yet",
            );
            return None;
        }
        let poll_ty = signature
            .result
            .clone()
            .expect("validated Future poll has a result");
        Some(AwaitedFutureInfo {
            ty: ty.clone(),
            factory_output: ty.clone(),
            output,
            poll_ty,
            poll_function,
            unsafe_effect: signature.unsafe_effect,
            field: 0,
            continuation: None,
            continuation_output: None,
            continuation_unsafe_effect: false,
            continuation_capture_modes: Vec::new(),
            continuation_fields: Vec::new(),
            continuation_capture_types: Vec::new(),
            retained_fields: Vec::new(),
            retained_types: Vec::new(),
            retained_modes: Vec::new(),
            next: None,
        })
    }

    pub(super) fn register_async_branch_future(&mut self, branch_types: &[Ty]) -> Option<String> {
        if self.async_factory_depth == 0 || branch_types.len() < 2 {
            return None;
        }
        let origin = self
            .current_origin
            .as_deref()
            .cloned()
            .unwrap_or_else(ItemOrigin::default);
        let future_trait = self.lang_item_name(LangItemKind::Future).to_owned();
        if branch_types.iter().any(|ty| {
            self.trait_method_candidates(ty, "poll", &origin)
                .into_iter()
                .filter(|candidate| candidate.trait_ref.name == future_trait)
                .count()
                != 1
        }) {
            return None;
        }
        let futures = branch_types
            .iter()
            .map(|ty| self.resolve_awaited_future(ty))
            .collect::<Option<Vec<_>>>()?;
        let output = futures.first()?.output.clone();
        if futures.iter().any(|future| future.output != output) {
            self.error("control-flow await branches must produce the same `Future.Output` type");
            return None;
        }
        let poll_ty = futures.first()?.poll_ty.clone();
        if futures.iter().any(|future| future.poll_ty != poll_ty) {
            self.error("control-flow await branches resolved incompatible `Poll` result types");
            return None;
        }

        let name = format!("$async$branch${}", self.next_async_future);
        self.next_async_future += 1;
        let access = AccessBoundary {
            visibility: Visibility::Private,
            origin,
        };
        let mut payload_offset = 0;
        let variants = branch_types
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let variant = VariantLayout {
                    name: format!("Branch{index}"),
                    fields: vec![FieldLayout {
                        name: "future".to_owned(),
                        ty: ty.clone(),
                        access: access.clone(),
                    }],
                    payload_offset,
                    named: false,
                };
                payload_offset += 1;
                variant
            })
            .collect::<Vec<_>>();
        self.enum_layouts.insert(
            name.clone(),
            EnumLayout {
                name: name.clone(),
                variants: variants.clone(),
            },
        );
        self.enum_order.push(name.clone());
        self.nominal_accesses.insert(name.clone(), access.clone());

        let self_ty = Ty::Enum(name.clone());
        let self_reference = Ty::Reference {
            pointee: Box::new(self_ty.clone()),
            mutable: true,
            region: None,
        };
        let unsafe_effect = futures.iter().any(|future| future.unsafe_effect);
        let effect_row = Ty::EffectRow {
            unsafe_effect,
            throws_error: None,
            custom_effects: Vec::new(),
        };
        let trait_key = TraitImplKey {
            self_ty: self_ty.clone(),
            trait_ref: TraitRefKey {
                name: future_trait,
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        self.signatures.insert(
            poll_function.clone(),
            FunctionSig {
                groups: vec![
                    vec![ParamSig {
                        name: "self".to_owned(),
                        ty: self_reference.clone(),
                        mode: PassMode::Inferred,
                    }],
                    Vec::new(),
                ],
                unsafe_effect,
                throws_error: None,
                custom_effects: Vec::new(),
                result: Some(poll_ty.clone()),
            },
        );
        self.function_accesses
            .insert(poll_function.clone(), access.clone());
        self.inherent_members
            .entry(name.clone())
            .or_default()
            .methods
            .insert("poll".to_owned(), poll_function.clone());
        self.trait_impl_headers.insert(trait_key.clone());
        self.trait_methods_by_receiver
            .entry((self_ty.clone(), "poll".to_owned()))
            .or_default()
            .push(trait_key.clone());
        let output_source = self.source_type_for_ty(&output)?;
        self.trait_impls.insert(
            trait_key.clone(),
            TraitImplInfo {
                key: trait_key,
                associated_types: HashMap::from([("Output".to_owned(), output.clone())]),
                associated_type_sources: HashMap::from([("Output".to_owned(), output_source)]),
                methods: HashMap::from([("poll".to_owned(), poll_function.clone())]),
                access,
            },
        );

        let scrutinee_place = HirPlace {
            local: 0,
            root_ty: self_ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
            ty: self_ty.clone(),
            capability: LocalCapability::MutParam,
            root_mutable: true,
            loan: None,
            indirect: true,
        };
        let arms = futures
            .iter()
            .zip(&variants)
            .enumerate()
            .map(|(index, (future, variant))| {
                let child_place = HirPlace {
                    local: 0,
                    root_ty: self_ty.clone(),
                    projections: vec![1 + variant.payload_offset],
                    dynamic_index: None,
                    ty: future.ty.clone(),
                    capability: LocalCapability::MutParam,
                    root_mutable: true,
                    loan: None,
                    indirect: true,
                };
                HirMatchArm {
                    matcher: HirMatcher::Variant(index),
                    bindings: Vec::new(),
                    guard: None,
                    body: HirExpr {
                        ty: poll_ty.clone(),
                        kind: HirExprKind::Call {
                            function: future.poll_function.clone(),
                            arguments: vec![HirArgument::Copy(HirExpr {
                                ty: Ty::Reference {
                                    pointee: Box::new(future.ty.clone()),
                                    mutable: true,
                                    region: None,
                                },
                                kind: HirExprKind::Borrow {
                                    place: child_place,
                                    mutable: true,
                                },
                            })],
                            consumed_callable: None,
                            diverges: false,
                        },
                    },
                }
            })
            .collect();
        self.lifted_functions.push(HirFunction {
            name: poll_function,
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference,
                mode: PassMode::Inferred,
            }],
            result: poll_ty.clone(),
            body: HirExpr {
                ty: poll_ty,
                kind: HirExprKind::Match {
                    scrutinee: Box::new(HirExpr {
                        ty: self_ty,
                        kind: HirExprKind::Read {
                            place: scrutinee_place,
                            kind: HirReadKind::Inspect,
                        },
                    }),
                    arms,
                },
            },
        });
        Some(name)
    }

    fn register_future_poll(&mut self, name: &str) {
        let Some(future) = self.async_futures.get(name).cloned() else {
            return;
        };
        let Some(output_source) = self.source_type_for_ty(&future.output) else {
            self.error(format!(
                "async output type `{}` cannot be represented in the source type system",
                future.output
            ));
            return;
        };
        let poll_template = self.lang_item_name(LangItemKind::Poll).to_owned();
        let Some(poll_name) = self.ensure_nominal_instance(
            NominalKind::Enum,
            &poll_template,
            vec![output_source.clone()],
            vec![future.output.clone()],
        ) else {
            return;
        };
        let poll_ty = Ty::Enum(poll_name.clone());
        let self_ty = Ty::Struct(name.to_owned());
        let self_reference = Ty::Reference {
            pointee: Box::new(self_ty.clone()),
            mutable: true,
            region: None,
        };
        let async_effect = self.lang_item_name(LangItemKind::AsyncEffect);
        let residual_custom_effects = future
            .custom_effects
            .iter()
            .filter(|effect| effect.as_str() != async_effect)
            .cloned()
            .collect::<Vec<_>>();
        let effect_row = Ty::EffectRow {
            unsafe_effect: future.unsafe_effect,
            throws_error: future.throws_error.clone().map(Box::new),
            custom_effects: residual_custom_effects.clone(),
        };
        let trait_key = TraitImplKey {
            self_ty: self_ty.clone(),
            trait_ref: TraitRefKey {
                name: self.lang_item_name(LangItemKind::Future).to_owned(),
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        let receiver = ParamSig {
            name: "self".to_owned(),
            ty: self_reference.clone(),
            mode: PassMode::Inferred,
        };
        self.signatures.insert(
            poll_function.clone(),
            FunctionSig {
                groups: vec![vec![receiver], Vec::new()],
                unsafe_effect: future.unsafe_effect,
                throws_error: future.throws_error.clone(),
                custom_effects: residual_custom_effects,
                result: Some(poll_ty.clone()),
            },
        );
        self.function_accesses
            .insert(poll_function.clone(), self.nominal_accesses[name].clone());
        self.inherent_members
            .entry(name.to_owned())
            .or_default()
            .methods
            .insert("poll".to_owned(), poll_function.clone());
        self.trait_impl_headers.insert(trait_key.clone());
        self.trait_methods_by_receiver
            .entry((self_ty.clone(), "poll".to_owned()))
            .or_default()
            .push(trait_key.clone());
        self.trait_impls.insert(
            trait_key.clone(),
            TraitImplInfo {
                key: trait_key,
                associated_types: HashMap::from([("Output".to_owned(), future.output.clone())]),
                associated_type_sources: HashMap::from([("Output".to_owned(), output_source)]),
                methods: HashMap::from([("poll".to_owned(), poll_function.clone())]),
                access: self.nominal_accesses[name].clone(),
            },
        );

        let layout = self.struct_layouts[name].clone();
        let arguments = future
            .capture_modes
            .iter()
            .enumerate()
            .map(|(capture, mode)| {
                let field = capture + 1;
                let ty = layout.fields[field].ty.clone();
                let place = async_field_place(0, self_ty.clone(), field, ty.clone());
                match mode {
                    PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => {
                        HirArgument::Copy(HirExpr {
                            ty,
                            kind: HirExprKind::Read {
                                place,
                                kind: HirReadKind::Copy,
                            },
                        })
                    }
                    PassMode::Move => HirArgument::Move(HirExpr {
                        ty: ty.clone(),
                        kind: HirExprKind::RawTake(Box::new(HirExpr {
                            ty: Ty::Pointer {
                                pointee: Box::new(ty),
                                mutable: true,
                            },
                            kind: HirExprKind::RawAddress { place },
                        })),
                    }),
                    PassMode::Inferred => {
                        unreachable!("async capture modes are normalized while materializing state")
                    }
                }
            })
            .collect::<Vec<_>>();
        let resume = HirExpr {
            ty: future.awaited.as_ref().map_or_else(
                || future.output.clone(),
                |awaited| awaited.factory_output.clone(),
            ),
            kind: HirExprKind::Call {
                function: future.resume.clone(),
                arguments,
                consumed_callable: None,
                diverges: future.awaited.is_none() && future.output == Ty::Never,
            },
        };
        let body = if let Some(awaited) = &future.awaited {
            suspended_poll_body(
                &self_ty,
                &poll_ty,
                &poll_name,
                &future.output,
                awaited,
                resume,
            )
        } else {
            ready_poll_body(&self_ty, &poll_ty, &poll_name, &future.output, resume)
        };
        self.lifted_functions.push(HirFunction {
            name: poll_function,
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference,
                mode: PassMode::Inferred,
            }],
            result: poll_ty,
            body,
        });
    }
}

fn materialize_async_capture(capture: &ClosureCapture) -> (Ty, HirExpr) {
    if capture.by_value {
        return (
            capture.place.ty.clone(),
            capture
                .value
                .as_deref()
                .cloned()
                .expect("by-value async capture materializes its value"),
        );
    }
    match capture.mode {
        ClosureCaptureMode::Shared => {
            let ty = Ty::Reference {
                pointee: Box::new(capture.place.ty.clone()),
                mutable: false,
                region: None,
            };
            (
                ty.clone(),
                HirExpr {
                    ty,
                    kind: HirExprKind::Borrow {
                        place: capture.place.clone(),
                        mutable: false,
                    },
                },
            )
        }
        ClosureCaptureMode::Mutable => {
            let ty = Ty::Reference {
                pointee: Box::new(capture.place.ty.clone()),
                mutable: true,
                region: None,
            };
            (
                ty.clone(),
                HirExpr {
                    ty,
                    kind: HirExprKind::Borrow {
                        place: capture.place.clone(),
                        mutable: true,
                    },
                },
            )
        }
        ClosureCaptureMode::Move => (
            capture.place.ty.clone(),
            capture
                .value
                .as_deref()
                .cloned()
                .expect("move capture materializes its value"),
        ),
    }
}

fn capture_pass_mode(capture: &ClosureCapture) -> PassMode {
    if capture.by_value {
        return PassMode::Copy;
    }
    match capture.mode {
        ClosureCaptureMode::Shared => PassMode::Borrow,
        ClosureCaptureMode::Mutable => PassMode::MutBorrow,
        ClosureCaptureMode::Move => PassMode::Move,
    }
}

fn ready_poll_body(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    output: &Ty,
    resume: HirExpr,
) -> HirExpr {
    let ready = poll_ready(poll_ty, poll_name, output, resume);
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::If {
            condition: Box::new(state_is(self_ty, 0)),
            then_branch: Box::new(HirExpr {
                ty: poll_ty.clone(),
                kind: HirExprKind::Block(
                    vec![HirStmt::Expr(set_state(self_ty, 1))],
                    Some(Box::new(ready)),
                ),
            }),
            else_branch: Some(Box::new(trap())),
        },
    }
}

fn suspended_poll_body(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    output: &Ty,
    awaited: &AwaitedFutureInfo,
    resume: HirExpr,
) -> HirExpr {
    let child_place = async_field_place(0, self_ty.clone(), awaited.field, awaited.ty.clone());
    let bundle_local = 1_000;
    let bundle_binding = (!awaited.retained_fields.is_empty()).then(|| {
        HirStmt::Let(super::hir::HirBinding {
            id: bundle_local,
            name: "async.segment".to_owned(),
            ty: awaited.factory_output.clone(),
            mutable: false,
            value: resume.clone(),
        })
    });
    let bundle_field = |index: usize, ty: &Ty| HirExpr {
        ty: ty.clone(),
        kind: HirExprKind::Read {
            place: HirPlace {
                local: bundle_local,
                root_ty: awaited.factory_output.clone(),
                projections: vec![index],
                dynamic_index: None,
                ty: ty.clone(),
                capability: LocalCapability::Owned,
                root_mutable: false,
                loan: None,
                indirect: false,
            },
            kind: HirReadKind::Move,
        },
    };
    let initialize_field = |place: HirPlace, value: HirExpr| HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::RawInit {
            pointer: Box::new(HirExpr {
                ty: Ty::Pointer {
                    pointee: Box::new(place.ty.clone()),
                    mutable: true,
                },
                kind: HirExprKind::RawAddress { place },
            }),
            value: Box::new(value),
        },
    };
    let initialize_child = initialize_field(
        child_place.clone(),
        if awaited.retained_fields.is_empty() {
            resume
        } else {
            bundle_field(0, &awaited.ty)
        },
    );
    let initialize_retained = awaited
        .retained_fields
        .iter()
        .zip(&awaited.retained_types)
        .enumerate()
        .map(|(index, (field, ty))| {
            initialize_field(
                async_field_place(0, self_ty.clone(), *field, ty.clone()),
                bundle_field(index + 1, ty),
            )
        });
    let cold_poll = poll_awaited(
        self_ty,
        poll_ty,
        poll_name,
        output,
        awaited,
        AwaitPollState::new(1, 3, 2),
    );
    let resumed_poll = poll_awaited(
        self_ty,
        poll_ty,
        poll_name,
        output,
        awaited,
        AwaitPollState::new(2, 4, 2),
    );
    let waiting_branch = if let Some(next) = &awaited.next {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(state_is(self_ty, 1)),
                then_branch: Box::new(resumed_poll),
                else_branch: Some(Box::new(HirExpr {
                    ty: poll_ty.clone(),
                    kind: HirExprKind::If {
                        condition: Box::new(state_is(self_ty, 2)),
                        then_branch: Box::new(poll_awaited(
                            self_ty,
                            poll_ty,
                            poll_name,
                            output,
                            next,
                            AwaitPollState::new(5, 6, 3),
                        )),
                        else_branch: Some(Box::new(trap())),
                    },
                })),
            },
        }
    } else {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(state_is(self_ty, 1)),
                then_branch: Box::new(resumed_poll),
                else_branch: Some(Box::new(trap())),
            },
        }
    };
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::If {
            condition: Box::new(state_is(self_ty, 0)),
            then_branch: Box::new(HirExpr {
                ty: poll_ty.clone(),
                kind: HirExprKind::Block(
                    bundle_binding
                        .into_iter()
                        .chain(std::iter::once(HirStmt::Expr(initialize_child)))
                        .chain(initialize_retained.map(HirStmt::Expr))
                        .chain(std::iter::once(HirStmt::Expr(set_state(self_ty, 1))))
                        .collect(),
                    Some(Box::new(cold_poll)),
                ),
            }),
            else_branch: Some(Box::new(waiting_branch)),
        },
    }
}

#[derive(Clone, Copy)]
struct AwaitPollState {
    output_local: usize,
    next_output_local: usize,
    completed: i128,
}

impl AwaitPollState {
    fn new(output_local: usize, next_output_local: usize, completed: i128) -> Self {
        Self {
            output_local,
            next_output_local,
            completed,
        }
    }
}

fn poll_awaited(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    parent_output: &Ty,
    awaited: &AwaitedFutureInfo,
    state: AwaitPollState,
) -> HirExpr {
    let child_place = async_field_place(0, self_ty.clone(), awaited.field, awaited.ty.clone());
    let child_reference = Ty::Reference {
        pointee: Box::new(awaited.ty.clone()),
        mutable: true,
        region: None,
    };
    let call = HirExpr {
        ty: awaited.poll_ty.clone(),
        kind: HirExprKind::Call {
            function: awaited.poll_function.clone(),
            arguments: vec![HirArgument::Copy(HirExpr {
                ty: child_reference,
                kind: HirExprKind::Borrow {
                    place: child_place.clone(),
                    mutable: true,
                },
            })],
            consumed_callable: None,
            diverges: false,
        },
    };
    let output_place = HirPlace {
        local: state.output_local,
        root_ty: awaited.output.clone(),
        projections: Vec::new(),
        dynamic_index: None,
        ty: awaited.output.clone(),
        capability: LocalCapability::Owned,
        root_mutable: false,
        loan: None,
        indirect: false,
    };
    let take_child = HirExpr {
        ty: awaited.ty.clone(),
        kind: HirExprKind::RawTake(Box::new(HirExpr {
            ty: Ty::Pointer {
                pointee: Box::new(awaited.ty.clone()),
                mutable: true,
            },
            kind: HirExprKind::RawAddress { place: child_place },
        })),
    };
    let awaited_output = HirExpr {
        ty: awaited.output.clone(),
        kind: HirExprKind::Read {
            place: output_place,
            kind: HirReadKind::Move,
        },
    };
    let mut continuation_arguments = Vec::new();
    for ((field, mode), ty) in awaited
        .continuation_fields
        .iter()
        .zip(&awaited.continuation_capture_modes)
        .zip(&awaited.continuation_capture_types)
    {
        let place = async_field_place(0, self_ty.clone(), *field, ty.clone());
        continuation_arguments.push(match mode {
            PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => HirArgument::Copy(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::Read {
                    place,
                    kind: HirReadKind::Copy,
                },
            }),
            PassMode::Move => HirArgument::Move(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::RawTake(Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place },
                })),
            }),
            PassMode::Inferred => {
                unreachable!("async continuation capture modes are normalized")
            }
        });
    }
    for ((field, mode), ty) in awaited
        .retained_fields
        .iter()
        .zip(&awaited.retained_modes)
        .zip(&awaited.retained_types)
    {
        let place = async_field_place(0, self_ty.clone(), *field, ty.clone());
        continuation_arguments.push(match mode {
            PassMode::Copy => HirArgument::Copy(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::Read {
                    place,
                    kind: HirReadKind::Copy,
                },
            }),
            PassMode::Move => HirArgument::Move(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::RawTake(Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place },
                })),
            }),
            PassMode::Borrow | PassMode::MutBorrow | PassMode::Inferred => {
                unreachable!("retained async locals use Copy or Move")
            }
        });
    }
    continuation_arguments.push(HirArgument::Move(awaited_output.clone()));
    let completed_value = match &awaited.continuation {
        Some(continuation) => HirExpr {
            ty: awaited
                .continuation_output
                .clone()
                .expect("async continuation has an output"),
            kind: HirExprKind::Call {
                function: continuation.clone(),
                arguments: continuation_arguments,
                consumed_callable: None,
                diverges: parent_output == &Ty::Never,
            },
        },
        None => awaited_output,
    };
    let ready_body = if let Some(next) = &awaited.next {
        let next_place = async_field_place(0, self_ty.clone(), next.field, next.ty.clone());
        let initialize_next = HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::RawInit {
                pointer: Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(next.ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place: next_place },
                }),
                value: Box::new(completed_value),
            },
        };
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::Block(
                vec![
                    HirStmt::Expr(take_child),
                    HirStmt::Expr(initialize_next),
                    HirStmt::Expr(set_state(self_ty, 2)),
                ],
                Some(Box::new(poll_awaited(
                    self_ty,
                    poll_ty,
                    poll_name,
                    parent_output,
                    next,
                    AwaitPollState::new(state.next_output_local, state.next_output_local + 100, 3),
                ))),
            ),
        }
    } else {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::Block(
                vec![
                    HirStmt::Expr(take_child),
                    HirStmt::Expr(set_state(self_ty, state.completed)),
                ],
                Some(Box::new(poll_ready(
                    poll_ty,
                    poll_name,
                    parent_output,
                    completed_value,
                ))),
            ),
        }
    };
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::Match {
            scrutinee: Box::new(call),
            arms: vec![
                HirMatchArm {
                    matcher: HirMatcher::Variant(0),
                    bindings: Vec::new(),
                    guard: None,
                    body: poll_pending(poll_ty, poll_name),
                },
                HirMatchArm {
                    matcher: HirMatcher::Variant(1),
                    bindings: vec![HirPatternBinding {
                        id: state.output_local,
                        name: "awaited.output".to_owned(),
                        ty: awaited.output.clone(),
                        path: vec![1],
                        moves: true,
                    }],
                    guard: None,
                    body: ready_body,
                },
            ],
        },
    }
}

fn state_is(self_ty: &Ty, state: i128) -> HirExpr {
    HirExpr {
        ty: Ty::Bool,
        kind: HirExprKind::Binary(
            Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Read {
                    place: async_field_place(0, self_ty.clone(), 0, Ty::I32),
                    kind: HirReadKind::Copy,
                },
            }),
            crate::ast::BinaryOp::Eq,
            Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(state),
            }),
        ),
    }
}

fn set_state(self_ty: &Ty, state: i128) -> HirExpr {
    HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::Assign {
            place: async_field_place(0, self_ty.clone(), 0, Ty::I32),
            value: Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(state),
            }),
            assignment: AssignmentKind::Overwrite,
            root_initialized: true,
        },
    }
}

fn poll_pending(poll_ty: &Ty, poll_name: &str) -> HirExpr {
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::ConstructEnum {
            name: poll_name.to_owned(),
            variant: 0,
            fields: Vec::new(),
        },
    }
}

fn poll_ready(poll_ty: &Ty, poll_name: &str, output: &Ty, value: HirExpr) -> HirExpr {
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::ConstructEnum {
            name: poll_name.to_owned(),
            variant: 1,
            fields: vec![(
                0,
                HirExpr {
                    ty: output.clone(),
                    ..value
                },
            )],
        },
    }
}

fn trap() -> HirExpr {
    HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::RawTrap,
    }
}

fn async_field_place(local: usize, state: Ty, field: usize, ty: Ty) -> HirPlace {
    HirPlace {
        local,
        root_ty: state,
        projections: vec![field],
        dynamic_index: None,
        ty,
        capability: LocalCapability::MutParam,
        root_mutable: true,
        loan: None,
        indirect: true,
    }
}

#[derive(Debug, Clone)]
pub(super) struct AsyncFutureInfo {
    pub(super) resume: String,
    pub(super) output: Ty,
    pub(super) unsafe_effect: bool,
    pub(super) throws_error: Option<Ty>,
    pub(super) custom_effects: Vec<String>,
    pub(super) capture_modes: Vec<PassMode>,
    pub(super) awaited: Option<AwaitedFutureInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct AwaitedFutureInfo {
    pub(super) ty: Ty,
    pub(super) factory_output: Ty,
    pub(super) output: Ty,
    pub(super) poll_ty: Ty,
    pub(super) poll_function: String,
    pub(super) unsafe_effect: bool,
    pub(super) field: usize,
    pub(super) continuation: Option<String>,
    pub(super) continuation_output: Option<Ty>,
    pub(super) continuation_unsafe_effect: bool,
    pub(super) continuation_capture_modes: Vec<PassMode>,
    pub(super) continuation_fields: Vec<usize>,
    pub(super) continuation_capture_types: Vec<Ty>,
    pub(super) retained_fields: Vec<usize>,
    pub(super) retained_types: Vec<Ty>,
    pub(super) retained_modes: Vec<PassMode>,
    pub(super) next: Option<Box<AwaitedFutureInfo>>,
}

struct AsyncSourcePlan {
    factory_body: Expr,
    has_await: bool,
    continuation: Option<AsyncContinuationSource>,
    retained: Vec<AsyncRetainedSource>,
}

struct AsyncContinuationSource {
    name: String,
    mutable: bool,
    body: Expr,
}

struct AsyncRetainedSource {
    name: String,
    referent: Option<String>,
    borrowed: bool,
}

fn split_async_source(body: &Expr) -> AsyncSourcePlan {
    let mut body = body.clone();
    match body.unlocated_mut() {
        Expr::Await(operand) => {
            return AsyncSourcePlan {
                factory_body: (**operand).clone(),
                has_await: true,
                continuation: None,
                retained: Vec::new(),
            };
        }
        expression if hoist_control_await(expression).is_some() => {
            return AsyncSourcePlan {
                factory_body: hoist_control_await(expression)
                    .expect("checked control-flow await hoisting"),
                has_await: true,
                continuation: None,
                retained: Vec::new(),
            };
        }
        Expr::Block(statements, Some(tail)) => {
            if let Expr::Await(operand) = tail.unlocated() {
                if !statements.is_empty() {
                    let result = "$async$tail$result".to_owned();
                    let mut rewritten = statements.clone();
                    rewritten.push(Stmt::Let(crate::ast::Binding {
                        mutable: false,
                        name: result.clone(),
                        annotation: None,
                        value: Expr::Await(Box::new((**operand).clone())),
                        value_source: None,
                    }));
                    rewritten.extend(
                        non_borrow_binding_names(statements)
                            .into_iter()
                            .map(|name| Stmt::Expr(Expr::Name(name))),
                    );
                    return split_async_source(&Expr::Block(
                        rewritten,
                        Some(Box::new(Expr::Name(result))),
                    ));
                }
                **tail = (**operand).clone();
                return AsyncSourcePlan {
                    factory_body: body,
                    has_await: true,
                    continuation: None,
                    retained: Vec::new(),
                };
            }
            if let Some(hoisted) = hoist_control_await(tail) {
                **tail = hoisted;
                return AsyncSourcePlan {
                    factory_body: body,
                    has_await: true,
                    continuation: None,
                    retained: Vec::new(),
                };
            }
        }
        _ => {}
    }
    let Expr::Block(statements, tail) = body.unlocated() else {
        return AsyncSourcePlan {
            factory_body: body,
            has_await: false,
            continuation: None,
            retained: Vec::new(),
        };
    };
    let Some((position, binding, operand)) =
        statements
            .iter()
            .enumerate()
            .find_map(|(position, statement)| {
                let Stmt::Let(binding) = statement else {
                    return None;
                };
                let operand = match binding.value.unlocated() {
                    Expr::Await(operand) => (**operand).clone(),
                    expression => hoist_control_await(expression)?,
                };
                Some((position, binding, operand))
            })
    else {
        return AsyncSourcePlan {
            factory_body: body,
            has_await: false,
            continuation: None,
            retained: Vec::new(),
        };
    };
    let continuation_body = Expr::Block(statements[position + 1..].to_vec(), tail.clone());
    let mut referenced = referenced_names(&continuation_body);
    let dependencies = statements[..position]
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            Some((
                binding.name.clone(),
                async_initializer_root(&binding.value)?,
            ))
        })
        .collect::<Vec<_>>();
    let borrowed_names = borrowed_binding_names(&statements[..position]);
    loop {
        let mut changed = false;
        for (binding, referent) in &dependencies {
            if referenced.contains(binding) {
                changed |= referenced.insert(referent.clone());
            }
        }
        if !changed {
            break;
        }
    }
    let mut retained = Vec::<AsyncRetainedSource>::new();
    for statement in &statements[..position] {
        let Stmt::Let(binding) = statement else {
            continue;
        };
        if !referenced.contains(&binding.name) {
            continue;
        }
        let referent = async_initializer_root(&binding.value)
            .map(|referent| resolve_async_dependency(&referent, &dependencies));
        if let Some(existing) = retained
            .iter_mut()
            .find(|retained| retained.name == binding.name)
        {
            *existing = AsyncRetainedSource {
                name: binding.name.clone(),
                referent,
                borrowed: borrowed_names.contains(&binding.name),
            };
        } else {
            retained.push(AsyncRetainedSource {
                name: binding.name.clone(),
                referent,
                borrowed: borrowed_names.contains(&binding.name),
            });
        }
    }
    let factory_tail = if retained.is_empty() {
        operand.clone()
    } else {
        Expr::Tuple(
            std::iter::once(operand.clone())
                .chain(
                    retained
                        .iter()
                        .map(|retained| Expr::Name(retained.name.clone())),
                )
                .collect(),
        )
    };
    AsyncSourcePlan {
        factory_body: Expr::Block(
            statements[..position].to_vec(),
            Some(Box::new(factory_tail)),
        ),
        has_await: true,
        continuation: Some(AsyncContinuationSource {
            name: binding.name.clone(),
            mutable: binding.mutable,
            body: continuation_body,
        }),
        retained,
    }
}

fn hoist_control_await(expression: &Expr) -> Option<Expr> {
    match expression.unlocated() {
        Expr::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        } => {
            let then_future = branch_await_future(then_branch);
            let else_future = branch_await_future(else_branch);
            if then_future.is_none() && else_future.is_none() {
                return None;
            }
            Some(Expr::If {
                condition: condition.clone(),
                then_branch: Box::new(then_future.unwrap_or_else(|| Expr::Async {
                    body: then_branch.clone(),
                })),
                else_branch: Some(Box::new(else_future.unwrap_or_else(|| Expr::Async {
                    body: else_branch.clone(),
                }))),
            })
        }
        Expr::Match { scrutinee, arms } if !arms.is_empty() => {
            let futures = arms
                .iter()
                .map(|arm| branch_await_future(&arm.body))
                .collect::<Vec<_>>();
            if futures.iter().all(Option::is_none) {
                return None;
            }
            let mut hoisted_arms = Vec::with_capacity(arms.len());
            for (arm, future) in arms.iter().zip(futures) {
                let mut arm = arm.clone();
                arm.body = future.unwrap_or_else(|| Expr::Async {
                    body: Box::new(arm.body.clone()),
                });
                hoisted_arms.push(arm);
            }
            Some(Expr::Match {
                scrutinee: scrutinee.clone(),
                arms: hoisted_arms,
            })
        }
        Expr::Loop { body } => {
            let (iteration, _) = terminating_loop_iteration(body)?;
            branch_await_future(&iteration)
        }
        Expr::While {
            condition,
            body,
            post_test,
        } => {
            let (iteration, break_value) = terminating_loop_iteration(body)?;
            if break_value.is_some() {
                return None;
            }
            let iteration = branch_await_future(&iteration)?;
            if *post_test {
                Some(iteration)
            } else {
                Some(Expr::If {
                    condition: condition.clone(),
                    then_branch: Box::new(iteration),
                    else_branch: Some(Box::new(Expr::Async {
                        body: Box::new(Expr::Unit),
                    })),
                })
            }
        }
        _ => None,
    }
}

fn terminating_loop_iteration(body: &Expr) -> Option<(Expr, Option<Expr>)> {
    match body.unlocated() {
        Expr::Break(value) => Some((
            value.as_deref().cloned().unwrap_or(Expr::Unit),
            value.as_deref().cloned(),
        )),
        Expr::Block(statements, tail) => {
            if let Some(Expr::Break(value)) = tail.as_deref().map(Expr::unlocated) {
                return Some((
                    Expr::Block(
                        statements.clone(),
                        Some(Box::new(value.as_deref().cloned().unwrap_or(Expr::Unit))),
                    ),
                    value.as_deref().cloned(),
                ));
            }
            let (last, prefix) = statements.split_last()?;
            let Stmt::Expr(last) = last else {
                return None;
            };
            let Expr::Break(value) = last.unlocated() else {
                return None;
            };
            Some((
                Expr::Block(
                    prefix.to_vec(),
                    Some(Box::new(value.as_deref().cloned().unwrap_or(Expr::Unit))),
                ),
                value.as_deref().cloned(),
            ))
        }
        _ => None,
    }
}

fn branch_await_future(expression: &Expr) -> Option<Expr> {
    if let Some(future) = tail_await_operand(expression) {
        return Some(future);
    }
    let Expr::Block(statements, _) = expression.unlocated() else {
        return None;
    };
    statements
        .iter()
        .any(|statement| {
            let Stmt::Let(binding) = statement else {
                return false;
            };
            matches!(binding.value.unlocated(), Expr::Await(_))
                || hoist_control_await(&binding.value).is_some()
        })
        .then(|| Expr::Async {
            body: Box::new(expression.clone()),
        })
}

fn tail_await_operand(expression: &Expr) -> Option<Expr> {
    match expression.unlocated() {
        Expr::Await(future) => Some((**future).clone()),
        Expr::Block(statements, Some(tail)) => {
            let Expr::Await(future) = tail.unlocated() else {
                return None;
            };
            if statements.is_empty() {
                Some((**future).clone())
            } else {
                Some(Expr::Async {
                    body: Box::new(expression.clone()),
                })
            }
        }
        _ => None,
    }
}

fn non_borrow_binding_names(statements: &[Stmt]) -> Vec<String> {
    let borrowed = borrowed_binding_names(statements);
    statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            (!borrowed.contains(&binding.name)).then(|| binding.name.clone())
        })
        .collect()
}

fn borrowed_binding_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
    let dependencies = statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            Some((
                binding.name.clone(),
                async_initializer_root(&binding.value)?,
            ))
        })
        .collect::<Vec<_>>();
    let mut borrowed = statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            (matches!(binding.annotation, Some(crate::ast::Type::Borrow { .. }))
                || matches!(binding.value.unlocated(), Expr::Borrow { .. }))
            .then(|| binding.name.clone())
        })
        .collect::<std::collections::HashSet<_>>();
    loop {
        let mut changed = false;
        for (binding, referent) in &dependencies {
            if borrowed.contains(referent) {
                changed |= borrowed.insert(binding.clone());
            }
        }
        if !changed {
            return borrowed;
        }
    }
}

fn referenced_names(expression: &Expr) -> std::collections::HashSet<String> {
    let mut expression = expression.clone();
    let mut names = std::collections::HashSet::new();
    super::source_rewrite::visit_expr_mut(&mut expression, &mut |expression| {
        if let Expr::Name(name) = expression.unlocated() {
            names.insert(name.clone());
        }
    });
    names
}

fn async_place_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Name(name) => Some(name.clone()),
        Expr::Member(base, _) | Expr::Index { base, .. } => async_place_root(base),
        _ => None,
    }
}

fn async_initializer_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Borrow { value, .. } => async_place_root(value),
        expression => async_place_root(expression),
    }
}

fn resolve_async_dependency(name: &str, dependencies: &[(String, String)]) -> String {
    let mut resolved = name.to_owned();
    let mut visited = std::collections::HashSet::new();
    while visited.insert(resolved.clone()) {
        let Some((_, referent)) = dependencies
            .iter()
            .rev()
            .find(|(binding, _)| binding == &resolved)
        else {
            break;
        };
        resolved = referent.clone();
    }
    resolved
}
