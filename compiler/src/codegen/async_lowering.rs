use crate::ast::{Expr, ItemOrigin, PassMode, Visibility};

use super::hir::{
    AccessBoundary, ClosureCaptureMode, ClosureCapturePolicy, ClosureEffectContext, FieldLayout,
    HirExpr, HirExprKind, StructLayout, Ty,
};
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
            ClosureEffectContext::default(),
            ClosureCapturePolicy::Lexical,
            context,
        );
        let HirExprKind::LocalClosure(closure) = lowered.kind else {
            return lowered;
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
        let _ = (
            metadata.resume.as_str(),
            &metadata.output,
            metadata.capture_modes.as_slice(),
        );
        self.async_futures.insert(name.clone(), metadata);

        HirExpr {
            ty: Ty::Struct(name.clone()),
            kind: HirExprKind::ConstructStruct {
                name,
                fields: values,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct AsyncFutureInfo {
    pub(super) resume: String,
    pub(super) output: Ty,
    pub(super) capture_modes: Vec<PassMode>,
}
