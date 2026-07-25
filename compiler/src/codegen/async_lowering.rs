use std::collections::HashMap;

use crate::ast::{Expr, ItemOrigin, PassMode, Visibility};
use crate::core::LangItemKind;

use super::hir::{
    AccessBoundary, AssignmentKind, ClosureCaptureMode, ClosureCapturePolicy, ClosureEffectContext,
    FieldLayout, FunctionSig, HirArgument, HirExpr, HirExprKind, HirFunction, HirParam, HirPlace,
    HirReadKind, HirStmt, LocalCapability, ParamSig, StructLayout, Ty,
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
        let lowered = self.lower_local_closure(
            &[],
            body,
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

        self.nominal_accesses.insert(name.clone(), access);
        self.struct_layouts.insert(
            name.clone(),
            StructLayout {
                name: name.clone(),
                fields,
            },
        );
        self.struct_order.push(name.clone());
        let metadata = AsyncFutureInfo {
            resume: closure.function,
            output: closure.result,
            unsafe_effect: closure.unsafe_effect,
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
        };
        self.async_futures.insert(name.clone(), metadata);
        self.register_ready_future_poll(&name);

        HirExpr {
            ty: Ty::Struct(name.clone()),
            kind: HirExprKind::ConstructStruct {
                name,
                fields: values,
            },
        }
    }

    fn register_ready_future_poll(&mut self, name: &str) {
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

        let state_place = async_field_place(0, self_ty.clone(), 0, Ty::I32);
        let condition = HirExpr {
            ty: Ty::Bool,
            kind: HirExprKind::Binary(
                Box::new(HirExpr {
                    ty: Ty::I32,
                    kind: HirExprKind::Read {
                        place: state_place.clone(),
                        kind: HirReadKind::Copy,
                    },
                }),
                crate::ast::BinaryOp::Eq,
                Box::new(HirExpr {
                    ty: Ty::I32,
                    kind: HirExprKind::Integer(0),
                }),
            ),
        };
        let mark_completed = HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::Assign {
                place: state_place,
                value: Box::new(HirExpr {
                    ty: Ty::I32,
                    kind: HirExprKind::Integer(1),
                }),
                assignment: AssignmentKind::Overwrite,
                root_initialized: true,
            },
        };
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
            .collect();
        let ready = HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::ConstructEnum {
                name: poll_name,
                variant: 1,
                fields: vec![(
                    0,
                    HirExpr {
                        ty: future.output.clone(),
                        kind: HirExprKind::Call {
                            function: future.resume,
                            arguments,
                            consumed_callable: None,
                            diverges: future.output == Ty::Never,
                        },
                    },
                )],
            },
        };
        let body = HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(HirExpr {
                    ty: poll_ty.clone(),
                    kind: HirExprKind::Block(
                        vec![HirStmt::Expr(mark_completed)],
                        Some(Box::new(ready)),
                    ),
                }),
                else_branch: Some(Box::new(HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::RawTrap,
                })),
            },
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
}
