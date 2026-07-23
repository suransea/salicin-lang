use crate::ast::{Expr, PassMode, Type};

use super::flow::LowerCtx;
use super::hir::{
    HirArgument, HirExpr, HirExprKind, HirIndex, HirStmt, LoanId, ReferenceCallSource, Ty,
};
use super::lower::{display_region, reference_value_types_compatible};
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_reference_value_expr(
        &mut self,
        expression: &Expr,
        expected: &Ty,
        context: &mut LowerCtx,
    ) -> HirExpr {
        context.reference_value_depth += 1;
        let mut lowered = self.lower_expr(expression, Some(expected), context);
        context.reference_value_depth -= 1;
        if reference_value_types_compatible(&lowered.ty, expected) {
            lowered.ty = expected.clone();
        }
        lowered
    }

    pub(super) fn reference_origin_for_hir_expr(
        &self,
        expression: &HirExpr,
        context: &LowerCtx,
    ) -> Option<(Option<String>, bool)> {
        if !matches!(expression.ty, Ty::Reference { .. }) {
            return None;
        }
        match &expression.kind {
            HirExprKind::Borrow { place, .. } | HirExprKind::Read { place, .. } => context
                .borrowed_parameter_regions
                .get(&place.local)
                .cloned(),
            HirExprKind::RawBorrow { anchor, .. } => context
                .borrowed_parameter_regions
                .get(&anchor.local)
                .cloned(),
            HirExprKind::Block(_, Some(tail)) => self.reference_origin_for_hir_expr(tail, context),
            HirExprKind::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                let then_origin = self.reference_origin_for_hir_expr(then_branch, context)?;
                let else_origin = self.reference_origin_for_hir_expr(else_branch, context)?;
                (then_origin == else_origin).then_some(then_origin)
            }
            HirExprKind::Call {
                function,
                arguments,
                ..
            } => {
                let sources = self.reference_call_source_expressions(function, arguments)?;
                let mut origins = sources.into_iter().map(|source| match source {
                    ReferenceCallSource::Place(place) => context
                        .borrowed_parameter_regions
                        .get(&place.local)
                        .cloned(),
                    ReferenceCallSource::Expression(value) => {
                        self.reference_origin_for_hir_expr(value, context)
                    }
                });
                let first = origins.next()??;
                origins
                    .all(|origin| origin.as_ref() == Some(&first))
                    .then_some(first)
            }
            _ => None,
        }
    }

    fn reference_call_source_expressions<'a>(
        &self,
        function: &str,
        arguments: &'a [HirArgument],
    ) -> Option<Vec<ReferenceCallSource<'a>>> {
        let result_region = match self.signatures.get(function)?.result.as_ref()? {
            Ty::Reference { region, .. } => region,
            _ => return None,
        };
        let parameters = self.functions.get(function)?.groups.iter().flatten();
        let mut sources = Vec::new();
        for (parameter, argument) in parameters.zip(arguments) {
            let parameter_region = match (&parameter.mode, &parameter.ty) {
                (PassMode::Borrow | PassMode::MutBorrow, _) => Some(&parameter.region),
                (_, Type::Borrow { region, .. }) => Some(region),
                _ => None,
            };
            let Some(parameter_region) = parameter_region else {
                continue;
            };
            if result_region.is_some() && parameter_region != result_region {
                continue;
            }
            sources.push(match argument {
                HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                    ReferenceCallSource::Place(place)
                }
                HirArgument::Copy(value) | HirArgument::Move(value) => {
                    ReferenceCallSource::Expression(value)
                }
                HirArgument::CallableCaptureBorrow { .. } => return None,
            });
        }
        Some(sources)
    }

    pub(super) fn reference_loans_for_hir_expr(
        &self,
        expression: &HirExpr,
        context: &LowerCtx,
    ) -> Vec<LoanId> {
        if !matches!(expression.ty, Ty::Reference { .. }) {
            return Vec::new();
        }
        let mut loans = match &expression.kind {
            HirExprKind::Borrow { place, .. } => place.loan.into_iter().collect(),
            HirExprKind::RawBorrow { anchor, .. } => anchor.loan.into_iter().collect(),
            HirExprKind::Read { place, .. } => context
                .reference_loans
                .get(&place.local)
                .cloned()
                .unwrap_or_default(),
            HirExprKind::Block(_, Some(tail)) => self.reference_loans_for_hir_expr(tail, context),
            HirExprKind::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                let mut loans = self.reference_loans_for_hir_expr(then_branch, context);
                loans.extend(self.reference_loans_for_hir_expr(else_branch, context));
                loans
            }
            HirExprKind::Call {
                function,
                arguments,
                ..
            } => self
                .reference_call_source_expressions(function, arguments)
                .unwrap_or_default()
                .into_iter()
                .flat_map(|source| match source {
                    ReferenceCallSource::Place(place) => place
                        .loan
                        .into_iter()
                        .chain(
                            context
                                .reference_loans
                                .get(&place.local)
                                .into_iter()
                                .flatten()
                                .copied(),
                        )
                        .collect::<Vec<_>>(),
                    ReferenceCallSource::Expression(value) => {
                        self.reference_loans_for_hir_expr(value, context)
                    }
                })
                .collect(),
            _ => Vec::new(),
        };
        loans.sort_unstable();
        loans.dedup();
        loans
    }

    pub(super) fn validate_reference_escape_value(
        &mut self,
        value: &HirExpr,
        expected: &Ty,
        context: &LowerCtx,
    ) {
        if value.ty == Ty::Error || self.is_uninhabited_type(&value.ty) {
            return;
        }
        if !matches!(value.ty, Ty::Reference { .. }) {
            return;
        }
        let Ty::Reference {
            mutable: expected_mutable,
            region: expected_region,
            ..
        } = expected
        else {
            return;
        };
        let Some((source_region, source_mutable)) =
            self.reference_origin_for_hir_expr(value, context)
        else {
            self.error("cannot return a reference value whose source is local or cannot be proven");
            return;
        };
        if expected_region.is_some() && source_region != *expected_region {
            self.error(format!(
                "returned reference value region mismatch: expected {}, found {}",
                display_region(expected_region.as_deref()),
                display_region(source_region.as_deref())
            ));
        }
        if *expected_mutable && !source_mutable {
            self.error("cannot return a mutable reference value through a shared source");
        }
    }

    pub(super) fn validate_explicit_reference_returns(
        &mut self,
        expression: &HirExpr,
        expected: &Ty,
        context: &LowerCtx,
    ) {
        match &expression.kind {
            HirExprKind::Return(Some(value)) => {
                self.validate_reference_escape_value(value, expected, context);
            }
            HirExprKind::Array(values) => {
                for value in values {
                    self.validate_explicit_reference_returns(value, expected, context);
                }
            }
            HirExprKind::Index { base, index, .. } => {
                self.validate_explicit_reference_returns(base, expected, context);
                if let HirIndex::Dynamic(index) = index {
                    self.validate_explicit_reference_returns(index, expected, context);
                }
            }
            HirExprKind::Unary(_, value)
            | HirExprKind::Field { base: value, .. }
            | HirExprKind::RawLoad(value)
            | HirExprKind::RawTake(value)
            | HirExprKind::Forget(value)
            | HirExprKind::Loop { body: value } => {
                self.validate_explicit_reference_returns(value, expected, context);
            }
            HirExprKind::Binary(left, _, right) => {
                self.validate_explicit_reference_returns(left, expected, context);
                self.validate_explicit_reference_returns(right, expected, context);
            }
            HirExprKind::InvokeContinuation {
                continuation,
                argument,
            }
            | HirExprKind::TailInvokeContinuation {
                continuation,
                argument,
                ..
            } => {
                self.validate_explicit_reference_returns(continuation, expected, context);
                self.validate_explicit_reference_returns(argument, expected, context);
            }
            HirExprKind::InvokeEffectCallable {
                action,
                input,
                continuation,
            } => {
                self.validate_explicit_reference_returns(action, expected, context);
                self.validate_explicit_reference_returns(input, expected, context);
                self.validate_explicit_reference_returns(continuation, expected, context);
            }
            HirExprKind::Assign { value, .. } => {
                self.validate_explicit_reference_returns(value, expected, context);
            }
            HirExprKind::Call { arguments, .. }
            | HirExprKind::Partial {
                captures: arguments,
                ..
            } => {
                for argument in arguments {
                    if let HirArgument::Copy(value) | HirArgument::Move(value) = argument {
                        self.validate_explicit_reference_returns(value, expected, context);
                    }
                }
            }
            HirExprKind::TailCall { arguments, .. } => {
                for argument in arguments {
                    if let HirArgument::Copy(value) | HirArgument::Move(value) = argument {
                        self.validate_explicit_reference_returns(value, expected, context);
                    }
                }
            }
            HirExprKind::IndirectCall {
                callee, arguments, ..
            } => {
                self.validate_explicit_reference_returns(callee, expected, context);
                for argument in arguments {
                    if let HirArgument::Copy(value) | HirArgument::Move(value) = argument {
                        self.validate_explicit_reference_returns(value, expected, context);
                    }
                }
            }
            HirExprKind::LocalClosure(closure) => {
                for capture in &closure.captures {
                    if let Some(value) = &capture.value {
                        self.validate_explicit_reference_returns(value, expected, context);
                    }
                }
            }
            HirExprKind::ConstructStruct { fields, .. }
            | HirExprKind::ConstructEnum { fields, .. } => {
                for (_, value) in fields {
                    self.validate_explicit_reference_returns(value, expected, context);
                }
            }
            HirExprKind::RawStore { pointer, value } | HirExprKind::RawInit { pointer, value } => {
                self.validate_explicit_reference_returns(pointer, expected, context);
                self.validate_explicit_reference_returns(value, expected, context);
            }
            HirExprKind::RawOffset { pointer, index } => {
                self.validate_explicit_reference_returns(pointer, expected, context);
                self.validate_explicit_reference_returns(index, expected, context);
            }
            HirExprKind::RawAlloc { size, align } => {
                self.validate_explicit_reference_returns(size, expected, context);
                self.validate_explicit_reference_returns(align, expected, context);
            }
            HirExprKind::RawDealloc {
                pointer,
                size,
                align,
            } => {
                self.validate_explicit_reference_returns(pointer, expected, context);
                self.validate_explicit_reference_returns(size, expected, context);
                self.validate_explicit_reference_returns(align, expected, context);
            }
            HirExprKind::Block(statements, tail) => {
                for statement in statements {
                    match statement {
                        HirStmt::Let(binding) => self.validate_explicit_reference_returns(
                            &binding.value,
                            expected,
                            context,
                        ),
                        HirStmt::Expr(value) => {
                            self.validate_explicit_reference_returns(value, expected, context)
                        }
                    }
                }
                if let Some(tail) = tail {
                    self.validate_explicit_reference_returns(tail, expected, context);
                }
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.validate_explicit_reference_returns(condition, expected, context);
                self.validate_explicit_reference_returns(then_branch, expected, context);
                if let Some(else_branch) = else_branch {
                    self.validate_explicit_reference_returns(else_branch, expected, context);
                }
            }
            HirExprKind::While { condition, body } => {
                self.validate_explicit_reference_returns(condition, expected, context);
                self.validate_explicit_reference_returns(body, expected, context);
            }
            HirExprKind::Break(Some(value)) => {
                self.validate_explicit_reference_returns(value, expected, context);
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.validate_explicit_reference_returns(scrutinee, expected, context);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.validate_explicit_reference_returns(guard, expected, context);
                    }
                    self.validate_explicit_reference_returns(&arm.body, expected, context);
                }
            }
            HirExprKind::Integer(_)
            | HirExprKind::Bool(_)
            | HirExprKind::Unit
            | HirExprKind::Read { .. }
            | HirExprKind::Global(_)
            | HirExprKind::Function(_)
            | HirExprKind::PartialCapture { .. }
            | HirExprKind::EraseContinuation { .. }
            | HirExprKind::EraseEffectCallable { .. }
            | HirExprKind::Borrow { .. }
            | HirExprKind::RawAddress { .. }
            | HirExprKind::RawBorrow { .. }
            | HirExprKind::RawTrap
            | HirExprKind::LayoutQuery { .. }
            | HirExprKind::Return(None)
            | HirExprKind::Break(None)
            | HirExprKind::Continue => {}
        }
    }
}
