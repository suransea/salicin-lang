use std::collections::HashMap;

use crate::ast::{Expr, ItemOrigin, PassMode, Visibility};
use crate::core::LangItemKind;

use super::hir::{
    AccessBoundary, AssignmentKind, ClosureCaptureMode, ClosureCapturePolicy, ClosureEffectContext,
    FieldLayout, FunctionSig, HirArgument, HirExpr, HirExprKind, HirFunction, HirMatchArm,
    HirMatcher, HirParam, HirPatternBinding, HirPlace, HirReadKind, HirStmt, LocalCapability,
    ParamSig, StructLayout, Ty,
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
        let (closure_body, has_tail_await) = match replace_tail_await(body) {
            Some(body) => (body, true),
            None => (body.clone(), false),
        };
        let lowered = self.lower_local_closure(
            &[],
            &closure_body,
            None,
            ClosureEffectContext {
                infer_effects: true,
                ..ClosureEffectContext::default()
            },
            ClosureCapturePolicy::Lexical,
            context,
        );
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
        let awaited = if has_tail_await {
            let Some(awaited) = self.resolve_awaited_future(&closure.result) else {
                return super::lower::error_expr();
            };
            Some(awaited)
        } else {
            None
        };

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
            let (ty, value) = match capture.mode {
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
            };
            fields.push(FieldLayout {
                name: format!("capture.{index}"),
                ty,
                access: access.clone(),
            });
            values.push((index + 1, value));
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
            .map(|awaited| awaited.output.clone())
            .unwrap_or_else(|| closure.result.clone());
        let unsafe_effect = closure.unsafe_effect
            || awaited
                .as_ref()
                .is_some_and(|awaited| awaited.unsafe_effect);
        let metadata = AsyncFutureInfo {
            resume: closure.function,
            output,
            unsafe_effect,
            throws_error: closure.throws_error,
            custom_effects: closure.custom_effects,
            capture_modes: closure
                .captures
                .iter()
                .map(|capture| match capture.mode {
                    ClosureCaptureMode::Shared => PassMode::Borrow,
                    ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                    ClosureCaptureMode::Move => PassMode::Move,
                })
                .collect(),
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
            output,
            poll_ty,
            poll_function,
            unsafe_effect: signature.unsafe_effect,
            field: 0,
        })
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
                    PassMode::Borrow | PassMode::MutBorrow => HirArgument::Copy(HirExpr {
                        ty,
                        kind: HirExprKind::Read {
                            place,
                            kind: HirReadKind::Copy,
                        },
                    }),
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
                    PassMode::Inferred | PassMode::Copy => {
                        unreachable!("async capture modes are normalized while materializing state")
                    }
                }
            })
            .collect::<Vec<_>>();
        let resume = HirExpr {
            ty: future
                .awaited
                .as_ref()
                .map(|awaited| awaited.ty.clone())
                .unwrap_or_else(|| future.output.clone()),
            kind: HirExprKind::Call {
                function: future.resume.clone(),
                arguments,
                consumed_callable: None,
                diverges: future.awaited.is_none() && future.output == Ty::Never,
            },
        };
        let body = if let Some(awaited) = &future.awaited {
            if awaited.poll_ty != poll_ty {
                self.error(format!(
                    "awaited `Future.poll` returned `{}`, expected `{poll_ty}`",
                    awaited.poll_ty
                ));
                return;
            }
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
    let initialize_child = HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::RawInit {
            pointer: Box::new(HirExpr {
                ty: Ty::Pointer {
                    pointee: Box::new(awaited.ty.clone()),
                    mutable: true,
                },
                kind: HirExprKind::RawAddress {
                    place: child_place.clone(),
                },
            }),
            value: Box::new(resume),
        },
    };
    let cold_poll = poll_awaited(self_ty, poll_ty, poll_name, output, awaited, 1);
    let resumed_poll = poll_awaited(self_ty, poll_ty, poll_name, output, awaited, 2);
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::If {
            condition: Box::new(state_is(self_ty, 0)),
            then_branch: Box::new(HirExpr {
                ty: poll_ty.clone(),
                kind: HirExprKind::Block(
                    vec![
                        HirStmt::Expr(initialize_child),
                        HirStmt::Expr(set_state(self_ty, 1)),
                    ],
                    Some(Box::new(cold_poll)),
                ),
            }),
            else_branch: Some(Box::new(HirExpr {
                ty: poll_ty.clone(),
                kind: HirExprKind::If {
                    condition: Box::new(state_is(self_ty, 1)),
                    then_branch: Box::new(resumed_poll),
                    else_branch: Some(Box::new(trap())),
                },
            })),
        },
    }
}

fn poll_awaited(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    output: &Ty,
    awaited: &AwaitedFutureInfo,
    output_local: usize,
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
        local: output_local,
        root_ty: output.clone(),
        projections: Vec::new(),
        dynamic_index: None,
        ty: output.clone(),
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
                        id: output_local,
                        name: "awaited.output".to_owned(),
                        ty: output.clone(),
                        path: vec![1],
                        moves: true,
                    }],
                    guard: None,
                    body: HirExpr {
                        ty: poll_ty.clone(),
                        kind: HirExprKind::Block(
                            vec![
                                HirStmt::Expr(take_child),
                                HirStmt::Expr(set_state(self_ty, 2)),
                            ],
                            Some(Box::new(poll_ready(
                                poll_ty,
                                poll_name,
                                output,
                                HirExpr {
                                    ty: output.clone(),
                                    kind: HirExprKind::Read {
                                        place: output_place,
                                        kind: HirReadKind::Move,
                                    },
                                },
                            ))),
                        ),
                    },
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
    pub(super) output: Ty,
    pub(super) poll_ty: Ty,
    pub(super) poll_function: String,
    pub(super) unsafe_effect: bool,
    pub(super) field: usize,
}

fn replace_tail_await(body: &Expr) -> Option<Expr> {
    let mut body = body.clone();
    match body.unlocated_mut() {
        Expr::Await(operand) => return Some((**operand).clone()),
        Expr::Block(_, Some(tail)) => {
            let Expr::Await(operand) = tail.unlocated() else {
                return None;
            };
            **tail = (**operand).clone();
        }
        _ => return None,
    }
    Some(body)
}
