use std::collections::HashMap;

use crate::ast::{BinaryOp, CallArg, Expr, Pattern, Stmt, Type, UnaryOp, VariantFields};

use super::calls;
use super::flow::{LocalInfo, LowerCtx};
use super::hir::{FunctionTy, LocalCapability, Ty};
use super::lower::{
    flatten_call, integer_literal_value, reference_value_types_compatible, TypeProbe,
};
use super::registry::NominalKind;
use super::source_rewrite::{substitute_function_types, substitute_type_parameters};
use super::Analyzer;

impl Analyzer {
    pub(super) fn string_ty(&self) -> Option<Ty> {
        self.collection
            .struct_layouts
            .iter()
            .find(|(_, layout)| layout.source_name == "string")
            .map(|(name, _)| Ty::Struct(name.clone()))
            .or_else(|| {
                self.collection
                    .struct_defs
                    .contains_key("core::string::string")
                    .then(|| Ty::Struct("core::string::string".to_owned()))
            })
    }

    pub(super) fn probe_expr_ty(
        &self,
        expression: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        match expression {
            Expr::Located { value, .. } => self.probe_expr_ty(value, hint, context),
            Expr::Type(_) => TypeProbe::Unsupported,
            Expr::Integer(_) => hint
                .filter(|ty| ty.is_integer())
                .cloned()
                .map_or(TypeProbe::Defaultable(Ty::I32), TypeProbe::Known),
            Expr::Bool(_) => TypeProbe::Known(Ty::Bool),
            Expr::String(_) => hint
                .filter(|hint| **hint != Ty::Error)
                .cloned()
                .map(TypeProbe::Known)
                .or_else(|| self.string_ty().map(TypeProbe::Known))
                .unwrap_or(TypeProbe::Unsupported),
            Expr::Unit => TypeProbe::Known(Ty::Unit),
            Expr::Tuple(fields) => {
                let expected_fields = match hint {
                    Some(Ty::Tuple(expected)) if expected.len() == fields.len() => Some(expected),
                    _ => None,
                };
                let mut defaultable = false;
                let mut types = Vec::with_capacity(fields.len());
                for (index, field) in fields.iter().enumerate() {
                    match self.probe_expr_ty(
                        field,
                        expected_fields.and_then(|expected| expected.get(index)),
                        context,
                    ) {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => types.push(ty),
                        TypeProbe::Defaultable(ty) => {
                            defaultable = true;
                            types.push(ty);
                        }
                        TypeProbe::Unsupported => return TypeProbe::Unsupported,
                    }
                }
                if defaultable {
                    TypeProbe::Defaultable(Ty::Tuple(types))
                } else {
                    TypeProbe::Known(Ty::Tuple(types))
                }
            }
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name) {
                    self.probe_reference_hint(expression, local.ty.clone(), hint, context)
                } else if context.has_type_parameter(name) {
                    TypeProbe::Unsupported
                } else if let Some(Some(annotation)) = self.lowering.global_annotations.get(name) {
                    TypeProbe::Known(annotation.clone())
                } else if let Some(global) = self.lowering.hir_globals.get(name) {
                    TypeProbe::Known(global.ty.clone())
                } else if let Some(signature) = self.lowering.signatures.get(name) {
                    signature
                        .function_ty()
                        .map_or(TypeProbe::Unsupported, TypeProbe::Known)
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Borrow { mutable, value, .. } => {
                let Some(pointee) = self.probe_place_ty(value, context) else {
                    return TypeProbe::Unsupported;
                };
                TypeProbe::Known(Ty::Reference {
                    pointee: Box::new(pointee),
                    mutable: *mutable,
                    region: match hint {
                        Some(Ty::Reference { region, .. }) => region.clone(),
                        _ => None,
                    },
                })
            }
            Expr::Unsafe(body) => self.probe_expr_ty(body, hint, context),
            Expr::Unary(operator @ (UnaryOp::Neg | UnaryOp::Not), operand) => {
                self.probe_unary_ty(*operator, operand, hint, context)
            }
            Expr::Unary(UnaryOp::Deref, operand) => {
                match self.probe_expr_ty(operand, None, context) {
                    TypeProbe::Known(Ty::Pointer { pointee, .. })
                    | TypeProbe::KnownSource(Ty::Pointer { pointee, .. }, _) => {
                        TypeProbe::Known(*pointee)
                    }
                    _ => TypeProbe::Unsupported,
                }
            }
            Expr::Binary(left, operator, right) => match operator {
                BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => TypeProbe::Known(Ty::Bool),
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Rem
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::Shl
                | BinaryOp::Shr => self.probe_arithmetic_ty(*operator, left, right, hint, context),
            },
            Expr::Coalesce(left, right) => self.probe_coalesce_ty(left, right, hint, context),
            Expr::HandlerCoalesce {
                success, fallback, ..
            } => {
                let success = self.probe_expr_ty(success, hint, context);
                if matches!(success, TypeProbe::Unsupported) {
                    self.probe_expr_ty(fallback, hint, context)
                } else {
                    success
                }
            }
            Expr::HandlerChainCall(chain) => {
                let success = self.probe_expr_ty(&chain.success, hint, context);
                if matches!(success, TypeProbe::Unsupported) {
                    self.probe_expr_ty(&chain.residual, hint, context)
                } else {
                    success
                }
            }
            Expr::Try(value) => {
                let probe = self.probe_expr_ty(value, None, context);
                let Some(info) = self.standard_fallible_info_for_probe(&probe) else {
                    return TypeProbe::Unsupported;
                };
                match info.payload_source {
                    Some(source) => TypeProbe::KnownSource(info.payload, source),
                    None => TypeProbe::Known(info.payload),
                }
            }
            Expr::Throw(_) => TypeProbe::Unsupported,
            Expr::DoBlock { body } => self.probe_expr_ty(body, hint, context),
            Expr::Array(elements) => {
                if let Some(Ty::Array(element, length)) = hint {
                    if *length != elements.len() as u64 {
                        return TypeProbe::Unsupported;
                    }
                    return TypeProbe::Known(Ty::Array(element.clone(), *length));
                }
                if let Some(hint) = hint.filter(|hint| **hint != Ty::Error) {
                    if let Some(element) =
                        self.literal_protocol_element("core::literal::array_literal", hint)
                    {
                        if elements.iter().all(|item| {
                            matches!(
                                self.probe_expr_ty(item, Some(&element), context),
                                TypeProbe::Known(ref ty) | TypeProbe::KnownSource(ref ty, _)
                                    if ty == &element
                            ) || matches!(
                                self.probe_expr_ty(item, Some(&element), context),
                                TypeProbe::Defaultable(ref ty)
                                    if ty.is_integer() && element.is_integer()
                            )
                        }) {
                            return TypeProbe::Known(hint.clone());
                        }
                        return TypeProbe::Unsupported;
                    }
                }
                let Some(first) = elements.first() else {
                    return TypeProbe::Unsupported;
                };
                let first = self.probe_expr_ty(first, None, context);
                let mut exact = match &first {
                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => Some(ty.clone()),
                    TypeProbe::Defaultable(_) => None,
                    TypeProbe::Unsupported => return TypeProbe::Unsupported,
                };
                let mut probes = vec![first];
                for item in elements.iter().skip(1) {
                    let probe = self.probe_expr_ty(item, exact.as_ref(), context);
                    match &probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => {
                            if exact.as_ref().is_some_and(|exact| exact != ty) {
                                return TypeProbe::Unsupported;
                            }
                            exact.get_or_insert_with(|| ty.clone());
                        }
                        TypeProbe::Defaultable(_) => {}
                        TypeProbe::Unsupported => return TypeProbe::Unsupported,
                    }
                    probes.push(probe);
                }
                if let Some(element) = exact {
                    if probes.iter().all(|probe| match probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty == &element,
                        TypeProbe::Defaultable(ty) => ty.is_integer() && element.is_integer(),
                        TypeProbe::Unsupported => false,
                    }) {
                        TypeProbe::Known(Ty::Array(Box::new(element), elements.len() as u64))
                    } else {
                        TypeProbe::Unsupported
                    }
                } else if probes
                    .iter()
                    .all(|probe| matches!(probe, TypeProbe::Defaultable(ty) if ty == &Ty::I32))
                {
                    TypeProbe::Defaultable(Ty::Array(Box::new(Ty::I32), elements.len() as u64))
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Index { base, .. } => match self.probe_expr_ty(base, None, context) {
                TypeProbe::Known(Ty::Array(element, _))
                | TypeProbe::KnownSource(Ty::Array(element, _), _) => {
                    self.probe_reference_hint(expression, *element, hint, context)
                }
                _ => TypeProbe::Unsupported,
            },
            Expr::Member(base, member) => {
                if let Some((NominalKind::Enum, ty, source)) =
                    self.probe_nominal_type_head(base, context)
                {
                    if matches!(
                        self.probe_enum_variant_fields(&source, member),
                        Some(VariantFields::Unit)
                    ) {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
                match self.probe_expr_ty(base, None, context) {
                    TypeProbe::Known(Ty::Tuple(fields))
                    | TypeProbe::KnownSource(Ty::Tuple(fields), _) => {
                        let Ok(index) = member.parse::<usize>() else {
                            return TypeProbe::Unsupported;
                        };
                        fields
                            .get(index)
                            .cloned()
                            .map_or(TypeProbe::Unsupported, |field| {
                                self.probe_reference_hint(expression, field, hint, context)
                            })
                    }
                    TypeProbe::Known(Ty::Struct(name))
                    | TypeProbe::KnownSource(Ty::Struct(name), _) => {
                        self.probe_struct_field_ty(expression, &name, member, hint, context)
                    }
                    TypeProbe::Known(Ty::Reference { pointee, .. })
                    | TypeProbe::KnownSource(Ty::Reference { pointee, .. }, _) => {
                        let Ty::Struct(name) = pointee.as_ref() else {
                            return TypeProbe::Unsupported;
                        };
                        self.probe_struct_field_ty(expression, name, member, hint, context)
                    }
                    _ => TypeProbe::Unsupported,
                }
            }
            Expr::ChainMember(base, member) => {
                self.probe_chain_ty(base, member, None, hint, context)
            }
            Expr::Call(_, _) => self.probe_call_ty(expression, hint, context),
            Expr::StructLiteral {
                constructor,
                fields,
            } => self.probe_struct_literal_ty(constructor, fields, hint, context),
            Expr::Block(statements, tail) => {
                let mut block_context = context.clone();
                block_context.push_scope();
                for statement in statements {
                    let Stmt::Let(binding) = statement else {
                        if matches!(
                            statement,
                            Stmt::Expr(
                                Expr::Return(_)
                                    | Expr::Break(_)
                                    | Expr::Throw(_)
                                    | Expr::While { .. }
                                    | Expr::Loop { .. }
                            )
                        ) {
                            return TypeProbe::Unsupported;
                        }
                        continue;
                    };
                    let annotation = binding.annotation.as_ref().and_then(|source| {
                        let mut source = source.clone();
                        substitute_type_parameters(&mut source, &block_context.type_substitutions);
                        self.probe_source_ty(&source)
                    });
                    let value =
                        self.probe_expr_ty(&binding.value, annotation.as_ref(), &block_context);
                    let inferred = match value {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => Some(ty),
                        TypeProbe::Unsupported => None,
                    };
                    let Some(ty) = annotation.or(inferred) else {
                        continue;
                    };
                    let id = block_context.fresh_local();
                    block_context.insert_local(
                        binding.name.clone(),
                        LocalInfo {
                            id,
                            ty,
                            mutable: binding.mutable,
                            capability: LocalCapability::Owned,
                            alias: None,
                            partial: None,
                            closure: None,
                        },
                    );
                }
                tail.as_ref().map_or(TypeProbe::Known(Ty::Unit), |tail| {
                    self.probe_expr_ty(tail, hint, &block_context)
                })
            }
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => self.probe_conditional_result_ty(then_branch, else_branch, hint, context),
            Expr::Match { arms, .. } => {
                let [first, second] = arms.as_slice() else {
                    return TypeProbe::Unsupported;
                };
                if first.guard.is_some() || second.guard.is_some() {
                    return TypeProbe::Unsupported;
                }
                let (then_branch, else_branch) = match (&first.pattern, &second.pattern) {
                    (Pattern::Bool(true), Pattern::Bool(false)) => (&first.body, &second.body),
                    (Pattern::Bool(false), Pattern::Bool(true)) => (&second.body, &first.body),
                    _ => return TypeProbe::Unsupported,
                };
                self.probe_conditional_result_ty(then_branch, else_branch, hint, context)
            }
            Expr::Assign(_, _)
            | Expr::CompoundAssign(_, _, _)
            | Expr::Async { .. }
            | Expr::Await(_)
            | Expr::Closure(_, _)
            | Expr::PatternClosure { .. }
            | Expr::If { .. }
            | Expr::Return(_)
            | Expr::While { .. }
            | Expr::Loop { .. }
            | Expr::Break(_)
            | Expr::Continue => TypeProbe::Unsupported,
        }
    }

    pub(super) fn probe_conditional_result_ty(
        &self,
        then_branch: &Expr,
        else_branch: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let then_ty = self.probe_expr_ty(then_branch, hint, context);
        let else_ty = self.probe_expr_ty(else_branch, hint, context);
        if then_ty == else_ty {
            return then_ty;
        }
        match (then_ty, else_ty) {
            (TypeProbe::Defaultable(default), exact) | (exact, TypeProbe::Defaultable(default)) => {
                match exact {
                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                        if default.is_integer() && ty.is_integer() =>
                    {
                        TypeProbe::Known(ty)
                    }
                    _ => TypeProbe::Unsupported,
                }
            }
            (TypeProbe::Known(left), TypeProbe::KnownSource(right, source))
            | (TypeProbe::KnownSource(right, source), TypeProbe::Known(left))
                if left == right =>
            {
                TypeProbe::KnownSource(left, source)
            }
            _ => TypeProbe::Unsupported,
        }
    }

    pub(super) fn probe_place_ty(&self, expression: &Expr, context: &LowerCtx) -> Option<Ty> {
        match expression {
            Expr::Name(name) => {
                let local = context.lookup(name)?;
                if let Some(alias) = &local.alias {
                    return Some(alias.ty.clone());
                }
                match &local.ty {
                    Ty::Reference { pointee, .. } => Some(pointee.as_ref().clone()),
                    ty => Some(ty.clone()),
                }
            }
            Expr::Member(base, member) => match self.probe_place_ty(base, context)? {
                Ty::Tuple(fields) => fields.get(member.parse::<usize>().ok()?).cloned(),
                Ty::Struct(name) => self
                    .collection
                    .struct_layouts
                    .get(&name)
                    .and_then(|layout| layout.fields.iter().find(|field| field.name == *member))
                    .filter(|field| Self::access_boundary_allows(&context.origin, &field.access))
                    .map(|field| field.ty.clone()),
                _ => None,
            },
            Expr::Index { base, index } => {
                let Ty::Array(element, length) = self.probe_place_ty(base, context)? else {
                    return None;
                };
                let index = u64::try_from(integer_literal_value(index)?).ok()?;
                (index < length).then(|| element.as_ref().clone())
            }
            _ => None,
        }
    }

    pub(super) fn probe_reference_hint(
        &self,
        expression: &Expr,
        actual: Ty,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let Some(expected @ Ty::Reference { pointee, .. }) = hint else {
            return TypeProbe::Known(actual);
        };
        if reference_value_types_compatible(&actual, expected)
            || (actual == **pointee && self.probe_borrowable_place(expression, context))
        {
            TypeProbe::Known(expected.clone())
        } else {
            TypeProbe::Known(actual)
        }
    }

    pub(super) fn probe_borrowable_place(&self, expression: &Expr, context: &LowerCtx) -> bool {
        match expression {
            Expr::Name(name) => context.lookup(name).is_some(),
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.probe_borrowable_place(base, context)
            }
            Expr::Index { base, .. } => self.probe_borrowable_place(base, context),
            _ => false,
        }
    }

    pub(super) fn probe_struct_field_ty(
        &self,
        expression: &Expr,
        struct_name: &str,
        member: &str,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        self.collection
            .struct_layouts
            .get(struct_name)
            .and_then(|layout| layout.fields.iter().find(|field| field.name == *member))
            .filter(|field| Self::access_boundary_allows(&context.origin, &field.access))
            .map(|field| self.probe_reference_hint(expression, field.ty.clone(), hint, context))
            .unwrap_or(TypeProbe::Unsupported)
    }

    pub(super) fn probe_struct_literal_ty(
        &self,
        constructor: &Expr,
        fields: &[CallArg],
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        if fields.iter().any(|field| field.label.is_none()) {
            return TypeProbe::Unsupported;
        }
        let mut groups = Vec::new();
        let root = flatten_call(constructor, &mut groups);
        let Expr::Name(name) = root else {
            return TypeProbe::Unsupported;
        };
        if context.shadows_top_level_name(name) {
            return TypeProbe::Unsupported;
        }
        if groups.is_empty()
            && self
                .collection
                .struct_layouts
                .get(name)
                .is_some_and(|layout| {
                    layout
                        .fields
                        .iter()
                        .all(|field| Self::access_boundary_allows(&context.origin, &field.access))
                        && fields.len() == layout.fields.len()
                        && fields.iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                layout.fields.iter().any(|field| field.name == *label)
                            })
                        })
                })
        {
            return TypeProbe::KnownSource(
                Ty::Struct(name.clone()),
                Type::Named(name.clone(), Vec::new()),
            );
        }
        if self.collection.struct_templates.contains_key(name) {
            if let Some((NominalKind::Struct, ty, source)) =
                self.probe_generic_nominal_type_head(name, &groups, context)
            {
                let template = &self.collection.struct_templates[name];
                if self.source_fields_are_accessible(name, &template.fields, &context.origin)
                    && fields.len() == template.fields.len()
                    && fields.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            template.fields.iter().any(|field| field.name == *label)
                        })
                    })
                {
                    return TypeProbe::KnownSource(ty, source);
                }
            }
            if let Some(hint @ Ty::Struct(canonical)) = hint {
                if self
                    .collection
                    .nominal_instances
                    .get(canonical)
                    .is_some_and(|instance| {
                        instance.key.kind == NominalKind::Struct && instance.key.template == *name
                    })
                    && self
                        .collection
                        .struct_layouts
                        .get(canonical)
                        .is_some_and(|layout| {
                            layout.fields.iter().all(|field| {
                                Self::access_boundary_allows(&context.origin, &field.access)
                            }) && fields.len() == layout.fields.len()
                                && fields.iter().all(|argument| {
                                    argument.label.as_ref().is_some_and(|label| {
                                        layout.fields.iter().any(|field| field.name == *label)
                                    })
                                })
                        })
                {
                    if let Some(source) = self.source_type_for_ty(hint) {
                        return TypeProbe::KnownSource(hint.clone(), source);
                    }
                    return TypeProbe::Known(hint.clone());
                }
            }
        }
        TypeProbe::Unsupported
    }

    pub(super) fn probe_function_candidate_call_ty(
        &self,
        canonical: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> TypeProbe {
        if let Some(signature) = self.lowering.signatures.get(canonical) {
            if groups.len() > signature.groups.len()
                || groups
                    .iter()
                    .zip(&signature.groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.failure_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafety: signature.unsafety,
                failure_error: signature.failure_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }

        let Some(template) = self.collection.function_templates.get(canonical) else {
            return TypeProbe::Unsupported;
        };
        let compile_group_count = template.compile_groups.len();
        if groups.len() < compile_group_count
            || groups.len() > compile_group_count + template.groups.len()
        {
            return TypeProbe::Unsupported;
        }
        let mut substitutions = HashMap::new();
        for (parameters, supplied) in template
            .compile_groups
            .iter()
            .zip(groups.iter().take(compile_group_count))
        {
            let Some(sources) =
                self.probe_compile_group_sources(parameters, supplied, &context.type_substitutions)
            else {
                return TypeProbe::Unsupported;
            };
            for (parameter, source) in parameters.iter().zip(sources) {
                substitutions.insert(parameter.name.clone(), source);
            }
        }

        let mut function = template.clone();
        substitute_function_types(&mut function, &substitutions);
        let runtime_groups = &groups[compile_group_count..];
        if runtime_groups
            .iter()
            .zip(&function.groups)
            .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            return TypeProbe::Unsupported;
        }
        let Some(result_source) = function.return_type.clone() else {
            return TypeProbe::Unsupported;
        };
        let Some(result) = self.probe_source_ty(&result_source) else {
            return TypeProbe::Unsupported;
        };
        if runtime_groups.len() == function.groups.len() {
            if function.effects.failure.is_some() {
                let Some(info) = self.standard_fallible_info_for_ty(&result) else {
                    return TypeProbe::Unsupported;
                };
                return TypeProbe::Known(info.payload);
            }
            return TypeProbe::KnownSource(result, result_source);
        }

        let remaining = function.groups[runtime_groups.len()..]
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|parameter| self.probe_source_ty(&parameter.ty))
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>();
        if let Some(groups) = remaining {
            let failure_error = match function.effects.failure.as_deref() {
                Some(error) => {
                    let Some(error) = self.probe_source_ty(error) else {
                        return TypeProbe::Unsupported;
                    };
                    Some(Box::new(error))
                }
                None => None,
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups,
                unsafety: self.function_effects_unsafe(&function.effects),
                failure_error,
                custom_effects: self.function_effects_custom_identities(&function.effects),
                result: Box::new(result),
            }));
        }
        TypeProbe::Unsupported
    }

    pub(super) fn probe_call_ty(
        &self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::ChainMember(base, member) = root {
            return self.probe_chain_ty(base, member, Some(&groups), expected, context);
        }
        if let Expr::Member(base, variant) = root {
            if let Expr::Name(target) = base.as_ref() {
                if !context.shadows_top_level_name(target)
                    && (self.collection.struct_templates.contains_key(target)
                        || self.collection.enum_templates.contains_key(target))
                {
                    let candidates = self.constructor_trait_associated_function_candidates(
                        target,
                        variant,
                        &context.origin,
                    );
                    let canonical = match candidates.as_slice() {
                        [canonical] => Some(canonical.clone()),
                        [_, _, ..]
                            if groups
                                .iter()
                                .flat_map(|group| group.iter())
                                .any(|argument| argument.label.is_some()) =>
                        {
                            let matches = self.matching_function_overloads(&candidates, &groups, 0);
                            match matches.as_slice() {
                                [selected] => Some(selected.clone()),
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(canonical) = canonical {
                        return self.probe_function_candidate_call_ty(&canonical, &groups, context);
                    }
                }
            }
            let Some((NominalKind::Enum, ty, source)) = self.probe_nominal_type_head(base, context)
            else {
                return TypeProbe::Unsupported;
            };
            let Some(fields) = self.probe_enum_variant_fields(&source, variant) else {
                return TypeProbe::Unsupported;
            };
            if !self.source_variant_fields_are_accessible(&source, &fields, &context.origin) {
                return TypeProbe::Unsupported;
            }
            let valid = match fields {
                VariantFields::Unit => false,
                VariantFields::Positional(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| argument.label.is_none())
                }
                VariantFields::Named(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                fields.iter().any(|field| field.name == *label)
                            })
                        })
                }
            };
            return if valid {
                TypeProbe::KnownSource(ty, source)
            } else {
                TypeProbe::Unsupported
            };
        }
        let Expr::Name(name) = root else {
            return TypeProbe::Unsupported;
        };
        if let Some(local) = context.lookup(name) {
            let function = match &local.ty {
                Ty::Function(function) => function,
                Ty::Callable(callable) => &callable.signature,
                _ => return TypeProbe::Unsupported,
            };
            if groups.len() > function.groups.len()
                || groups
                    .iter()
                    .zip(&function.groups)
                    .any(|(arguments, parameters)| {
                        arguments.len() != parameters.len()
                            || arguments.iter().any(|argument| argument.label.is_some())
                    })
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == function.groups.len() {
                if function.failure_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&function.result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known((*function.result).clone());
            }
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: function.groups[groups.len()..].to_vec(),
                unsafety: function.unsafety,
                failure_error: function.failure_error.clone(),
                custom_effects: function.custom_effects.clone(),
                result: function.result.clone(),
            }));
        }
        if self.empty_struct_candidate(name, &groups, context) {
            let constructor = calls::empty_trailing_closure_constructor(expression)
                .expect("empty struct candidate has an empty trailing closure");
            return self.probe_struct_literal_ty(constructor, &[], expected, context);
        }
        if let Some(candidates) = self.collection.function_overloads.get(name) {
            if !groups
                .iter()
                .flat_map(|group| group.iter())
                .any(|argument| argument.label.is_some())
            {
                return TypeProbe::Unsupported;
            }
            let matches = self.matching_function_overloads(candidates, &groups, 0);
            let [selected] = matches.as_slice() else {
                return TypeProbe::Unsupported;
            };
            let Some(signature) = self.lowering.signatures.get(selected) else {
                return TypeProbe::Unsupported;
            };
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.failure_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafety: signature.unsafety,
                failure_error: signature.failure_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }
        if context.shadows_top_level_name(name) {
            return TypeProbe::Unsupported;
        }
        if let Some(template) = self.collection.struct_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() == compile_group_count + 1
                && self.source_fields_are_accessible(name, &template.fields, &context.origin)
            {
                let value_arguments = groups[compile_group_count];
                let labeled = value_arguments
                    .iter()
                    .filter(|argument| argument.label.is_some())
                    .count();
                let valid_fields = if labeled == 0 {
                    value_arguments.len() == template.fields.len()
                } else if labeled == value_arguments.len() {
                    value_arguments.len() == template.fields.len()
                        && value_arguments.iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                template.fields.iter().any(|field| field.name == *label)
                            })
                        })
                } else {
                    false
                };
                if valid_fields {
                    if let Some((NominalKind::Struct, ty, source)) = self
                        .probe_generic_nominal_type_head(
                            name,
                            &groups[..compile_group_count],
                            context,
                        )
                    {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
            }
        }
        if let Some(template) = self.collection.function_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() >= compile_group_count
                && groups.len() <= compile_group_count + template.groups.len()
            {
                let mut substitutions = HashMap::new();
                let mut valid = true;
                for (parameters, supplied) in template
                    .compile_groups
                    .iter()
                    .zip(groups.iter().take(compile_group_count))
                {
                    let Some(sources) = self.probe_compile_group_sources(
                        parameters,
                        supplied,
                        &context.type_substitutions,
                    ) else {
                        valid = false;
                        break;
                    };
                    for (parameter, source) in parameters.iter().zip(sources) {
                        substitutions.insert(parameter.name.clone(), source);
                    }
                }
                let runtime_groups = &groups[compile_group_count..];
                valid &= runtime_groups
                    .iter()
                    .zip(&template.groups)
                    .all(|(arguments, parameters)| arguments.len() == parameters.len());
                if valid {
                    let Some(mut result_source) = template.return_type.clone() else {
                        return TypeProbe::Unsupported;
                    };
                    substitute_type_parameters(&mut result_source, &substitutions);
                    let Some(result) = self.probe_source_ty(&result_source) else {
                        return TypeProbe::Unsupported;
                    };
                    if runtime_groups.len() == template.groups.len() {
                        if template.effects.failure.is_some() {
                            let Some(info) = self.standard_fallible_info_for_ty(&result) else {
                                return TypeProbe::Unsupported;
                            };
                            return TypeProbe::Known(info.payload);
                        }
                        return TypeProbe::KnownSource(result, result_source);
                    }
                    let remaining = template.groups[runtime_groups.len()..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| {
                                    let mut source = parameter.ty.clone();
                                    substitute_type_parameters(&mut source, &substitutions);
                                    self.probe_source_ty(&source)
                                })
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>();
                    if let Some(groups) = remaining {
                        let failure_error = match template.effects.failure.as_deref() {
                            Some(error) => {
                                let Some(error) = self.probe_source_ty(error) else {
                                    return TypeProbe::Unsupported;
                                };
                                Some(Box::new(error))
                            }
                            None => None,
                        };
                        return TypeProbe::Known(Ty::Function(FunctionTy {
                            groups,
                            unsafety: self.function_effects_unsafe(&template.effects),
                            failure_error,
                            custom_effects: self
                                .function_effects_custom_identities(&template.effects),
                            result: Box::new(result),
                        }));
                    }
                }
            }
        }
        if let Some(signature) = self.lowering.signatures.get(name) {
            if groups.len() > signature.groups.len()
                || groups
                    .iter()
                    .zip(&signature.groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == signature.groups.len() {
                let Some(result) = signature.result.clone() else {
                    return TypeProbe::Unsupported;
                };
                if signature.failure_error.is_some() {
                    return self
                        .standard_fallible_info_for_ty(&result)
                        .map_or(TypeProbe::Unsupported, |info| {
                            TypeProbe::Known(info.payload)
                        });
                }
                return TypeProbe::Known(result);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                unsafety: signature.unsafety,
                failure_error: signature.failure_error.clone().map(Box::new),
                custom_effects: signature.custom_effects.clone(),
                result: Box::new(result),
            }));
        }
        if self
            .collection
            .struct_layouts
            .get(name)
            .is_some_and(|layout| {
                layout
                    .fields
                    .iter()
                    .all(|field| Self::access_boundary_allows(&context.origin, &field.access))
            })
            && groups.len() == 1
        {
            return TypeProbe::Known(Ty::Struct(name.clone()));
        }
        TypeProbe::Unsupported
    }
}
