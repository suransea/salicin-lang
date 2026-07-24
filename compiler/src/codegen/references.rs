use std::collections::HashSet;

use crate::ast::{Expr, PassMode, Type};

use super::flow::{LoanKind, LowerCtx};
use super::hir::{
    AccessKind, HirArgument, HirBinding, HirExpr, HirExprKind, HirIndex, HirPlace, HirStmt, LoanId,
    LocalCapability, ParamSig, ReferenceCallSource, Ty,
};
use super::lower::{display_region, reference_value_types_compatible, TypeProbe};
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_reference_value_expr(
        &mut self,
        expression: &Expr,
        expected: &Ty,
        context: &mut LowerCtx,
    ) -> HirExpr {
        self.lower_reference_value_expr_with_access(expression, expected, context, AccessKind::Auto)
    }

    pub(super) fn lower_reference_value_expr_with_access(
        &mut self,
        expression: &Expr,
        expected: &Ty,
        context: &mut LowerCtx,
        access: AccessKind,
    ) -> HirExpr {
        let saved_access = context.reference_value_access;
        context.reference_value_access = Some(access);
        context.reference_value_depth += 1;
        let mut lowered = self.lower_expr(expression, Some(expected), context);
        context.reference_value_depth -= 1;
        context.reference_value_access = saved_access;
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
            HirExprKind::While {
                condition, body, ..
            } => {
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn promote_returned_reference_loans(
        &mut self,
        function: &str,
        result: &Ty,
        arguments: &[HirArgument],
        temporary_bindings: &[HirBinding],
        temporary_loans: &mut Vec<LoanId>,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) {
        let Ty::Reference {
            mutable: result_mutable,
            region: result_region,
            ..
        } = result
        else {
            return;
        };
        let Some(source) = self.functions.get(function) else {
            self.error(format!(
                "internal error: reference-returning function `{function}` has no source signature"
            ));
            return;
        };
        let source_parameters = source.groups.iter().flatten().cloned().collect::<Vec<_>>();
        if source_parameters.len() != arguments.len() {
            self.error(format!(
                "internal error: reference-returning call `{function}` lost parameter alignment"
            ));
            return;
        }
        let temporary_ids = temporary_bindings
            .iter()
            .map(|binding| binding.id)
            .collect::<HashSet<_>>();
        let mut sources = Vec::new();
        for (parameter, argument) in source_parameters.into_iter().zip(arguments) {
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
            let (place, origin) = match argument {
                HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => (
                    Some(place),
                    context
                        .borrowed_parameter_regions
                        .get(&place.local)
                        .cloned(),
                ),
                HirArgument::Copy(value) | HirArgument::Move(value) => {
                    let place = match &value.kind {
                        HirExprKind::Borrow { place, .. } | HirExprKind::Read { place, .. } => {
                            Some(place)
                        }
                        _ => None,
                    };
                    (place, self.reference_origin_for_hir_expr(value, context))
                }
                HirArgument::CallableCaptureBorrow { .. } => (None, None),
            };
            if let Some(place) = place {
                if temporary_ids.contains(&place.local) {
                    self.error("a returned borrow cannot originate from a temporary call argument");
                }
                if let Some(loan) = place.loan {
                    temporary_loans.retain(|candidate| *candidate != loan);
                    let lexical_loans = &mut context
                        .scopes
                        .last_mut()
                        .expect("call lowering has a lexical scope")
                        .lexical_loans;
                    if !lexical_loans.contains(&loan) {
                        lexical_loans.push(loan);
                    }
                }
            }
            sources.push(origin);
        }
        if sources.is_empty() {
            self.error(format!(
                "reference-returning call `{function}` has no argument for {}",
                display_region(result_region.as_deref())
            ));
        }

        if let Some(Ty::Reference {
            mutable: expected_mutable,
            region: expected_region,
            ..
        }) = expected
        {
            if result_region.is_some() && expected_region != result_region {
                self.error(format!(
                    "returned call region mismatch: expected {}, found {}",
                    display_region(expected_region.as_deref()),
                    display_region(result_region.as_deref())
                ));
            }
            if *expected_mutable && !result_mutable {
                self.error("cannot return a shared call result as a mutable borrow");
            }
            for source in sources {
                match source {
                    Some((source_region, source_mutable)) => {
                        if expected_region.is_some() && source_region != *expected_region {
                            self.error(format!(
                                "returned call argument region mismatch: expected {}, found {}",
                                display_region(expected_region.as_deref()),
                                display_region(source_region.as_deref())
                            ));
                        }
                        if *expected_mutable && !source_mutable {
                            self.error(
                                "cannot return a mutable call result through a shared borrow parameter",
                            );
                        }
                    }
                    None if context.reference_value_depth > 0 => {}
                    None => self.error(
                        "cannot return a call result borrowing a local value; its source must be a region-bound borrow parameter",
                    ),
                }
            }
        }
    }

    pub(super) fn lower_reference_call_argument(
        &mut self,
        argument: &Expr,
        parameter: &ParamSig,
        context: &mut LowerCtx,
        temporary_loans: &mut Vec<LoanId>,
        temporary_bindings: &mut Vec<HirBinding>,
    ) -> HirArgument {
        let argument_is_reference_value = match self.probe_expr_ty(argument, None, context) {
            TypeProbe::Known(actual) | TypeProbe::KnownSource(actual, _) => {
                reference_value_types_compatible(&actual, &parameter.ty)
            }
            TypeProbe::Defaultable(_) | TypeProbe::Unsupported => false,
        };
        if argument_is_reference_value && parameter.mode != PassMode::Inferred {
            return self.lower_reference_value_call_argument(argument, parameter, context);
        }

        if let Some(place) = self.lower_place_without_diagnostic(argument, context) {
            if let Some(argument) = self.lower_reference_place_call_argument(
                place,
                parameter,
                &parameter.ty,
                context,
                temporary_loans,
            ) {
                return argument;
            }
        }
        if argument_is_reference_value {
            return self.lower_reference_value_call_argument(argument, parameter, context);
        }

        let Ty::Reference {
            pointee, mutable, ..
        } = &parameter.ty
        else {
            unreachable!("reference argument lowering requires a reference parameter");
        };
        let value = self.lower_expr(argument, Some(pointee), context);
        self.require_same_type(
            &value.ty,
            pointee,
            format!("argument for parameter `{}`", parameter.name),
        );
        let id = context.fresh_local();
        let ty = value.ty.clone();
        temporary_bindings.push(HirBinding {
            id,
            name: format!("$temporary argument for {}", parameter.name),
            ty: ty.clone(),
            mutable: *mutable,
            value,
        });
        let mut place = HirPlace {
            local: id,
            root_ty: ty.clone(),
            projections: Vec::new(),
            ty,
            capability: LocalCapability::Owned,
            root_mutable: *mutable,
            loan: None,
            indirect: false,
        };
        let kind = if *mutable {
            LoanKind::Mutable
        } else {
            LoanKind::Shared
        };
        if let Some(loan) = self.acquire_loan(&place, kind, false, context) {
            place.loan = Some(loan);
            temporary_loans.push(loan);
        }
        place.capability = if *mutable {
            LocalCapability::MutParam
        } else {
            LocalCapability::SharedParam
        };
        if *mutable {
            HirArgument::MutBorrow(place)
        } else {
            HirArgument::SharedBorrow(place)
        }
    }

    fn lower_reference_value_call_argument(
        &mut self,
        argument: &Expr,
        parameter: &ParamSig,
        context: &mut LowerCtx,
    ) -> HirArgument {
        let mode = self.effective_pass_mode(parameter.mode, &parameter.ty);
        let access = match mode {
            PassMode::Copy => AccessKind::Copy,
            PassMode::Move => AccessKind::Move,
            PassMode::Borrow | PassMode::MutBorrow | PassMode::Inferred => AccessKind::Auto,
        };
        let value =
            self.lower_reference_value_expr_with_access(argument, &parameter.ty, context, access);
        self.require_same_type(
            &value.ty,
            &parameter.ty,
            format!("argument for parameter `{}`", parameter.name),
        );
        if mode == PassMode::Copy {
            if !self.is_copy_type(&parameter.ty) {
                let ty = self.diagnostic_type_name(&parameter.ty);
                self.error(format!(
                    "parameter `{}` requires Copy, but `{}` does not implement Copy",
                    parameter.name, ty
                ));
            }
            HirArgument::Copy(value)
        } else {
            HirArgument::Move(value)
        }
    }

    pub(super) fn lower_reference_place_call_argument(
        &mut self,
        mut place: HirPlace,
        parameter: &ParamSig,
        expected: &Ty,
        context: &mut LowerCtx,
        temporary_loans: &mut Vec<LoanId>,
    ) -> Option<HirArgument> {
        if reference_value_types_compatible(&place.ty, expected) {
            let mut value = self.access_place(place, AccessKind::Auto, context);
            value.ty = expected.clone();
            let mode = self.effective_pass_mode(parameter.mode, expected);
            return Some(if mode == PassMode::Copy {
                HirArgument::Copy(value)
            } else {
                HirArgument::Move(value)
            });
        }
        let Ty::Reference {
            pointee, mutable, ..
        } = expected
        else {
            return None;
        };
        if place.ty != **pointee {
            return None;
        }
        if *mutable {
            self.ensure_writable(&place);
        }
        let kind = if *mutable {
            LoanKind::Mutable
        } else {
            LoanKind::Shared
        };
        if let Some(loan) = self.acquire_loan(&place, kind, false, context) {
            place.loan = Some(loan);
            temporary_loans.push(loan);
        }
        place.capability = if *mutable {
            LocalCapability::MutParam
        } else {
            LocalCapability::SharedParam
        };
        Some(if *mutable {
            HirArgument::MutBorrow(place)
        } else {
            HirArgument::SharedBorrow(place)
        })
    }
}
