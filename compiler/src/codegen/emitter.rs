use super::*;
use crate::cleanup::{CleanupPlan, LocalOwnership as CleanupLocalOwnership};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConstValue {
    Integer(i128),
    Bool(bool),
    Unit,
    Aggregate(Vec<ConstValue>),
    LayoutQuery(Ty, LayoutQueryKind),
}

pub(super) fn evaluate_globals(
    program: &HirProgram,
) -> Result<HashMap<String, ConstValue>, Vec<Diagnostic>> {
    let globals: HashMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global))
        .collect();
    let mut evaluator = ConstantEvaluator {
        program,
        globals,
        values: HashMap::new(),
        active: HashSet::new(),
        diagnostics: Vec::new(),
    };
    for global in &program.globals {
        evaluator.evaluate_global(&global.name);
    }
    if evaluator.diagnostics.is_empty() {
        Ok(evaluator.values)
    } else {
        Err(evaluator.diagnostics)
    }
}

struct ConstantEvaluator<'a> {
    program: &'a HirProgram,
    globals: HashMap<String, &'a HirGlobal>,
    values: HashMap<String, ConstValue>,
    active: HashSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl ConstantEvaluator<'_> {
    fn evaluate_global(&mut self, name: &str) -> Option<ConstValue> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }
        if !self.active.insert(name.to_owned()) {
            self.error(format!("cyclic global constant involving `{name}`"));
            return None;
        }
        let global = *self.globals.get(name)?;
        let mut locals = HashMap::new();
        let value = self.evaluate_expr(&global.value, &mut locals);
        self.active.remove(name);
        if let Some(value) = &value {
            self.values.insert(name.to_owned(), value.clone());
        }
        value
    }

    fn evaluate_expr(
        &mut self,
        expression: &HirExpr,
        locals: &mut HashMap<LocalId, ConstValue>,
    ) -> Option<ConstValue> {
        match &expression.kind {
            HirExprKind::Integer(value) => Some(ConstValue::Integer(*value)),
            HirExprKind::Bool(value) => Some(ConstValue::Bool(*value)),
            HirExprKind::Unit => Some(ConstValue::Unit),
            HirExprKind::LayoutQuery { queried, kind } => {
                Some(ConstValue::LayoutQuery(queried.clone(), *kind))
            }
            HirExprKind::Array(elements) => Some(ConstValue::Aggregate(
                elements
                    .iter()
                    .map(|element| self.evaluate_expr(element, locals))
                    .collect::<Option<Vec<_>>>()?,
            )),
            HirExprKind::Index { base, index, .. } => {
                let ConstValue::Aggregate(elements) = self.evaluate_expr(base, locals)? else {
                    self.error("invalid array value in constant expression");
                    return None;
                };
                let index = match index {
                    HirIndex::Static(index) => i128::from(*index),
                    HirIndex::Dynamic(index) => {
                        let ConstValue::Integer(index) = self.evaluate_expr(index, locals)? else {
                            self.error("invalid array index in constant expression");
                            return None;
                        };
                        index
                    }
                };
                let Ok(index) = usize::try_from(index) else {
                    self.error("array index is out of bounds in constant expression");
                    return None;
                };
                elements.get(index).cloned().or_else(|| {
                    self.error("array index is out of bounds in constant expression");
                    None
                })
            }
            HirExprKind::Read { place, .. } => {
                let mut value = locals.get(&place.local).cloned().or_else(|| {
                    self.error("invalid local in constant expression");
                    None
                })?;
                for index in &place.projections {
                    let ConstValue::Aggregate(fields) = value else {
                        self.error("invalid field read in constant expression");
                        return None;
                    };
                    let Some(field) = fields.get(*index).cloned() else {
                        self.error("invalid field index in constant expression");
                        return None;
                    };
                    value = field;
                }
                Some(value)
            }
            HirExprKind::Global(name) => self.evaluate_global(name),
            HirExprKind::ConstructStruct { name, fields } => {
                let layout = self.program.struct_layout(name)?;
                let mut values = layout
                    .fields
                    .iter()
                    .map(|field| zero_const(&field.ty, self.program))
                    .collect::<Option<Vec<_>>>()?;
                for (index, field) in fields {
                    values[*index] = self.evaluate_expr(field, locals)?;
                }
                Some(ConstValue::Aggregate(values))
            }
            HirExprKind::ConstructEnum {
                name,
                variant,
                fields,
            } => {
                let layout = self.program.enum_layout(name)?;
                let variant_layout = &layout.variants[*variant];
                let mut values = vec![ConstValue::Integer(*variant as i128)];
                values.extend(
                    layout
                        .variants
                        .iter()
                        .flat_map(|variant| &variant.fields)
                        .map(|field| zero_const(&field.ty, self.program))
                        .collect::<Option<Vec<_>>>()?,
                );
                for (index, field) in fields {
                    values[1 + variant_layout.payload_offset + index] =
                        self.evaluate_expr(field, locals)?;
                }
                Some(ConstValue::Aggregate(values))
            }
            HirExprKind::Field { base, index } => {
                let ConstValue::Aggregate(fields) = self.evaluate_expr(base, locals)? else {
                    return None;
                };
                fields.get(*index).cloned()
            }
            HirExprKind::Unary(operator, operand) => {
                let operand = self.evaluate_expr(operand, locals)?;
                self.evaluate_unary(*operator, operand, &expression.ty)
            }
            HirExprKind::Binary(left, BinaryOp::And, right) => {
                let ConstValue::Bool(left) = self.evaluate_expr(left, locals)? else {
                    return None;
                };
                if !left {
                    Some(ConstValue::Bool(false))
                } else {
                    self.evaluate_expr(right, locals)
                }
            }
            HirExprKind::Binary(left, BinaryOp::Or, right) => {
                let ConstValue::Bool(left) = self.evaluate_expr(left, locals)? else {
                    return None;
                };
                if left {
                    Some(ConstValue::Bool(true))
                } else {
                    self.evaluate_expr(right, locals)
                }
            }
            HirExprKind::Binary(left, operator, right) => {
                let left_value = self.evaluate_expr(left, locals)?;
                let right_value = self.evaluate_expr(right, locals)?;
                self.evaluate_binary(left_value, *operator, right_value, &left.ty)
            }
            HirExprKind::Block(statements, tail) => {
                let saved = locals.clone();
                for statement in statements {
                    match statement {
                        HirStmt::Let(binding) => {
                            let value = self.evaluate_expr(&binding.value, locals)?;
                            locals.insert(binding.id, value);
                        }
                        HirStmt::Expr(expression) => {
                            self.evaluate_expr(expression, locals)?;
                        }
                    }
                }
                let result = match tail {
                    Some(tail) => self.evaluate_expr(tail, locals),
                    None => Some(ConstValue::Unit),
                };
                *locals = saved;
                result
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let ConstValue::Bool(condition) = self.evaluate_expr(condition, locals)? else {
                    return None;
                };
                if condition {
                    self.evaluate_expr(then_branch, locals)
                } else if let Some(else_branch) = else_branch {
                    self.evaluate_expr(else_branch, locals)
                } else {
                    Some(ConstValue::Unit)
                }
            }
            HirExprKind::Assign { .. }
            | HirExprKind::Borrow { .. }
            | HirExprKind::RawAddress { .. }
            | HirExprKind::RawOffset { .. }
            | HirExprKind::RawBorrow { .. }
            | HirExprKind::RawLoad(_)
            | HirExprKind::RawStore { .. }
            | HirExprKind::RawInit { .. }
            | HirExprKind::RawTake(_)
            | HirExprKind::Forget(_)
            | HirExprKind::RawTrap
            | HirExprKind::RawAlloc { .. }
            | HirExprKind::RawDealloc { .. }
            | HirExprKind::Call { .. }
            | HirExprKind::TailCall { .. }
            | HirExprKind::TailInvokeContinuation { .. }
            | HirExprKind::EraseContinuation { .. }
            | HirExprKind::InvokeContinuation { .. }
            | HirExprKind::EraseEffectCallable { .. }
            | HirExprKind::InvokeEffectCallable { .. }
            | HirExprKind::IndirectCall { .. }
            | HirExprKind::Partial { .. }
            | HirExprKind::PartialCapture { .. }
            | HirExprKind::LocalClosure(_)
            | HirExprKind::Function(_)
            | HirExprKind::Return(_)
            | HirExprKind::While { .. }
            | HirExprKind::Loop { .. }
            | HirExprKind::Break(_)
            | HirExprKind::Continue
            | HirExprKind::Match { .. } => {
                self.error("global initializer is not a compile-time constant");
                None
            }
        }
    }

    fn evaluate_unary(
        &mut self,
        operator: UnaryOp,
        operand: ConstValue,
        ty: &Ty,
    ) -> Option<ConstValue> {
        if matches!(&operand, ConstValue::LayoutQuery(_, _)) {
            self.error(
                "target layout queries may only be standalone global constants in this version",
            );
            return None;
        }
        match (operator, operand) {
            (UnaryOp::Not, ConstValue::Bool(value)) => Some(ConstValue::Bool(!value)),
            (UnaryOp::Neg, ConstValue::Integer(value)) => value
                .checked_neg()
                .filter(|value| integer_fits(*value, ty))
                .map(ConstValue::Integer)
                .or_else(|| {
                    self.error(format!("constant arithmetic overflows `{ty}`"));
                    None
                }),
            _ => None,
        }
    }

    fn evaluate_binary(
        &mut self,
        left: ConstValue,
        operator: BinaryOp,
        right: ConstValue,
        operand_ty: &Ty,
    ) -> Option<ConstValue> {
        use BinaryOp::*;
        if matches!(&left, ConstValue::LayoutQuery(_, _))
            || matches!(&right, ConstValue::LayoutQuery(_, _))
        {
            self.error(
                "target layout queries may only be standalone global constants in this version",
            );
            return None;
        }
        match (left, right) {
            (ConstValue::Integer(left), ConstValue::Integer(right)) => {
                if matches!(operator, Div | Rem)
                    && right == -1
                    && signed_integer_min(operand_ty) == Some(left)
                {
                    self.error(format!("constant arithmetic overflows `{operand_ty}`"));
                    return None;
                }
                if matches!(operator, Shl | Shr)
                    && u32::try_from(right)
                        .ok()
                        .is_none_or(|shift| shift >= integer_bit_width(operand_ty))
                {
                    self.error(format!(
                        "shift count `{right}` is out of range for `{operand_ty}`"
                    ));
                    return None;
                }
                let arithmetic = match operator {
                    Add => left.checked_add(right),
                    Sub => left.checked_sub(right),
                    Mul => left.checked_mul(right),
                    Div if right == 0 => {
                        self.error("division by zero in global constant");
                        return None;
                    }
                    Div => left.checked_div(right),
                    Rem if right == 0 => {
                        self.error("remainder by zero in global constant");
                        return None;
                    }
                    Rem => left.checked_rem(right),
                    BitAnd => Some(left & right),
                    BitOr => Some(left | right),
                    BitXor => Some(left ^ right),
                    Shl => u32::try_from(right)
                        .ok()
                        .filter(|shift| *shift < integer_bit_width(operand_ty))
                        .and_then(|shift| left.checked_shl(shift)),
                    Shr => u32::try_from(right)
                        .ok()
                        .filter(|shift| *shift < integer_bit_width(operand_ty))
                        .and_then(|shift| left.checked_shr(shift)),
                    Eq => return Some(ConstValue::Bool(left == right)),
                    Ne => return Some(ConstValue::Bool(left != right)),
                    Lt => return Some(ConstValue::Bool(left < right)),
                    Le => return Some(ConstValue::Bool(left <= right)),
                    Gt => return Some(ConstValue::Bool(left > right)),
                    Ge => return Some(ConstValue::Bool(left >= right)),
                    And | Or => unreachable!("short-circuit operators handled separately"),
                };
                arithmetic
                    .filter(|value| integer_fits(*value, operand_ty))
                    .map(ConstValue::Integer)
                    .or_else(|| {
                        self.error(format!("constant arithmetic overflows `{operand_ty}`"));
                        None
                    })
            }
            (ConstValue::Bool(left), ConstValue::Bool(right)) => match operator {
                Eq => Some(ConstValue::Bool(left == right)),
                Ne => Some(ConstValue::Bool(left != right)),
                _ => None,
            },
            _ => None,
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(message));
    }
}

pub(super) struct Emitter<'a> {
    program: &'a HirProgram,
    constants: HashMap<String, ConstValue>,
    cleanup_plans: &'a [CleanupPlan],
}

impl<'a> Emitter<'a> {
    pub(super) fn new(
        program: &'a HirProgram,
        constants: HashMap<String, ConstValue>,
        cleanup_plans: &'a [CleanupPlan],
    ) -> Self {
        Self {
            program,
            constants,
            cleanup_plans,
        }
    }

    pub(super) fn emit_module(&self, include_entry_point: bool) -> Result<String, Diagnostic> {
        let mut output = String::new();
        output.push_str(
            "; ModuleID = 'salicin'\nsource_filename = \"salicin\"\n\n%salicin.continuation = type { ptr, ptr, ptr, ptr }\n%salicin.effect_callable = type { ptr, ptr, ptr, ptr }\n\ndeclare void @llvm.trap()\ndeclare ptr @salicin_alloc(i64, i64)\ndeclare void @salicin_dealloc(ptr, i64, i64)\n\n",
        );

        for layout in &self.program.structs {
            let fields = layout
                .fields
                .iter()
                .map(|field| llvm_field_type(&field.ty))
                .collect::<Result<Vec<_>, _>>()?;
            output.push_str(&format!(
                "%{} = type {{ {} }}\n",
                type_symbol(&layout.name),
                fields.join(", ")
            ));
        }
        for layout in &self.program.enums {
            let mut fields = vec!["i32".to_owned()];
            for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                fields.push(llvm_field_type(&field.ty)?);
            }
            output.push_str(&format!(
                "%{} = type {{ {} }}\n",
                type_symbol(&layout.name),
                fields.join(", ")
            ));
        }
        let callable_types = self.callable_types();
        for callable_ty in &callable_types {
            let Ty::Callable(callable) = callable_ty else {
                unreachable!("callable type collector only returns callables");
            };
            let fields = callable
                .captures
                .iter()
                .map(|capture| {
                    if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow) {
                        Ok("ptr".to_owned())
                    } else {
                        llvm_field_type(&capture.ty)
                    }
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            output.push_str(&format!(
                "%{} = type {{ {} }}\n",
                type_symbol(&canonical_type_encoding(callable_ty)),
                fields.join(", ")
            ));
        }
        if !self.program.structs.is_empty()
            || !self.program.enums.is_empty()
            || !callable_types.is_empty()
        {
            output.push('\n');
        }

        for global in &self.program.globals {
            if global.ty == Ty::Unit {
                continue;
            }
            let llvm_ty = llvm_value_type(&global.ty)?;
            let value = self.constants.get(&global.name).ok_or_else(|| {
                Diagnostic::new(format!("constant `{}` was not evaluated", global.name))
            })?;
            output.push_str(&format!(
                "@{} = internal unnamed_addr constant {} {}\n",
                global_symbol(&global.name),
                llvm_ty,
                const_ir(value, &global.ty, self.program)?
            ));
        }
        if !self.program.globals.is_empty() {
            output.push('\n');
        }

        if self.cleanup_plans.len() != self.program.functions.len() {
            return Err(Diagnostic::new(
                "internal error: cleanup plan count does not match HIR function count",
            ));
        }
        for (function, cleanup_plan) in self.program.functions.iter().zip(self.cleanup_plans) {
            let mut emitter = FunctionEmitter::new(function, self.program, cleanup_plan);
            output.push_str(&emitter.emit()?);
            output.push('\n');
        }

        for adapter in &self.program.continuation_adapters {
            output.push_str(&self.emit_continuation_adapter(adapter)?);
            output.push('\n');
            output.push_str(&self.emit_continuation_drop_adapter(adapter)?);
            output.push('\n');
        }
        for adapter in &self.program.effect_callable_adapters {
            output.push_str(&self.emit_effect_callable_adapter(adapter)?);
            output.push('\n');
            output.push_str(&self.emit_effect_callable_drop_adapter(adapter)?);
            output.push('\n');
        }

        for ty in self.drop_glue_types() {
            output.push_str(&self.emit_drop_glue(&ty)?);
            output.push('\n');
        }

        if !include_entry_point {
            return Ok(output);
        }

        let main = self
            .program
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("entry point checked by analyzer");
        output.push_str("define i32 @main() {\nentry:\n");
        match main.result {
            Ty::Unit => {
                output.push_str(&format!("  call void @{}()\n", function_symbol("main")));
                output.push_str("  ret i32 0\n");
            }
            Ty::I32 => {
                output.push_str(&format!(
                    "  %status = call i32 @{}()\n",
                    function_symbol("main")
                ));
                output.push_str("  ret i32 %status\n");
            }
            _ => unreachable!("entry result checked by analyzer"),
        }
        output.push_str("}\n");
        Ok(output)
    }

    fn callable_types(&self) -> Vec<Ty> {
        fn collect(ty: &Ty, types: &mut HashSet<Ty>) {
            match ty {
                Ty::Array(element, _) => collect(element, types),
                Ty::Function(function) => {
                    for parameter in function.groups.iter().flatten() {
                        collect(parameter, types);
                    }
                    collect(&function.result, types);
                }
                Ty::Callable(callable) => {
                    if !types.insert(ty.clone()) {
                        return;
                    }
                    for capture in &callable.captures {
                        collect(&capture.ty, types);
                    }
                    collect(&Ty::Function(callable.signature.clone()), types);
                }
                Ty::Continuation { input, output } => {
                    collect(input, types);
                    collect(output, types);
                }
                Ty::EffectCallable {
                    input,
                    output,
                    answer,
                } => {
                    collect(input, types);
                    collect(output, types);
                    collect(answer, types);
                }
                Ty::I32
                | Ty::I64
                | Ty::U32
                | Ty::U64
                | Ty::Bool
                | Ty::Unit
                | Ty::Pointer { .. }
                | Ty::Reference { .. }
                | Ty::Struct(_)
                | Ty::Enum(_)
                | Ty::EffectRow { .. }
                | Ty::Never
                | Ty::Error => {}
            }
        }

        let mut types = HashSet::new();
        for global in &self.program.globals {
            collect(&global.ty, &mut types);
        }
        for function in &self.program.functions {
            collect(&function.result, &mut types);
            for parameter in &function.params {
                collect(&parameter.ty, &mut types);
            }
        }
        for layout in &self.program.structs {
            for field in &layout.fields {
                collect(&field.ty, &mut types);
            }
        }
        for adapter in &self.program.continuation_adapters {
            collect(&adapter.callable_ty, &mut types);
        }
        for adapter in &self.program.effect_callable_adapters {
            collect(&adapter.callable_ty, &mut types);
        }
        for layout in &self.program.enums {
            for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                collect(&field.ty, &mut types);
            }
        }
        let mut types = types.into_iter().collect::<Vec<_>>();
        types.sort_by_key(canonical_type_encoding);
        types
    }

    fn drop_glue_types(&self) -> Vec<Ty> {
        let mut types = HashSet::new();
        for global in &self.program.globals {
            self.collect_drop_glue_type(&global.ty, &mut types);
        }
        for function in &self.program.functions {
            self.collect_drop_glue_type(&function.result, &mut types);
            for parameter in &function.params {
                self.collect_drop_glue_type(&parameter.ty, &mut types);
            }
        }
        for ty in &self.program.array_types {
            self.collect_drop_glue_type(ty, &mut types);
        }
        for layout in &self.program.structs {
            let ty = Ty::Struct(layout.name.clone());
            if self.program.needs_drop(&ty) {
                types.insert(ty);
            }
            for field in &layout.fields {
                self.collect_drop_glue_type(&field.ty, &mut types);
            }
        }
        for ty in self.callable_types() {
            self.collect_drop_glue_type(&ty, &mut types);
        }
        for layout in &self.program.enums {
            let ty = Ty::Enum(layout.name.clone());
            if self.program.needs_drop(&ty) {
                types.insert(ty);
            }
            for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                self.collect_drop_glue_type(&field.ty, &mut types);
            }
        }
        let mut types = types.into_iter().collect::<Vec<_>>();
        types.sort_by_key(canonical_type_encoding);
        types
    }

    fn collect_drop_glue_type(&self, ty: &Ty, types: &mut HashSet<Ty>) {
        if !self.program.needs_drop(ty) || !types.insert(ty.clone()) {
            return;
        }
        match ty {
            Ty::Array(element, _) => self.collect_drop_glue_type(element, types),
            Ty::Struct(name) => {
                if let Some(pointee) = self.program.box_pointee(name) {
                    self.collect_drop_glue_type(pointee, types);
                }
                if let Some(layout) = self.program.struct_layout(name) {
                    for field in &layout.fields {
                        self.collect_drop_glue_type(&field.ty, types);
                    }
                }
            }
            Ty::Enum(name) => {
                if let Some(layout) = self.program.enum_layout(name) {
                    for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                        self.collect_drop_glue_type(&field.ty, types);
                    }
                }
            }
            Ty::Callable(callable) => {
                for capture in &callable.captures {
                    self.collect_drop_glue_type(&capture.ty, types);
                }
            }
            Ty::Continuation { .. } | Ty::EffectCallable { .. } => {}
            Ty::I32
            | Ty::I64
            | Ty::U32
            | Ty::U64
            | Ty::Bool
            | Ty::Unit
            | Ty::Pointer { .. }
            | Ty::Reference { .. }
            | Ty::Never
            | Ty::Function(_)
            | Ty::EffectRow { .. }
            | Ty::Error => {}
        }
    }

    fn emit_drop_glue(&self, ty: &Ty) -> Result<String, Diagnostic> {
        let mut output = format!(
            "define internal void @{}(ptr %value) {{\nentry:\n",
            drop_glue_symbol(ty)
        );
        if let Some(method) = self.program.drop_methods.get(ty) {
            output.push_str(&format!(
                "  call void @{}(ptr %value)\n",
                function_symbol(method)
            ));
        }
        if let Ty::Struct(name) = ty {
            if let Some(pointee) = self.program.box_pointee(name) {
                let aggregate_ty = llvm_value_type(ty)?;
                output.push_str(&format!(
                    "  %pointer.addr = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 0\n  %pointer = load ptr, ptr %pointer.addr\n"
                ));
                if self.program.needs_drop(pointee) {
                    output.push_str(&format!(
                        "  call void @{}(ptr %pointer)\n",
                        drop_glue_symbol(pointee)
                    ));
                }
                output.push_str(&format!(
                    "  call void @salicin_dealloc(ptr %pointer, i64 {}, i64 {})\n  ret void\n}}\n",
                    llvm_layout_const(pointee, LayoutQueryKind::Size)?,
                    llvm_layout_const(pointee, LayoutQueryKind::Align)?
                ));
                return Ok(output);
            }
        }
        match ty {
            Ty::Struct(name) => {
                let layout = self.program.struct_layout(name).ok_or_else(|| {
                    Diagnostic::new(format!("internal error: missing struct layout `{name}`"))
                })?;
                let aggregate_ty = llvm_value_type(ty)?;
                for (index, field) in layout.fields.iter().enumerate() {
                    if !self.program.needs_drop(&field.ty) {
                        continue;
                    }
                    output.push_str(&format!(
                        "  %field.{index} = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 {index}\n  call void @{}(ptr %field.{index})\n",
                        drop_glue_symbol(&field.ty)
                    ));
                }
                output.push_str("  ret void\n}\n");
            }
            Ty::Continuation { .. } => {
                output.push_str(
                    "  %drop.addr = getelementptr inbounds %salicin.continuation, ptr %value, i32 0, i32 1\n  %drop = load ptr, ptr %drop.addr\n  %environment.addr = getelementptr inbounds %salicin.continuation, ptr %value, i32 0, i32 2\n  %environment = load ptr, ptr %environment.addr\n  %flag.addr = getelementptr inbounds %salicin.continuation, ptr %value, i32 0, i32 3\n  %flag = load ptr, ptr %flag.addr\n  %active = load i1, ptr %flag\n  br i1 %active, label %drop.active, label %drop.done\ndrop.active:\n  store i1 false, ptr %flag\n  call void %drop(ptr %environment)\n  br label %drop.done\ndrop.done:\n  ret void\n}\n",
                );
            }
            Ty::EffectCallable { .. } => {
                output.push_str(
                    "  %drop.addr = getelementptr inbounds %salicin.effect_callable, ptr %value, i32 0, i32 1\n  %drop = load ptr, ptr %drop.addr\n  %environment.addr = getelementptr inbounds %salicin.effect_callable, ptr %value, i32 0, i32 2\n  %environment = load ptr, ptr %environment.addr\n  %flag.addr = getelementptr inbounds %salicin.effect_callable, ptr %value, i32 0, i32 3\n  %flag = load ptr, ptr %flag.addr\n  %active = load i1, ptr %flag\n  br i1 %active, label %drop.active, label %drop.done\ndrop.active:\n  store i1 false, ptr %flag\n  call void %drop(ptr %environment)\n  br label %drop.done\ndrop.done:\n  ret void\n}\n",
                );
            }
            Ty::Enum(name) => {
                let layout = self.program.enum_layout(name).ok_or_else(|| {
                    Diagnostic::new(format!("internal error: missing enum layout `{name}`"))
                })?;
                let aggregate_ty = llvm_value_type(ty)?;
                output.push_str(&format!(
                    "  %tag.ptr = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 0\n  %tag = load i32, ptr %tag.ptr\n  switch i32 %tag, label %done ["
                ));
                for (index, _) in layout.variants.iter().enumerate() {
                    output.push_str(&format!(" i32 {index}, label %variant.{index}"));
                }
                output.push_str(" ]\n");
                for (variant_index, variant) in layout.variants.iter().enumerate() {
                    output.push_str(&format!("variant.{variant_index}:\n"));
                    for (field_index, field) in variant.fields.iter().enumerate() {
                        if !self.program.needs_drop(&field.ty) {
                            continue;
                        }
                        let llvm_index = 1 + variant.payload_offset + field_index;
                        output.push_str(&format!(
                            "  %variant.{variant_index}.field.{field_index} = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 {llvm_index}\n  call void @{}(ptr %variant.{variant_index}.field.{field_index})\n",
                            drop_glue_symbol(&field.ty)
                        ));
                    }
                    output.push_str("  br label %done\n");
                }
                output.push_str("done:\n  ret void\n}\n");
            }
            Ty::Array(element, length) => {
                let aggregate_ty = llvm_value_type(ty)?;
                for index in 0..*length {
                    output.push_str(&format!(
                        "  %element.{index} = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 {index}\n  call void @{}(ptr %element.{index})\n",
                        drop_glue_symbol(element)
                    ));
                }
                output.push_str("  ret void\n}\n");
            }
            Ty::Callable(callable) => {
                let aggregate_ty = llvm_value_type(ty)?;
                for (index, capture) in callable.captures.iter().enumerate() {
                    if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)
                        || !self.program.needs_drop(&capture.ty)
                    {
                        continue;
                    }
                    output.push_str(&format!(
                        "  %capture.{index} = getelementptr inbounds {aggregate_ty}, ptr %value, i32 0, i32 {index}\n  call void @{}(ptr %capture.{index})\n",
                        drop_glue_symbol(&capture.ty)
                    ));
                }
                output.push_str("  ret void\n}\n");
            }
            _ => {
                return Err(Diagnostic::new(format!(
                    "internal error: cannot emit drop glue for `{ty}`"
                )));
            }
        }
        Ok(output)
    }

    fn emit_continuation_adapter(
        &self,
        adapter: &ContinuationAdapter,
    ) -> Result<String, Diagnostic> {
        let result_ty = llvm_return_type(&adapter.output)?;
        let input_parameter = if adapter.input == Ty::Unit {
            String::new()
        } else {
            format!(", {} %input", llvm_value_type(&adapter.input)?)
        };
        let mut output = format!(
            "define internal {result_ty} @{}(ptr %environment{input_parameter}) {{\nentry:\n",
            function_symbol(&adapter.name)
        );
        let callable_ty = llvm_value_type(&adapter.callable_ty)?;
        output.push_str(&format!(
            "  %callable = load {callable_ty}, ptr %environment\n"
        ));
        let mut arguments = Vec::new();
        for (index, capture) in adapter.captures.iter().enumerate() {
            if capture.ty == Ty::Unit {
                continue;
            }
            let field_ty = if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow) {
                "ptr".to_owned()
            } else {
                llvm_value_type(&capture.ty)?
            };
            output.push_str(&format!(
                "  %capture.{index} = extractvalue {callable_ty} %callable, {index}\n"
            ));
            arguments.push(format!("{field_ty} %capture.{index}"));
        }
        if adapter.input != Ty::Unit {
            arguments.push(format!("{} %input", llvm_value_type(&adapter.input)?));
        }
        let call = format!(
            "call {result_ty} @{}({})",
            function_symbol(&adapter.function),
            arguments.join(", ")
        );
        if adapter.output == Ty::Unit {
            output.push_str(&format!("  {call}\n  ret void\n}}\n"));
        } else {
            output.push_str(&format!(
                "  %result = {call}\n  ret {} %result\n}}\n",
                llvm_value_type(&adapter.output)?
            ));
        }
        Ok(output)
    }

    fn emit_continuation_drop_adapter(
        &self,
        adapter: &ContinuationAdapter,
    ) -> Result<String, Diagnostic> {
        let name = format!("{}$drop", adapter.name);
        let mut output = format!(
            "define internal void @{}(ptr %environment) {{\nentry:\n",
            function_symbol(&name)
        );
        if self.program.needs_drop(&adapter.callable_ty) {
            output.push_str(&format!(
                "  call void @{}(ptr %environment)\n",
                drop_glue_symbol(&adapter.callable_ty)
            ));
        }
        output.push_str("  ret void\n}\n");
        Ok(output)
    }

    fn emit_effect_callable_adapter(
        &self,
        adapter: &EffectCallableAdapter,
    ) -> Result<String, Diagnostic> {
        let result_ty = llvm_return_type(&adapter.answer)?;
        let input_parameter = if adapter.input == Ty::Unit {
            String::new()
        } else {
            format!(", {} %input", llvm_value_type(&adapter.input)?)
        };
        let continuation_ty = Ty::Continuation {
            input: Box::new(adapter.output.clone()),
            output: Box::new(adapter.answer.clone()),
        };
        let continuation_llvm_ty = llvm_value_type(&continuation_ty)?;
        let mut output = format!(
            "define internal {result_ty} @{}(ptr %environment{input_parameter}, {continuation_llvm_ty} %continuation) {{\nentry:\n",
            function_symbol(&adapter.name)
        );
        let callable_ty = llvm_value_type(&adapter.callable_ty)?;
        output.push_str(&format!(
            "  %callable = load {callable_ty}, ptr %environment\n"
        ));
        let mut arguments = Vec::new();
        for (index, capture) in adapter.captures.iter().enumerate() {
            if capture.ty == Ty::Unit {
                continue;
            }
            let field_ty = if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow) {
                "ptr".to_owned()
            } else {
                llvm_value_type(&capture.ty)?
            };
            output.push_str(&format!(
                "  %capture.{index} = extractvalue {callable_ty} %callable, {index}\n"
            ));
            arguments.push(format!("{field_ty} %capture.{index}"));
        }
        if adapter.input != Ty::Unit {
            arguments.push(format!("{} %input", llvm_value_type(&adapter.input)?));
        }
        arguments.push(format!("{continuation_llvm_ty} %continuation"));
        let call = format!(
            "call {result_ty} @{}({})",
            function_symbol(&adapter.function),
            arguments.join(", ")
        );
        if adapter.answer == Ty::Unit {
            output.push_str(&format!("  {call}\n  ret void\n}}\n"));
        } else {
            output.push_str(&format!(
                "  %result = {call}\n  ret {} %result\n}}\n",
                llvm_value_type(&adapter.answer)?
            ));
        }
        Ok(output)
    }

    fn emit_effect_callable_drop_adapter(
        &self,
        adapter: &EffectCallableAdapter,
    ) -> Result<String, Diagnostic> {
        let name = format!("{}$drop", adapter.name);
        let mut output = format!(
            "define internal void @{}(ptr %environment) {{\nentry:\n",
            function_symbol(&name)
        );
        if self.program.needs_drop(&adapter.callable_ty) {
            output.push_str(&format!(
                "  call void @{}(ptr %environment)\n",
                drop_glue_symbol(&adapter.callable_ty)
            ));
        }
        output.push_str("  ret void\n}\n");
        Ok(output)
    }
}

#[derive(Debug, Clone)]
struct Operand {
    ty: Ty,
    value: Option<String>,
}

impl Operand {
    fn unit() -> Self {
        Self {
            ty: Ty::Unit,
            value: None,
        }
    }

    fn never() -> Self {
        Self {
            ty: Ty::Never,
            value: None,
        }
    }

    fn value(&self) -> Result<&str, Diagnostic> {
        self.value.as_deref().ok_or_else(|| {
            Diagnostic::new(format!("internal error: `{}` has no LLVM value", self.ty))
        })
    }
}

#[derive(Clone)]
struct EmitLoopTarget {
    break_label: String,
    continue_label: String,
    result: Option<(Ty, String)>,
    cleanup_depth: usize,
}

#[derive(Clone)]
struct RuntimeDropSlot {
    local: Option<LocalId>,
    ty: Ty,
    pointer: String,
    flag: String,
    projections: Vec<usize>,
    children: Vec<RuntimeDropSlot>,
}

#[derive(Clone)]
struct StoredCapture {
    ty: Ty,
    pointer: String,
    drop_flag: Option<String>,
}

struct FunctionEmitter<'a> {
    function: &'a HirFunction,
    program: &'a HirProgram,
    cleanup_plan: &'a CleanupPlan,
    output: String,
    next_register: usize,
    next_label: usize,
    locals: HashMap<LocalId, String>,
    partial_captures: HashMap<LocalId, Vec<Option<StoredCapture>>>,
    entry_allocas: String,
    loops: Vec<EmitLoopTarget>,
    drop_slots: Vec<RuntimeDropSlot>,
    current_label: String,
    terminated: bool,
}

impl<'a> FunctionEmitter<'a> {
    fn new(
        function: &'a HirFunction,
        program: &'a HirProgram,
        cleanup_plan: &'a CleanupPlan,
    ) -> Self {
        Self {
            function,
            program,
            cleanup_plan,
            output: String::new(),
            next_register: 0,
            next_label: 0,
            locals: HashMap::new(),
            partial_captures: HashMap::new(),
            entry_allocas: String::new(),
            loops: Vec::new(),
            drop_slots: Vec::new(),
            current_label: "entry".to_owned(),
            terminated: false,
        }
    }

    fn emit(&mut self) -> Result<String, Diagnostic> {
        let result = llvm_return_type(&self.function.result)?;
        self.output.push_str(&format!(
            "define internal {result} @{}(",
            function_symbol(&self.function.name)
        ));
        let mut emitted_parameter_count = 0;
        for (index, parameter) in self.function.params.iter().enumerate() {
            if parameter.ty == Ty::Unit {
                continue;
            }
            if emitted_parameter_count != 0 {
                self.output.push_str(", ");
            }
            let abi_ty = if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                "ptr".to_owned()
            } else {
                llvm_value_type(&parameter.ty)?
            };
            self.output.push_str(&format!("{abi_ty} %arg.{index}"));
            emitted_parameter_count += 1;
        }
        self.output.push_str(") {\nentry:\n");
        let entry_alloca_offset = self.output.len();

        if self
            .function
            .params
            .iter()
            .any(|parameter| self.program.is_uninhabited(&parameter.ty))
        {
            self.terminate("unreachable");
        }

        for (index, parameter) in self.function.params.iter().enumerate() {
            if self.terminated {
                break;
            }
            if parameter.ty == Ty::Unit {
                continue;
            }
            if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                self.locals.insert(parameter.id, format!("%arg.{index}"));
                continue;
            }
            let ty = llvm_value_type(&parameter.ty)?;
            let pointer = self.entry_alloca(&ty, &llvm_comment(&parameter.name));
            self.instruction(format!("store {ty} %arg.{index}, ptr {pointer}"));
            self.locals.insert(parameter.id, pointer);
            if self.source_local_needs_drop(parameter.id)? {
                self.register_drop_slot(
                    Some(parameter.id),
                    parameter.ty.clone(),
                    self.locals[&parameter.id].clone(),
                )?;
            }
        }

        let body = self.emit_expr(&self.function.body)?;
        if !self.terminated {
            self.emit_cleanup_range(0)?;
            match self.function.result {
                Ty::Unit => self.terminate("ret void"),
                _ => {
                    let ty = llvm_value_type(&self.function.result)?;
                    self.terminate(format!("ret {ty} {}", body.value()?));
                }
            }
        }
        self.output
            .insert_str(entry_alloca_offset, &self.entry_allocas);
        self.output.push_str("}\n");
        Ok(std::mem::take(&mut self.output))
    }

    fn register_drop_slot(
        &mut self,
        local: Option<LocalId>,
        ty: Ty,
        pointer: String,
    ) -> Result<(), Diagnostic> {
        let slot = self.build_drop_slot(local, ty, pointer, Vec::new())?;
        self.drop_slots.push(slot);
        Ok(())
    }

    fn build_drop_slot(
        &mut self,
        local: Option<LocalId>,
        ty: Ty,
        pointer: String,
        projections: Vec<usize>,
    ) -> Result<RuntimeDropSlot, Diagnostic> {
        let flag = self.entry_alloca("i1", "drop flag");
        self.instruction(format!("store i1 true, ptr {flag}"));
        let mut children = Vec::new();
        if !self.program.drop_methods.contains_key(&ty) {
            if let Ty::Array(element, length) = &ty {
                let aggregate_ty = llvm_value_type(&ty)?;
                for index in 0..*length {
                    let projection = usize::try_from(index).map_err(|_| {
                        Diagnostic::new(format!(
                            "internal error: array drop projection {index} does not fit this target"
                        ))
                    })?;
                    let element_pointer = self.fresh_register();
                    self.instruction(format!(
                        "{element_pointer} = getelementptr inbounds {aggregate_ty}, ptr {pointer}, i32 0, i64 {index}"
                    ));
                    let mut element_projections = projections.clone();
                    element_projections.push(projection);
                    children.push(self.build_drop_slot(
                        local,
                        element.as_ref().clone(),
                        element_pointer,
                        element_projections,
                    )?);
                }
            }
            if let Ty::Struct(name) = &ty {
                let fields = self
                    .program
                    .struct_layout(name)
                    .ok_or_else(|| {
                        Diagnostic::new(format!("internal error: missing struct layout `{name}`"))
                    })?
                    .fields
                    .clone();
                let aggregate_ty = llvm_value_type(&ty)?;
                for (index, field) in fields.iter().enumerate() {
                    if !self.program.needs_drop(&field.ty) {
                        continue;
                    }
                    let field_pointer = self.fresh_register();
                    self.instruction(format!(
                        "{field_pointer} = getelementptr inbounds {aggregate_ty}, ptr {pointer}, i32 0, i32 {index}"
                    ));
                    let mut field_projections = projections.clone();
                    field_projections.push(index);
                    children.push(self.build_drop_slot(
                        local,
                        field.ty.clone(),
                        field_pointer,
                        field_projections,
                    )?);
                }
            }
            if let Ty::Callable(callable) = &ty {
                let aggregate_ty = llvm_value_type(&ty)?;
                for (index, capture) in callable.captures.iter().enumerate() {
                    if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)
                        || !self.program.needs_drop(&capture.ty)
                    {
                        continue;
                    }
                    let capture_pointer = self.fresh_register();
                    self.instruction(format!(
                        "{capture_pointer} = getelementptr inbounds {aggregate_ty}, ptr {pointer}, i32 0, i32 {index}"
                    ));
                    let mut capture_projections = projections.clone();
                    capture_projections.push(index);
                    children.push(self.build_drop_slot(
                        local,
                        capture.ty.clone(),
                        capture_pointer,
                        capture_projections,
                    )?);
                }
            }
        }
        Ok(RuntimeDropSlot {
            local,
            ty,
            pointer,
            flag,
            projections,
            children,
        })
    }

    fn source_local_needs_drop(&self, source_local: LocalId) -> Result<bool, Diagnostic> {
        let local = self
            .cleanup_plan
            .locals
            .iter()
            .find(|local| {
                local.source_local == Some(source_local)
                    && local.ownership == CleanupLocalOwnership::Owned
            })
            .ok_or_else(|| {
                Diagnostic::new(format!(
                    "internal error: HIR local {source_local} has no owned cleanup local"
                ))
            })?;
        let root = self
            .cleanup_plan
            .move_paths
            .iter()
            .find(|path| path.place.local == local.id && path.parent.is_none())
            .ok_or_else(|| {
                Diagnostic::new(format!(
                    "internal error: cleanup local {:?} has no root move path",
                    local.id
                ))
            })?;
        Ok(root.needs_drop)
    }

    fn update_place_drop_flags(
        &mut self,
        local: LocalId,
        projections: &[usize],
        initialized: bool,
        root_initialized: bool,
    ) {
        if let Some(slot) = self
            .drop_slots
            .iter()
            .rev()
            .find(|slot| slot.local == Some(local))
            .cloned()
        {
            let mut updates = Vec::new();
            Self::collect_place_flag_updates(
                &slot,
                projections,
                initialized,
                root_initialized,
                &mut updates,
            );
            for (flag, value) in updates {
                self.instruction(format!("store i1 {value}, ptr {flag}"));
            }
        }
    }

    fn deactivate_drop_slot_at(&mut self, pointer: &str) {
        let Some(slot) = self
            .drop_slots
            .iter()
            .rev()
            .find(|slot| slot.pointer == pointer)
            .cloned()
        else {
            return;
        };
        let mut updates = Vec::new();
        Self::collect_place_flag_updates(&slot, &[], false, false, &mut updates);
        for (flag, value) in updates {
            self.instruction(format!("store i1 {value}, ptr {flag}"));
        }
    }

    fn collect_place_flag_updates(
        slot: &RuntimeDropSlot,
        projections: &[usize],
        initialized: bool,
        root_initialized: bool,
        updates: &mut Vec<(String, bool)>,
    ) {
        let slot_is_ancestor = projections.starts_with(&slot.projections);
        let slot_is_descendant = slot.projections.starts_with(projections);
        let update = if root_initialized {
            Some(true)
        } else if initialized {
            slot_is_descendant.then_some(true)
        } else {
            (slot_is_ancestor || slot_is_descendant).then_some(false)
        };
        if let Some(value) = update {
            updates.push((slot.flag.clone(), value));
        }
        for child in &slot.children {
            Self::collect_place_flag_updates(
                child,
                projections,
                initialized,
                root_initialized,
                updates,
            );
        }
    }

    fn find_drop_slot(slot: &RuntimeDropSlot, projections: &[usize]) -> Option<RuntimeDropSlot> {
        if slot.projections == projections {
            return Some(slot.clone());
        }
        slot.children
            .iter()
            .find_map(|child| Self::find_drop_slot(child, projections))
    }

    fn emit_cleanup_range(&mut self, start: usize) -> Result<(), Diagnostic> {
        let slots = self.drop_slots[start..].to_vec();
        for slot in slots.into_iter().rev() {
            self.emit_conditional_drop(&slot)?;
        }
        Ok(())
    }

    fn release_drop_slots(&mut self, start: usize) {
        let mut flags = Vec::new();
        for slot in &self.drop_slots[start..] {
            Self::collect_slot_flags(slot, &mut flags);
        }
        for flag in flags {
            self.instruction(format!("store i1 false, ptr {flag}"));
        }
        self.drop_slots.truncate(start);
    }

    fn collect_slot_flags(slot: &RuntimeDropSlot, flags: &mut Vec<String>) {
        flags.push(slot.flag.clone());
        for child in &slot.children {
            Self::collect_slot_flags(child, flags);
        }
    }

    fn hold_operand_for_early_exit(&mut self, operand: &Operand) -> Result<(), Diagnostic> {
        if !self.program.needs_drop(&operand.ty) {
            return Ok(());
        }
        let ty = llvm_value_type(&operand.ty)?;
        let pointer = self.entry_alloca(&ty, "staged owned value");
        self.instruction(format!("store {ty} {}, ptr {pointer}", operand.value()?));
        self.register_drop_slot(None, operand.ty.clone(), pointer)?;
        Ok(())
    }

    fn emit_conditional_drop(&mut self, slot: &RuntimeDropSlot) -> Result<(), Diagnostic> {
        let flag = self.fresh_register();
        self.instruction(format!("{flag} = load i1, ptr {}", slot.flag));
        let drop_label = self.fresh_label("drop.run");
        let fallback_label = self.fresh_label("drop.fields");
        let done_label = self.fresh_label("drop.done");
        self.terminate(format!(
            "br i1 {flag}, label %{drop_label}, label %{fallback_label}"
        ));
        self.start_block(&drop_label);
        self.instruction(format!("store i1 false, ptr {}", slot.flag));
        self.instruction(format!(
            "call void @{}(ptr {})",
            drop_glue_symbol(&slot.ty),
            slot.pointer
        ));
        self.terminate(format!("br label %{done_label}"));
        self.start_block(&fallback_label);
        for child in &slot.children {
            self.emit_conditional_drop(child)?;
        }
        self.terminate(format!("br label %{done_label}"));
        self.start_block(&done_label);
        Ok(())
    }

    fn emit_drop_operand(&mut self, operand: &Operand) -> Result<(), Diagnostic> {
        if !self.program.needs_drop(&operand.ty) {
            return Ok(());
        }
        let ty = llvm_value_type(&operand.ty)?;
        let pointer = self.entry_alloca(&ty, "discarded drop value");
        self.instruction(format!("store {ty} {}, ptr {pointer}", operand.value()?));
        self.instruction(format!(
            "call void @{}(ptr {pointer})",
            drop_glue_symbol(&operand.ty)
        ));
        Ok(())
    }

    fn emit_expr(&mut self, expression: &HirExpr) -> Result<Operand, Diagnostic> {
        if self.terminated {
            return Ok(Operand::never());
        }
        let operand = self.emit_expr_inner(expression)?;
        if !self.terminated && self.program.is_uninhabited(&operand.ty) {
            self.terminate("unreachable");
            Ok(Operand::never())
        } else {
            Ok(operand)
        }
    }

    fn emit_expr_inner(&mut self, expression: &HirExpr) -> Result<Operand, Diagnostic> {
        match &expression.kind {
            HirExprKind::Integer(value) => Ok(Operand {
                ty: expression.ty.clone(),
                value: Some(value.to_string()),
            }),
            HirExprKind::Bool(value) => Ok(Operand {
                ty: Ty::Bool,
                value: Some(if *value { "1" } else { "0" }.to_owned()),
            }),
            HirExprKind::Unit => Ok(Operand::unit()),
            HirExprKind::LayoutQuery { queried, kind } => Ok(Operand {
                ty: Ty::U64,
                value: Some(llvm_layout_const(queried, *kind)?),
            }),
            HirExprKind::Array(elements) => {
                let cleanup_depth = self.drop_slots.len();
                let aggregate_ty = llvm_value_type(&expression.ty)?;
                let mut aggregate = "zeroinitializer".to_owned();
                for (index, element) in elements.iter().enumerate() {
                    let element = self.emit_expr(element)?;
                    if self.terminated {
                        self.drop_slots.truncate(cleanup_depth);
                        return Ok(Operand::never());
                    }
                    self.hold_operand_for_early_exit(&element)?;
                    let register = self.fresh_register();
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {index}",
                        llvm_value_type(&element.ty)?,
                        element.value()?
                    ));
                    aggregate = register;
                }
                self.release_drop_slots(cleanup_depth);
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::Index {
                base,
                index,
                length,
                moves,
            } => self.emit_index(expression, base, index, *length, *moves),
            HirExprKind::Read { place, kind } => {
                if place.projections.is_empty()
                    && matches!(expression.ty, Ty::Callable(_))
                    && self.partial_captures.contains_key(&place.local)
                {
                    return self.emit_stored_callable_read(expression, place.local, *kind);
                }
                if *kind == HirReadKind::Move && self.program.needs_drop(&place.root_ty) {
                    self.update_place_drop_flags(place.local, &place.projections, false, false);
                }
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let pointer = self.emit_place_address(place)?;
                let register = self.fresh_register();
                let ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {ty}, ptr {pointer}"));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::RawBorrow { pointer, .. } => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: pointer.value,
                })
            }
            HirExprKind::RawAddress { place } => Ok(Operand {
                ty: expression.ty.clone(),
                value: Some(self.emit_place_address(place)?),
            }),
            HirExprKind::RawOffset { pointer, index } => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let index = self.emit_expr(index)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let Ty::Pointer { pointee, .. } = &expression.ty else {
                    return Err(Diagnostic::new(
                        "internal error: raw offset result is not a pointer",
                    ));
                };
                if **pointee == Ty::Unit {
                    return Ok(Operand {
                        ty: expression.ty.clone(),
                        value: pointer.value,
                    });
                }
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = getelementptr {}, ptr {}, i64 {}",
                    llvm_value_type(pointee)?,
                    pointer.value()?,
                    index.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::RawLoad(pointer) => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let register = self.fresh_register();
                let ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {ty}, ptr {}", pointer.value()?));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::RawStore { pointer, value } => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if value.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                self.instruction(format!(
                    "store {} {}, ptr {}",
                    llvm_value_type(&value.ty)?,
                    value.value()?,
                    pointer.value()?
                ));
                Ok(Operand::unit())
            }
            HirExprKind::RawInit { pointer, value } => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if value.ty != Ty::Unit {
                    self.instruction(format!(
                        "store {} {}, ptr {}",
                        llvm_value_type(&value.ty)?,
                        value.value()?,
                        pointer.value()?
                    ));
                }
                Ok(Operand::unit())
            }
            HirExprKind::RawTake(pointer) => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let result = self.fresh_register();
                self.instruction(format!(
                    "{result} = load {}, ptr {}",
                    llvm_value_type(&expression.ty)?,
                    pointer.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(result),
                })
            }
            HirExprKind::Forget(value) => {
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let _ = value;
                Ok(Operand::unit())
            }
            HirExprKind::RawTrap => {
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");
                Ok(Operand::never())
            }
            HirExprKind::RawAlloc { size, align } => {
                let size = self.emit_expr(size)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let align = self.emit_expr(align)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = call ptr @salicin_alloc(i64 {}, i64 {})",
                    size.value()?,
                    align.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::RawDealloc {
                pointer,
                size,
                align,
            } => {
                let pointer = self.emit_expr(pointer)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let size = self.emit_expr(size)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let align = self.emit_expr(align)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                self.instruction(format!(
                    "call void @salicin_dealloc(ptr {}, i64 {}, i64 {})",
                    pointer.value()?,
                    size.value()?,
                    align.value()?
                ));
                Ok(Operand::unit())
            }
            HirExprKind::Global(name) => {
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let register = self.fresh_register();
                let ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!(
                    "{register} = load {ty}, ptr @{}",
                    global_symbol(name)
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Function(name) => Ok(Operand {
                ty: expression.ty.clone(),
                value: Some(format!("@{}", function_symbol(name))),
            }),
            HirExprKind::ConstructStruct { name, fields } => {
                let cleanup_depth = self.drop_slots.len();
                let aggregate_ty = llvm_value_type(&Ty::Struct(name.clone()))?;
                let mut aggregate = "zeroinitializer".to_owned();
                for (index, field) in fields {
                    let field = self.emit_expr(field)?;
                    if self.terminated {
                        self.drop_slots.truncate(cleanup_depth);
                        return Ok(Operand::never());
                    }
                    self.hold_operand_for_early_exit(&field)?;
                    if field.ty == Ty::Unit {
                        continue;
                    }
                    let register = self.fresh_register();
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {index}",
                        llvm_value_type(&field.ty)?,
                        field.value()?
                    ));
                    aggregate = register;
                }
                self.release_drop_slots(cleanup_depth);
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::ConstructEnum {
                name,
                variant,
                fields,
            } => {
                let cleanup_depth = self.drop_slots.len();
                let layout = self.program.enum_layout(name).ok_or_else(|| {
                    Diagnostic::new(format!("internal error: missing enum layout `{name}`"))
                })?;
                let variant_layout = &layout.variants[*variant];
                let aggregate_ty = llvm_value_type(&Ty::Enum(name.clone()))?;
                let tag_register = self.fresh_register();
                self.instruction(format!(
                    "{tag_register} = insertvalue {aggregate_ty} zeroinitializer, i32 {variant}, 0"
                ));
                let mut aggregate = tag_register;
                for (index, field) in fields {
                    let field = self.emit_expr(field)?;
                    if self.terminated {
                        self.drop_slots.truncate(cleanup_depth);
                        return Ok(Operand::never());
                    }
                    self.hold_operand_for_early_exit(&field)?;
                    if field.ty == Ty::Unit {
                        continue;
                    }
                    let register = self.fresh_register();
                    let payload_index = 1 + variant_layout.payload_offset + index;
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {payload_index}",
                        llvm_value_type(&field.ty)?,
                        field.value()?
                    ));
                    aggregate = register;
                }
                self.release_drop_slots(cleanup_depth);
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::Field { base, index } => {
                let base = self.emit_expr(base)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = extractvalue {} {}, {index}",
                    llvm_value_type(&base.ty)?,
                    base.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Unary(operator, operand) => {
                let operand = self.emit_expr(operand)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let register = self.fresh_register();
                match operator {
                    UnaryOp::Neg => {
                        let ty = llvm_value_type(&operand.ty)?;
                        self.instruction(format!("{register} = sub {ty} 0, {}", operand.value()?));
                    }
                    UnaryOp::Not => {
                        self.instruction(format!("{register} = xor i1 {}, true", operand.value()?))
                    }
                    UnaryOp::Deref => {
                        return Err(Diagnostic::new(
                            "internal error: raw dereference reached generic unary emission",
                        ));
                    }
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Binary(left, BinaryOp::And, right) => {
                self.emit_short_circuit(left, right, false)
            }
            HirExprKind::Binary(left, BinaryOp::Or, right) => {
                self.emit_short_circuit(left, right, true)
            }
            HirExprKind::Binary(left, operator, right) => {
                let left = self.emit_expr(left)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let right = self.emit_expr(right)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if matches!(operator, BinaryOp::Div | BinaryOp::Rem) {
                    self.emit_integer_division_guard(&left, &right)?;
                }
                if matches!(operator, BinaryOp::Shl | BinaryOp::Shr) {
                    self.emit_shift_guard(&right)?;
                }
                let register = self.fresh_register();
                let ty = llvm_value_type(&left.ty)?;
                let instruction = match operator {
                    BinaryOp::Add => "add",
                    BinaryOp::Sub => "sub",
                    BinaryOp::Mul => "mul",
                    BinaryOp::Div if left.ty.is_signed() => "sdiv",
                    BinaryOp::Div => "udiv",
                    BinaryOp::Rem if left.ty.is_signed() => "srem",
                    BinaryOp::Rem => "urem",
                    BinaryOp::BitAnd => "and",
                    BinaryOp::BitOr => "or",
                    BinaryOp::BitXor => "xor",
                    BinaryOp::Shl => "shl",
                    BinaryOp::Shr if left.ty.is_signed() => "ashr",
                    BinaryOp::Shr => "lshr",
                    BinaryOp::Eq => "icmp eq",
                    BinaryOp::Ne => "icmp ne",
                    BinaryOp::Lt if left.ty.is_signed() => "icmp slt",
                    BinaryOp::Lt => "icmp ult",
                    BinaryOp::Le if left.ty.is_signed() => "icmp sle",
                    BinaryOp::Le => "icmp ule",
                    BinaryOp::Gt if left.ty.is_signed() => "icmp sgt",
                    BinaryOp::Gt => "icmp ugt",
                    BinaryOp::Ge if left.ty.is_signed() => "icmp sge",
                    BinaryOp::Ge => "icmp uge",
                    BinaryOp::And | BinaryOp::Or => unreachable!(),
                };
                self.instruction(format!(
                    "{register} = {instruction} {ty} {}, {}",
                    left.value()?,
                    right.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Assign {
                place,
                value,
                assignment,
                root_initialized,
            } => {
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if value.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let ty = llvm_value_type(&value.ty)?;
                let pointer = self.emit_place_address(place)?;
                if self.program.needs_drop(&place.ty)
                    && matches!(
                        assignment,
                        AssignmentKind::Overwrite | AssignmentKind::MaybeOverwrite
                    )
                {
                    let root = self
                        .drop_slots
                        .iter()
                        .rev()
                        .find(|slot| slot.local == Some(place.local))
                        .cloned();
                    if let Some(root) = root {
                        let slot =
                            Self::find_drop_slot(&root, &place.projections).ok_or_else(|| {
                                Diagnostic::new(format!(
                                    "internal error: missing projection drop slot for local {}",
                                    place.local
                                ))
                            })?;
                        self.emit_conditional_drop(&slot)?;
                    } else if place.capability == LocalCapability::MutParam {
                        if *assignment != AssignmentKind::Overwrite {
                            return Err(Diagnostic::new(format!(
                                "internal error: mutable borrow assignment to `{}` was not a definite overwrite",
                                place.ty
                            )));
                        }
                        self.instruction(format!(
                            "call void @{}(ptr {pointer})",
                            drop_glue_symbol(&place.ty)
                        ));
                    } else {
                        return Err(Diagnostic::new(format!(
                            "internal error: missing drop slot for owned local {}",
                            place.local
                        )));
                    }
                }
                self.instruction(format!("store {ty} {}, ptr {pointer}", value.value()?));
                if self.program.needs_drop(&place.root_ty) {
                    self.update_place_drop_flags(
                        place.local,
                        &place.projections,
                        true,
                        *root_initialized,
                    );
                }
                Ok(Operand::unit())
            }
            HirExprKind::Call {
                function,
                arguments,
                ..
            } => {
                let cleanup_depth = self.drop_slots.len();
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    match argument {
                        HirArgument::Copy(argument) | HirArgument::Move(argument) => {
                            let argument = self.emit_expr(argument)?;
                            if self.terminated {
                                self.drop_slots.truncate(cleanup_depth);
                                return Ok(Operand::never());
                            }
                            if argument.ty == Ty::Unit {
                                continue;
                            }
                            emitted_arguments.push(format!(
                                "{} {}",
                                llvm_value_type(&argument.ty)?,
                                argument.value()?
                            ));
                            self.hold_operand_for_early_exit(&argument)?;
                        }
                        HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                            if place.ty == Ty::Unit {
                                continue;
                            }
                            let pointer = self.emit_borrow_address(place)?;
                            emitted_arguments.push(format!("ptr {pointer}"));
                        }
                        HirArgument::CallableCaptureBorrow {
                            binding,
                            index,
                            callable_ty,
                            capture_ty,
                            ..
                        } => {
                            if *capture_ty != Ty::Unit {
                                let pointer = self.emit_callable_capture_borrow(
                                    *binding,
                                    *index,
                                    callable_ty,
                                )?;
                                emitted_arguments.push(format!("ptr {pointer}"));
                            }
                        }
                    }
                }
                self.release_drop_slots(cleanup_depth);
                let call = format!(
                    "call {} @{}({})",
                    llvm_return_type(&expression.ty)?,
                    function_symbol(function),
                    emitted_arguments.join(", ")
                );
                if expression.ty == Ty::Unit {
                    self.instruction(call);
                    Ok(Operand::unit())
                } else {
                    let register = self.fresh_register();
                    self.instruction(format!("{register} = {call}"));
                    if self.program.is_uninhabited(&expression.ty) {
                        self.terminate("unreachable");
                        return Ok(Operand::never());
                    }
                    Ok(Operand {
                        ty: expression.ty.clone(),
                        value: Some(register),
                    })
                }
            }
            HirExprKind::TailCall {
                function,
                arguments,
                result,
                ..
            } => {
                let cleanup_depth = self.drop_slots.len();
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    match argument {
                        HirArgument::Copy(argument) | HirArgument::Move(argument) => {
                            let argument = self.emit_expr(argument)?;
                            if self.terminated {
                                self.drop_slots.truncate(cleanup_depth);
                                return Ok(Operand::never());
                            }
                            if argument.ty == Ty::Unit {
                                continue;
                            }
                            emitted_arguments.push(format!(
                                "{} {}",
                                llvm_value_type(&argument.ty)?,
                                argument.value()?
                            ));
                            self.hold_operand_for_early_exit(&argument)?;
                        }
                        HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                            if place.ty != Ty::Unit {
                                emitted_arguments
                                    .push(format!("ptr {}", self.emit_borrow_address(place)?));
                            }
                        }
                        HirArgument::CallableCaptureBorrow {
                            binding,
                            index,
                            callable_ty,
                            capture_ty,
                            ..
                        } => {
                            if *capture_ty != Ty::Unit {
                                let pointer = self.emit_callable_capture_borrow(
                                    *binding,
                                    *index,
                                    callable_ty,
                                )?;
                                emitted_arguments.push(format!("ptr {pointer}"));
                            }
                        }
                    }
                }
                self.release_drop_slots(cleanup_depth);
                self.emit_cleanup_range(0)?;
                let call = format!(
                    "call {} @{}({})",
                    llvm_return_type(result)?,
                    function_symbol(function),
                    emitted_arguments.join(", ")
                );
                if *result == Ty::Unit {
                    self.instruction(call);
                    self.terminate("ret void");
                } else {
                    let register = self.fresh_register();
                    self.instruction(format!("{register} = {call}"));
                    self.terminate(format!("ret {} {register}", llvm_value_type(result)?));
                }
                Ok(Operand::never())
            }
            HirExprKind::TailInvokeContinuation {
                continuation,
                argument,
                result,
            } => {
                let cleanup_depth = self.drop_slots.len();
                let continuation = self.emit_expr(continuation)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&continuation)?;
                let argument = self.emit_expr(argument)?;
                if self.terminated {
                    self.drop_slots.truncate(cleanup_depth);
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&argument)?;
                let continuation_ty = llvm_value_type(&continuation.ty)?;
                let entry = self.fresh_register();
                self.instruction(format!(
                    "{entry} = extractvalue {continuation_ty} {}, 0",
                    continuation.value()?
                ));
                let environment = self.fresh_register();
                self.instruction(format!(
                    "{environment} = extractvalue {continuation_ty} {}, 2",
                    continuation.value()?
                ));
                let flag = self.fresh_register();
                self.instruction(format!(
                    "{flag} = extractvalue {continuation_ty} {}, 3",
                    continuation.value()?
                ));
                let active = self.fresh_register();
                self.instruction(format!("{active} = load i1, ptr {flag}"));
                let invoke_label = self.fresh_label("tail.continuation.invoke");
                let used_label = self.fresh_label("tail.continuation.used");
                self.terminate(format!(
                    "br i1 {active}, label %{invoke_label}, label %{used_label}"
                ));
                self.start_block(&used_label);
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");
                self.start_block(&invoke_label);
                self.instruction(format!("store i1 false, ptr {flag}"));
                self.release_drop_slots(cleanup_depth);
                self.emit_cleanup_range(0)?;
                let mut arguments = vec![format!("ptr {environment}")];
                if argument.ty != Ty::Unit {
                    arguments.push(format!(
                        "{} {}",
                        llvm_value_type(&argument.ty)?,
                        argument.value()?
                    ));
                }
                let call = format!(
                    "call {} {entry}({})",
                    llvm_return_type(result)?,
                    arguments.join(", ")
                );
                if *result == Ty::Unit {
                    self.instruction(call);
                    self.terminate("ret void");
                } else {
                    let returned = self.fresh_register();
                    self.instruction(format!("{returned} = {call}"));
                    self.terminate(format!("ret {} {returned}", llvm_value_type(result)?));
                }
                Ok(Operand::never())
            }
            HirExprKind::IndirectCall {
                callee, arguments, ..
            } => {
                let cleanup_depth = self.drop_slots.len();
                let callee = self.emit_expr(callee)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    match argument {
                        HirArgument::Copy(argument) | HirArgument::Move(argument) => {
                            let argument = self.emit_expr(argument)?;
                            if self.terminated {
                                self.drop_slots.truncate(cleanup_depth);
                                return Ok(Operand::never());
                            }
                            if argument.ty == Ty::Unit {
                                continue;
                            }
                            emitted_arguments.push(format!(
                                "{} {}",
                                llvm_value_type(&argument.ty)?,
                                argument.value()?
                            ));
                            self.hold_operand_for_early_exit(&argument)?;
                        }
                        HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                            if place.ty == Ty::Unit {
                                continue;
                            }
                            let pointer = self.emit_borrow_address(place)?;
                            emitted_arguments.push(format!("ptr {pointer}"));
                        }
                        HirArgument::CallableCaptureBorrow {
                            binding,
                            index,
                            callable_ty,
                            capture_ty,
                            ..
                        } => {
                            if *capture_ty != Ty::Unit {
                                let pointer = self.emit_callable_capture_borrow(
                                    *binding,
                                    *index,
                                    callable_ty,
                                )?;
                                emitted_arguments.push(format!("ptr {pointer}"));
                            }
                        }
                    }
                }
                self.release_drop_slots(cleanup_depth);
                let call = format!(
                    "call {} {}({})",
                    llvm_return_type(&expression.ty)?,
                    callee.value()?,
                    emitted_arguments.join(", ")
                );
                if expression.ty == Ty::Unit {
                    self.instruction(call);
                    Ok(Operand::unit())
                } else {
                    let register = self.fresh_register();
                    self.instruction(format!("{register} = {call}"));
                    if self.program.is_uninhabited(&expression.ty) {
                        self.terminate("unreachable");
                        return Ok(Operand::never());
                    }
                    Ok(Operand {
                        ty: expression.ty.clone(),
                        value: Some(register),
                    })
                }
            }
            HirExprKind::EraseContinuation {
                binding,
                callable_ty,
                adapter,
            } => {
                let callable_expression = HirExpr {
                    ty: callable_ty.clone(),
                    kind: HirExprKind::Unit,
                };
                let callable = self.emit_stored_callable_read(
                    &callable_expression,
                    *binding,
                    HirReadKind::Move,
                )?;
                let callable_llvm_ty = llvm_value_type(callable_ty)?;
                let environment =
                    self.entry_alloca(&callable_llvm_ty, "erased continuation environment");
                self.instruction(format!(
                    "store {callable_llvm_ty} {}, ptr {environment}",
                    callable.value()?
                ));
                let flag = self.entry_alloca("i1", "erased continuation active flag");
                self.instruction(format!("store i1 true, ptr {flag}"));
                let continuation_ty = llvm_value_type(&expression.ty)?;
                let call_entry = format!("@{}", function_symbol(adapter));
                let drop_entry = format!("@{}", function_symbol(&format!("{adapter}$drop")));
                let first = self.fresh_register();
                self.instruction(format!(
                    "{first} = insertvalue {continuation_ty} zeroinitializer, ptr {call_entry}, 0"
                ));
                let second = self.fresh_register();
                self.instruction(format!(
                    "{second} = insertvalue {continuation_ty} {first}, ptr {drop_entry}, 1"
                ));
                let third = self.fresh_register();
                self.instruction(format!(
                    "{third} = insertvalue {continuation_ty} {second}, ptr {environment}, 2"
                ));
                let fourth = self.fresh_register();
                self.instruction(format!(
                    "{fourth} = insertvalue {continuation_ty} {third}, ptr {flag}, 3"
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(fourth),
                })
            }
            HirExprKind::InvokeContinuation {
                continuation,
                argument,
            } => {
                let cleanup_depth = self.drop_slots.len();
                let continuation = self.emit_expr(continuation)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&continuation)?;
                let argument = self.emit_expr(argument)?;
                if self.terminated {
                    self.drop_slots.truncate(cleanup_depth);
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&argument)?;
                let continuation_ty = llvm_value_type(&continuation.ty)?;
                let entry = self.fresh_register();
                self.instruction(format!(
                    "{entry} = extractvalue {continuation_ty} {}, 0",
                    continuation.value()?
                ));
                let environment = self.fresh_register();
                self.instruction(format!(
                    "{environment} = extractvalue {continuation_ty} {}, 2",
                    continuation.value()?
                ));
                let flag = self.fresh_register();
                self.instruction(format!(
                    "{flag} = extractvalue {continuation_ty} {}, 3",
                    continuation.value()?
                ));
                let active = self.fresh_register();
                self.instruction(format!("{active} = load i1, ptr {flag}"));
                let invoke_label = self.fresh_label("continuation.invoke");
                let used_label = self.fresh_label("continuation.used");
                self.terminate(format!(
                    "br i1 {active}, label %{invoke_label}, label %{used_label}"
                ));
                self.start_block(&used_label);
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");
                self.start_block(&invoke_label);
                self.instruction(format!("store i1 false, ptr {flag}"));
                self.release_drop_slots(cleanup_depth);
                let mut arguments = vec![format!("ptr {environment}")];
                if argument.ty != Ty::Unit {
                    arguments.push(format!(
                        "{} {}",
                        llvm_value_type(&argument.ty)?,
                        argument.value()?
                    ));
                }
                let call = format!(
                    "call {} {entry}({})",
                    llvm_return_type(&expression.ty)?,
                    arguments.join(", ")
                );
                if expression.ty == Ty::Unit {
                    self.instruction(call);
                    Ok(Operand::unit())
                } else {
                    let result = self.fresh_register();
                    self.instruction(format!("{result} = {call}"));
                    Ok(Operand {
                        ty: expression.ty.clone(),
                        value: Some(result),
                    })
                }
            }
            HirExprKind::EraseEffectCallable {
                binding,
                callable_ty,
                adapter,
            } => {
                let callable_expression = HirExpr {
                    ty: callable_ty.clone(),
                    kind: HirExprKind::Unit,
                };
                let callable = self.emit_stored_callable_read(
                    &callable_expression,
                    *binding,
                    HirReadKind::Move,
                )?;
                let callable_llvm_ty = llvm_value_type(callable_ty)?;
                let environment =
                    self.entry_alloca(&callable_llvm_ty, "erased effect-callable environment");
                self.instruction(format!(
                    "store {callable_llvm_ty} {}, ptr {environment}",
                    callable.value()?
                ));
                let flag = self.entry_alloca("i1", "erased effect-callable active flag");
                self.instruction(format!("store i1 true, ptr {flag}"));
                let action_ty = llvm_value_type(&expression.ty)?;
                let call_entry = format!("@{}", function_symbol(adapter));
                let drop_entry = format!("@{}", function_symbol(&format!("{adapter}$drop")));
                let first = self.fresh_register();
                self.instruction(format!(
                    "{first} = insertvalue {action_ty} zeroinitializer, ptr {call_entry}, 0"
                ));
                let second = self.fresh_register();
                self.instruction(format!(
                    "{second} = insertvalue {action_ty} {first}, ptr {drop_entry}, 1"
                ));
                let third = self.fresh_register();
                self.instruction(format!(
                    "{third} = insertvalue {action_ty} {second}, ptr {environment}, 2"
                ));
                let fourth = self.fresh_register();
                self.instruction(format!(
                    "{fourth} = insertvalue {action_ty} {third}, ptr {flag}, 3"
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(fourth),
                })
            }
            HirExprKind::InvokeEffectCallable {
                action,
                input,
                continuation,
            } => {
                let cleanup_depth = self.drop_slots.len();
                let action = self.emit_expr(action)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&action)?;
                let input = self.emit_expr(input)?;
                if self.terminated {
                    self.drop_slots.truncate(cleanup_depth);
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&input)?;
                let continuation = self.emit_expr(continuation)?;
                if self.terminated {
                    self.drop_slots.truncate(cleanup_depth);
                    return Ok(Operand::never());
                }
                self.hold_operand_for_early_exit(&continuation)?;
                let action_ty = llvm_value_type(&action.ty)?;
                let entry = self.fresh_register();
                self.instruction(format!(
                    "{entry} = extractvalue {action_ty} {}, 0",
                    action.value()?
                ));
                let environment = self.fresh_register();
                self.instruction(format!(
                    "{environment} = extractvalue {action_ty} {}, 2",
                    action.value()?
                ));
                let flag = self.fresh_register();
                self.instruction(format!(
                    "{flag} = extractvalue {action_ty} {}, 3",
                    action.value()?
                ));
                let active = self.fresh_register();
                self.instruction(format!("{active} = load i1, ptr {flag}"));
                let invoke_label = self.fresh_label("effect.callable.invoke");
                let used_label = self.fresh_label("effect.callable.used");
                self.terminate(format!(
                    "br i1 {active}, label %{invoke_label}, label %{used_label}"
                ));
                self.start_block(&used_label);
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");
                self.start_block(&invoke_label);
                self.instruction(format!("store i1 false, ptr {flag}"));
                self.release_drop_slots(cleanup_depth);
                let mut arguments = vec![format!("ptr {environment}")];
                if input.ty != Ty::Unit {
                    arguments.push(format!(
                        "{} {}",
                        llvm_value_type(&input.ty)?,
                        input.value()?
                    ));
                }
                arguments.push(format!(
                    "{} {}",
                    llvm_value_type(&continuation.ty)?,
                    continuation.value()?
                ));
                let call = format!(
                    "call {} {entry}({})",
                    llvm_return_type(&expression.ty)?,
                    arguments.join(", ")
                );
                if expression.ty == Ty::Unit {
                    self.instruction(call);
                    Ok(Operand::unit())
                } else {
                    let result = self.fresh_register();
                    self.instruction(format!("{result} = {call}"));
                    Ok(Operand {
                        ty: expression.ty.clone(),
                        value: Some(result),
                    })
                }
            }
            HirExprKind::Partial { captures, .. } => {
                self.emit_callable_environment(expression, captures)
            }
            HirExprKind::Borrow { place, .. } if matches!(expression.ty, Ty::Reference { .. }) => {
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(self.emit_place_address(place)?),
                })
            }
            HirExprKind::Borrow { .. } => Err(Diagnostic::new(
                "borrow value escaped the local binding that owns its loan",
            )),
            HirExprKind::PartialCapture {
                binding,
                index,
                moves,
                callable_ty,
            } => {
                let stored = self
                    .partial_captures
                    .get(binding)
                    .and_then(|captures| captures.get(*index))
                    .cloned();
                if let Some(stored) = stored {
                    let Some(capture) = stored else {
                        return Ok(Operand::unit());
                    };
                    let register = self.fresh_register();
                    self.instruction(format!(
                        "{register} = load {}, ptr {}",
                        llvm_value_type(&capture.ty)?,
                        capture.pointer
                    ));
                    if *moves && capture.drop_flag.is_some() {
                        self.deactivate_drop_slot_at(&capture.pointer);
                    }
                    return Ok(Operand {
                        ty: capture.ty,
                        value: Some(register),
                    });
                }
                let environment = self.locals.get(binding).cloned().ok_or_else(|| {
                    Diagnostic::new(format!(
                        "internal error: unknown callable environment local {binding}"
                    ))
                })?;
                let environment_ty = llvm_value_type(callable_ty)?;
                let pointer = self.fresh_register();
                self.instruction(format!(
                    "{pointer} = getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}"
                ));
                let register = self.fresh_register();
                let capture_ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {capture_ty}, ptr {pointer}"));
                if *moves && self.program.needs_drop(callable_ty) {
                    self.update_place_drop_flags(*binding, &[*index], false, false);
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::LocalClosure(closure) => Err(Diagnostic::new(format!(
                "local closure `{}` escaped its binding",
                closure.function
            ))),
            HirExprKind::Block(statements, tail) => {
                let cleanup_depth = self.drop_slots.len();
                for statement in statements {
                    if self.terminated {
                        break;
                    }
                    match statement {
                        HirStmt::Let(binding) => {
                            if matches!(binding.value.kind, HirExprKind::Function(_)) {
                                self.partial_captures.insert(binding.id, Vec::new());
                                continue;
                            }
                            if let HirExprKind::Read {
                                place,
                                kind: HirReadKind::Move,
                            } = &binding.value.kind
                            {
                                if matches!(binding.ty, Ty::Function(_) | Ty::Callable(_))
                                    && self.partial_captures.contains_key(&place.local)
                                {
                                    self.relocate_callable_captures(
                                        place.local,
                                        binding.id,
                                        &place.ty,
                                    )?;
                                    continue;
                                }
                            }
                            if let HirExprKind::LocalClosure(closure) = &binding.value.kind {
                                let mut stored = Vec::new();
                                for capture in &closure.captures {
                                    if capture.mode != ClosureCaptureMode::Move {
                                        let address = if matches!(capture.place.ty, Ty::Callable(_))
                                            && self
                                                .partial_captures
                                                .contains_key(&capture.place.local)
                                        {
                                            let callable = self.emit_stored_callable_read(
                                                &HirExpr {
                                                    ty: capture.place.ty.clone(),
                                                    kind: HirExprKind::Unit,
                                                },
                                                capture.place.local,
                                                HirReadKind::Copy,
                                            )?;
                                            let callable_ty = llvm_value_type(&callable.ty)?;
                                            let environment = self.entry_alloca(
                                                &callable_ty,
                                                "borrowed callable environment",
                                            );
                                            self.instruction(format!(
                                                "store {callable_ty} {}, ptr {environment}",
                                                callable.value()?
                                            ));
                                            environment
                                        } else {
                                            self.emit_place_address(&capture.place)?
                                        };
                                        let pointer =
                                            self.entry_alloca("ptr", "borrowed closure capture");
                                        self.instruction(format!(
                                            "store ptr {address}, ptr {pointer}"
                                        ));
                                        stored.push(Some(StoredCapture {
                                            ty: capture.place.ty.clone(),
                                            pointer,
                                            drop_flag: None,
                                        }));
                                        continue;
                                    }
                                    let value = self.emit_expr(
                                        capture.value.as_deref().ok_or_else(|| {
                                            Diagnostic::new(
                                                "internal error: move closure capture has no value",
                                            )
                                        })?,
                                    )?;
                                    if self.terminated {
                                        break;
                                    }
                                    let ty = llvm_value_type(&value.ty)?;
                                    let pointer = self.entry_alloca(&ty, "closure capture");
                                    self.instruction(format!(
                                        "store {ty} {}, ptr {pointer}",
                                        value.value()?
                                    ));
                                    let drop_flag = if self.program.needs_drop(&value.ty) {
                                        self.register_drop_slot(
                                            None,
                                            value.ty.clone(),
                                            pointer.clone(),
                                        )?;
                                        Some(
                                            self.drop_slots
                                                .last()
                                                .expect("registered closure capture slot")
                                                .flag
                                                .clone(),
                                        )
                                    } else {
                                        None
                                    };
                                    stored.push(Some(StoredCapture {
                                        ty: value.ty,
                                        pointer,
                                        drop_flag,
                                    }));
                                }
                                if !self.terminated {
                                    self.partial_captures.insert(binding.id, stored);
                                }
                                continue;
                            }
                            if let HirExprKind::Partial { captures, .. } = &binding.value.kind {
                                let mut stored = Vec::new();
                                for capture in captures {
                                    let capture = match capture {
                                        HirArgument::Copy(capture) | HirArgument::Move(capture) => {
                                            self.emit_expr(capture)?
                                        }
                                        HirArgument::SharedBorrow(_)
                                        | HirArgument::MutBorrow(_) => {
                                            return Err(Diagnostic::new(
                                                "borrowed argument reached partial application emission",
                                            ));
                                        }
                                        HirArgument::CallableCaptureBorrow { .. } => {
                                            return Err(Diagnostic::new(
                                                "forwarded borrowed argument reached partial application emission",
                                            ));
                                        }
                                    };
                                    if self.terminated {
                                        break;
                                    }
                                    if capture.ty == Ty::Unit {
                                        stored.push(None);
                                        continue;
                                    }
                                    let ty = llvm_value_type(&capture.ty)?;
                                    let pointer = self.entry_alloca(&ty, "partial capture");
                                    self.instruction(format!(
                                        "store {ty} {}, ptr {pointer}",
                                        capture.value()?
                                    ));
                                    let drop_flag = if self.program.needs_drop(&capture.ty) {
                                        self.register_drop_slot(
                                            None,
                                            capture.ty.clone(),
                                            pointer.clone(),
                                        )?;
                                        Some(
                                            self.drop_slots
                                                .last()
                                                .expect("registered partial capture slot")
                                                .flag
                                                .clone(),
                                        )
                                    } else {
                                        None
                                    };
                                    stored.push(Some(StoredCapture {
                                        ty: capture.ty,
                                        pointer,
                                        drop_flag,
                                    }));
                                }
                                if !self.terminated {
                                    self.partial_captures.insert(binding.id, stored);
                                }
                                continue;
                            }
                            if matches!(binding.value.kind, HirExprKind::Borrow { .. })
                                && !matches!(binding.ty, Ty::Reference { .. })
                            {
                                continue;
                            }
                            let value = self.emit_expr(&binding.value)?;
                            if self.terminated {
                                break;
                            }
                            if binding.ty == Ty::Unit {
                                continue;
                            }
                            let ty = llvm_value_type(&binding.ty)?;
                            let pointer = self.entry_alloca(&ty, &llvm_comment(&binding.name));
                            self.instruction(format!(
                                "store {ty} {}, ptr {pointer}",
                                value.value()?
                            ));
                            self.locals.insert(binding.id, pointer);
                            if self.program.needs_drop(&binding.ty)
                                && self.source_local_needs_drop(binding.id)?
                            {
                                self.register_drop_slot(
                                    Some(binding.id),
                                    binding.ty.clone(),
                                    self.locals[&binding.id].clone(),
                                )?;
                            }
                        }
                        HirStmt::Expr(expression) => {
                            let value = self.emit_expr(expression)?;
                            if !self.terminated {
                                self.emit_drop_operand(&value)?;
                            }
                        }
                    }
                }
                let result = if self.terminated {
                    Ok(Operand::never())
                } else if let Some(tail) = tail {
                    self.emit_expr(tail)
                } else {
                    Ok(Operand::unit())
                };
                if !self.terminated {
                    self.emit_cleanup_range(cleanup_depth)?;
                }
                self.drop_slots.truncate(cleanup_depth);
                result
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.emit_if(expression, condition, then_branch, else_branch.as_deref()),
            HirExprKind::Return(value) => {
                if let Some(value) = value {
                    let value = self.emit_expr(value)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    self.emit_cleanup_range(0)?;
                    if value.ty == Ty::Unit {
                        self.terminate("ret void");
                    } else {
                        let ty = llvm_value_type(&value.ty)?;
                        self.terminate(format!("ret {ty} {}", value.value()?));
                    }
                } else {
                    self.emit_cleanup_range(0)?;
                    self.terminate("ret void");
                }
                Ok(Operand::never())
            }
            HirExprKind::While { condition, body } => self.emit_while(condition, body),
            HirExprKind::Loop { body } => self.emit_loop(expression, body),
            HirExprKind::Break(value) => self.emit_break(value.as_deref()),
            HirExprKind::Continue => self.emit_continue(),
            HirExprKind::Match { scrutinee, arms } => self.emit_match(expression, scrutinee, arms),
        }
    }

    fn emit_index(
        &mut self,
        expression: &HirExpr,
        base: &HirExpr,
        index: &HirIndex,
        length: u64,
        moves: bool,
    ) -> Result<Operand, Diagnostic> {
        let base = self.emit_expr(base)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let array_ty = llvm_value_type(&base.ty)?;
        match index {
            HirIndex::Static(index) => {
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = extractvalue {array_ty} {}, {index}",
                    base.value()?
                ));
                if moves && self.program.needs_drop(&expression.ty) {
                    let cleanup_depth = self.drop_slots.len();
                    let spill = self.entry_alloca(&array_ty, "resource array index spill");
                    self.instruction(format!("store {array_ty} {}, ptr {spill}", base.value()?));
                    self.register_drop_slot(None, base.ty.clone(), spill)?;
                    let root = self
                        .drop_slots
                        .last()
                        .cloned()
                        .expect("resource array spill registered a drop slot");
                    let projection = usize::try_from(*index).map_err(|_| {
                        Diagnostic::new(format!(
                            "internal error: array move index {index} does not fit this target"
                        ))
                    })?;
                    let mut updates = Vec::new();
                    Self::collect_place_flag_updates(
                        &root,
                        &[projection],
                        false,
                        false,
                        &mut updates,
                    );
                    for (flag, value) in updates {
                        self.instruction(format!("store i1 {value}, ptr {flag}"));
                    }
                    self.emit_cleanup_range(cleanup_depth)?;
                    self.drop_slots.truncate(cleanup_depth);
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirIndex::Dynamic(index) => {
                let index = self.emit_expr(index)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let wide_index = self.fresh_register();
                self.instruction(format!("{wide_index} = sext i32 {} to i64", index.value()?));
                let in_bounds = self.fresh_register();
                self.instruction(format!("{in_bounds} = icmp ult i64 {wide_index}, {length}"));
                let ok_label = self.fresh_label("index.ok");
                let trap_label = self.fresh_label("index.trap");
                self.terminate(format!(
                    "br i1 {in_bounds}, label %{ok_label}, label %{trap_label}"
                ));

                self.start_block(&trap_label);
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");

                self.start_block(&ok_label);
                let spill = self.entry_alloca(&array_ty, "array index spill");
                self.instruction(format!("store {array_ty} {}, ptr {spill}", base.value()?));
                let pointer = self.fresh_register();
                self.instruction(format!(
                    "{pointer} = getelementptr inbounds {array_ty}, ptr {spill}, i32 0, i64 {wide_index}"
                ));
                let register = self.fresh_register();
                let element_ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {element_ty}, ptr {pointer}"));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
        }
    }

    fn emit_integer_division_guard(
        &mut self,
        left: &Operand,
        right: &Operand,
    ) -> Result<(), Diagnostic> {
        let ty = llvm_value_type(&left.ty)?;
        let left_value = left.value()?.to_owned();
        let right_value = right.value()?.to_owned();
        let is_zero = self.fresh_register();
        self.instruction(format!("{is_zero} = icmp eq {ty} {right_value}, 0"));

        let invalid = if let Some(minimum) = signed_integer_min(&left.ty) {
            let is_minimum = self.fresh_register();
            self.instruction(format!(
                "{is_minimum} = icmp eq {ty} {left_value}, {minimum}"
            ));
            let is_negative_one = self.fresh_register();
            self.instruction(format!(
                "{is_negative_one} = icmp eq {ty} {right_value}, -1"
            ));
            let overflows = self.fresh_register();
            self.instruction(format!(
                "{overflows} = and i1 {is_minimum}, {is_negative_one}"
            ));
            let invalid = self.fresh_register();
            self.instruction(format!("{invalid} = or i1 {is_zero}, {overflows}"));
            invalid
        } else {
            is_zero
        };

        let ok_label = self.fresh_label("arithmetic.ok");
        let trap_label = self.fresh_label("arithmetic.trap");
        self.terminate(format!(
            "br i1 {invalid}, label %{trap_label}, label %{ok_label}"
        ));

        self.start_block(&trap_label);
        self.instruction("call void @llvm.trap()");
        self.terminate("unreachable");
        self.start_block(&ok_label);
        Ok(())
    }

    fn emit_shift_guard(&mut self, right: &Operand) -> Result<(), Diagnostic> {
        let ty = llvm_value_type(&right.ty)?;
        let invalid = self.fresh_register();
        self.instruction(format!(
            "{invalid} = icmp uge {ty} {}, {}",
            right.value()?,
            integer_bit_width(&right.ty)
        ));
        let ok_label = self.fresh_label("shift.ok");
        let trap_label = self.fresh_label("shift.trap");
        self.terminate(format!(
            "br i1 {invalid}, label %{trap_label}, label %{ok_label}"
        ));
        self.start_block(&trap_label);
        self.instruction("call void @llvm.trap()");
        self.terminate("unreachable");
        self.start_block(&ok_label);
        Ok(())
    }

    fn emit_while(&mut self, condition: &HirExpr, body: &HirExpr) -> Result<Operand, Diagnostic> {
        let condition_label = self.fresh_label("while.condition");
        let body_label = self.fresh_label("while.body");
        let end_label = self.fresh_label("while.end");
        self.terminate(format!("br label %{condition_label}"));
        self.loops.push(EmitLoopTarget {
            break_label: end_label.clone(),
            continue_label: condition_label.clone(),
            result: None,
            cleanup_depth: self.drop_slots.len(),
        });

        self.start_block(&condition_label);
        let condition = self.emit_expr(condition)?;
        if !self.terminated {
            self.terminate(format!(
                "br i1 {}, label %{body_label}, label %{end_label}",
                condition.value()?
            ));
            self.start_block(&body_label);
            self.emit_expr(body)?;
            if !self.terminated {
                self.terminate(format!("br label %{condition_label}"));
            }
        }

        self.loops.pop().expect("while emission frame");
        self.start_block(&end_label);
        Ok(Operand::unit())
    }

    fn emit_loop(&mut self, expression: &HirExpr, body: &HirExpr) -> Result<Operand, Diagnostic> {
        let body_label = self.fresh_label("loop.body");
        let end_label = self.fresh_label("loop.end");
        let result = if matches!(expression.ty, Ty::Unit | Ty::Never) {
            None
        } else {
            let ty = llvm_value_type(&expression.ty)?;
            Some((expression.ty.clone(), self.entry_alloca(&ty, "loop result")))
        };
        self.terminate(format!("br label %{body_label}"));
        self.loops.push(EmitLoopTarget {
            break_label: end_label.clone(),
            continue_label: body_label.clone(),
            result: result.clone(),
            cleanup_depth: self.drop_slots.len(),
        });
        self.start_block(&body_label);
        self.emit_expr(body)?;
        if !self.terminated {
            self.terminate(format!("br label %{body_label}"));
        }
        self.loops.pop().expect("loop emission frame");

        if expression.ty == Ty::Never {
            return Ok(Operand::never());
        }
        self.start_block(&end_label);
        let Some((ty, pointer)) = result else {
            return Ok(Operand::unit());
        };
        let register = self.fresh_register();
        let llvm_ty = llvm_value_type(&ty)?;
        self.instruction(format!("{register} = load {llvm_ty}, ptr {pointer}"));
        Ok(Operand {
            ty,
            value: Some(register),
        })
    }

    fn emit_break(&mut self, value: Option<&HirExpr>) -> Result<Operand, Diagnostic> {
        let target = self.loops.last().cloned().ok_or_else(|| {
            Diagnostic::new("internal error: break reached emission outside a loop")
        })?;
        let value = match value {
            Some(value) => Some(self.emit_expr(value)?),
            None => None,
        };
        if self.terminated {
            return Ok(Operand::never());
        }
        match (&target.result, value) {
            (Some((ty, pointer)), Some(value)) => {
                let llvm_ty = llvm_value_type(ty)?;
                self.instruction(format!("store {llvm_ty} {}, ptr {pointer}", value.value()?));
            }
            (Some(_), None) => {
                return Err(Diagnostic::new(
                    "internal error: value-producing loop break has no value",
                ));
            }
            (None, Some(value)) if value.ty != Ty::Unit => {
                return Err(Diagnostic::new(
                    "internal error: unit loop break carries a value",
                ));
            }
            (None, None) | (None, Some(_)) => {}
        }
        self.emit_cleanup_range(target.cleanup_depth)?;
        self.terminate(format!("br label %{}", target.break_label));
        Ok(Operand::never())
    }

    fn emit_continue(&mut self) -> Result<Operand, Diagnostic> {
        let target = self.loops.last().cloned().ok_or_else(|| {
            Diagnostic::new("internal error: continue reached emission outside a loop")
        })?;
        self.emit_cleanup_range(target.cleanup_depth)?;
        self.terminate(format!("br label %{}", target.continue_label));
        Ok(Operand::never())
    }

    fn emit_match(
        &mut self,
        expression: &HirExpr,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
    ) -> Result<Operand, Diagnostic> {
        let inspects_borrowed_storage = matches!(
            scrutinee.kind,
            HirExprKind::Read {
                kind: HirReadKind::Inspect,
                ..
            }
        );
        let scrutinee = self.emit_expr(scrutinee)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let match_cleanup_depth = self.drop_slots.len();
        let mut match_drop_slot = None;
        if self.program.needs_drop(&scrutinee.ty) && !inspects_borrowed_storage {
            let ty = llvm_value_type(&scrutinee.ty)?;
            let pointer = self.entry_alloca(&ty, "match scrutinee");
            self.instruction(format!("store {ty} {}, ptr {pointer}", scrutinee.value()?));
            self.register_drop_slot(None, scrutinee.ty.clone(), pointer)?;
            match_drop_slot = self.drop_slots.last().cloned();
        }
        let Ty::Enum(enum_name) = &scrutinee.ty else {
            return Err(Diagnostic::new(
                "internal error: non-enum scrutinee reached match emission",
            ));
        };
        let layout = self.program.enum_layout(enum_name).ok_or_else(|| {
            Diagnostic::new(format!("internal error: missing enum layout `{enum_name}`"))
        })?;
        let tag = self.fresh_register();
        self.instruction(format!(
            "{tag} = extractvalue {} {}, 0",
            llvm_value_type(&scrutinee.ty)?,
            scrutinee.value()?
        ));

        let mut candidates = Vec::new();
        let mut labels = Vec::new();
        for variant in 0..layout.variants.len() {
            let mut variant_candidates = Vec::new();
            for (arm_index, arm) in arms.iter().enumerate() {
                if matches!(arm.matcher, HirMatcher::All)
                    || arm.matcher == HirMatcher::Variant(variant)
                {
                    variant_candidates.push(arm_index);
                    if arm.guard.is_none() {
                        break;
                    }
                }
            }
            let variant_labels: Vec<_> = (0..variant_candidates.len())
                .map(|_| self.fresh_label("match.candidate"))
                .collect();
            candidates.push(variant_candidates);
            labels.push(variant_labels);
        }
        let default_label = self.fresh_label("match.invalid");
        let merge_label = self.fresh_label("match.end");
        let cases = labels
            .iter()
            .enumerate()
            .filter_map(|(variant, labels)| {
                labels
                    .first()
                    .map(|label| format!("i32 {variant}, label %{label}"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.terminate(format!(
            "switch i32 {tag}, label %{default_label} [ {cases} ]"
        ));

        self.start_block(&default_label);
        self.terminate("unreachable");

        let mut incoming = Vec::new();
        for variant in 0..layout.variants.len() {
            for (position, arm_index) in candidates[variant].iter().copied().enumerate() {
                self.start_block(&labels[variant][position]);
                let arm = &arms[arm_index];
                let candidate_cleanup_depth = self.drop_slots.len();
                if arm.guard.is_none() {
                    if let Some(root) = &match_drop_slot {
                        self.prepare_match_pattern_ownership(root, layout, variant, &arm.bindings)?;
                    }
                }
                self.emit_pattern_bindings(&scrutinee, &arm.bindings, arm.guard.is_none())?;

                if let Some(guard) = &arm.guard {
                    let guard = self.emit_expr(guard)?;
                    if !self.terminated {
                        let body_label = self.fresh_label("match.body");
                        let false_label = labels[variant]
                            .get(position + 1)
                            .cloned()
                            .unwrap_or_else(|| default_label.clone());
                        self.terminate(format!(
                            "br i1 {}, label %{body_label}, label %{false_label}",
                            guard.value()?
                        ));
                        self.start_block(&body_label);
                        if let Some(root) = &match_drop_slot {
                            self.prepare_match_pattern_ownership(
                                root,
                                layout,
                                variant,
                                &arm.bindings,
                            )?;
                        }
                        self.activate_pattern_binding_ownership(&arm.bindings)?;
                    }
                }

                let body = self.emit_expr(&arm.body)?;
                if !self.terminated {
                    self.emit_cleanup_range(match_cleanup_depth)?;
                    let predecessor = self.current_label.clone();
                    self.terminate(format!("br label %{merge_label}"));
                    incoming.push((body, predecessor));
                }
                self.drop_slots.truncate(candidate_cleanup_depth);
            }
        }

        if incoming.is_empty() {
            self.drop_slots.truncate(match_cleanup_depth);
            self.terminated = true;
            return Ok(Operand::never());
        }
        self.drop_slots.truncate(match_cleanup_depth);
        self.start_block(&merge_label);
        if expression.ty == Ty::Unit {
            return Ok(Operand::unit());
        }
        if incoming.len() == 1 {
            return Ok(incoming.pop().expect("one incoming match value").0);
        }
        let register = self.fresh_register();
        let incoming = incoming
            .iter()
            .map(|(operand, label)| Ok(format!("[{}, %{label}]", operand.value()?)))
            .collect::<Result<Vec<_>, Diagnostic>>()?
            .join(", ");
        self.instruction(format!(
            "{register} = phi {} {incoming}",
            llvm_value_type(&expression.ty)?
        ));
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(register),
        })
    }

    fn prepare_match_pattern_ownership(
        &mut self,
        root: &RuntimeDropSlot,
        layout: &EnumLayout,
        variant: usize,
        bindings: &[HirPatternBinding],
    ) -> Result<(), Diagnostic> {
        if !bindings.iter().any(|binding| binding.moves) {
            return Ok(());
        }
        self.instruction(format!("store i1 false, ptr {}", root.flag));
        if bindings
            .iter()
            .any(|binding| binding.moves && binding.path.is_empty())
        {
            return Ok(());
        }

        let variant = &layout.variants[variant];
        let aggregate_ty = llvm_value_type(&root.ty)?;
        for (field_index, field) in variant.fields.iter().enumerate() {
            let llvm_index = 1 + variant.payload_offset + field_index;
            let moved_paths = bindings
                .iter()
                .filter(|binding| binding.moves && binding.path.first() == Some(&llvm_index))
                .map(|binding| binding.path[1..].to_vec())
                .collect::<Vec<_>>();
            if !self.program.needs_drop(&field.ty) {
                continue;
            }
            let pointer = self.fresh_register();
            self.instruction(format!(
                "{pointer} = getelementptr inbounds {aggregate_ty}, ptr {}, i32 0, i32 {llvm_index}",
                root.pointer
            ));
            self.register_match_remainder(&field.ty, pointer, &moved_paths)?;
        }
        Ok(())
    }

    fn register_match_remainder(
        &mut self,
        ty: &Ty,
        pointer: String,
        moved_paths: &[Vec<usize>],
    ) -> Result<(), Diagnostic> {
        if moved_paths.is_empty() {
            return self.register_drop_slot(None, ty.clone(), pointer);
        }
        if moved_paths.iter().any(Vec::is_empty) || !self.program.needs_drop(ty) {
            return Ok(());
        }
        if self.program.drop_methods.contains_key(ty) {
            return Err(Diagnostic::new(format!(
                "internal error: match split custom Drop type `{ty}`"
            )));
        }
        let Ty::Struct(name) = ty else {
            return Err(Diagnostic::new(format!(
                "internal error: match move path descends through non-struct `{ty}`"
            )));
        };
        let fields = self
            .program
            .struct_layout(name)
            .ok_or_else(|| {
                Diagnostic::new(format!("internal error: missing struct layout `{name}`"))
            })?
            .fields
            .clone();
        let aggregate_ty = llvm_value_type(ty)?;
        for (index, field) in fields.iter().enumerate() {
            if !self.program.needs_drop(&field.ty) {
                continue;
            }
            let child_paths = moved_paths
                .iter()
                .filter(|path| path.first() == Some(&index))
                .map(|path| path[1..].to_vec())
                .collect::<Vec<_>>();
            let child_pointer = self.fresh_register();
            self.instruction(format!(
                "{child_pointer} = getelementptr inbounds {aggregate_ty}, ptr {pointer}, i32 0, i32 {index}"
            ));
            self.register_match_remainder(&field.ty, child_pointer, &child_paths)?;
        }
        Ok(())
    }

    fn emit_pattern_bindings(
        &mut self,
        scrutinee: &Operand,
        bindings: &[HirPatternBinding],
        activate_ownership: bool,
    ) -> Result<(), Diagnostic> {
        for binding in bindings {
            if binding.ty == Ty::Unit {
                continue;
            }
            let value = if binding.path.is_empty() {
                scrutinee.value()?.to_owned()
            } else {
                let register = self.fresh_register();
                let path = binding
                    .path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                self.instruction(format!(
                    "{register} = extractvalue {} {}, {path}",
                    llvm_value_type(&scrutinee.ty)?,
                    scrutinee.value()?
                ));
                register
            };
            let ty = llvm_value_type(&binding.ty)?;
            let pointer = self.entry_alloca(&ty, &llvm_comment(&binding.name));
            self.instruction(format!("store {ty} {value}, ptr {pointer}"));
            self.locals.insert(binding.id, pointer.clone());
            if activate_ownership && binding.moves && self.program.needs_drop(&binding.ty) {
                self.register_drop_slot(Some(binding.id), binding.ty.clone(), pointer)?;
            }
        }
        Ok(())
    }

    fn activate_pattern_binding_ownership(
        &mut self,
        bindings: &[HirPatternBinding],
    ) -> Result<(), Diagnostic> {
        for binding in bindings {
            if !binding.moves || !self.program.needs_drop(&binding.ty) {
                continue;
            }
            let pointer = self.locals.get(&binding.id).cloned().ok_or_else(|| {
                Diagnostic::new(format!(
                    "internal error: missing speculative pattern binding `{}`",
                    binding.name
                ))
            })?;
            self.register_drop_slot(Some(binding.id), binding.ty.clone(), pointer)?;
        }
        Ok(())
    }

    fn emit_short_circuit(
        &mut self,
        left: &HirExpr,
        right: &HirExpr,
        short_value: bool,
    ) -> Result<Operand, Diagnostic> {
        let left = self.emit_expr(left)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let left_label = self.current_label.clone();
        let right_label = self.fresh_label("logic.rhs");
        let merge_label = self.fresh_label("logic.end");
        if short_value {
            self.terminate(format!(
                "br i1 {}, label %{merge_label}, label %{right_label}",
                left.value()?
            ));
        } else {
            self.terminate(format!(
                "br i1 {}, label %{right_label}, label %{merge_label}",
                left.value()?
            ));
        }

        self.start_block(&right_label);
        let right = self.emit_expr(right)?;
        let right_end = self.current_label.clone();
        let right_reaches_merge = !self.terminated;
        if right_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        self.start_block(&merge_label);
        let short = if short_value { 1 } else { 0 };
        if !right_reaches_merge {
            return Ok(Operand {
                ty: Ty::Bool,
                value: Some(short.to_string()),
            });
        }
        let register = self.fresh_register();
        self.instruction(format!(
            "{register} = phi i1 [{short}, %{left_label}], [{}, %{right_end}]",
            right.value()?
        ));
        Ok(Operand {
            ty: Ty::Bool,
            value: Some(register),
        })
    }

    fn emit_if(
        &mut self,
        expression: &HirExpr,
        condition: &HirExpr,
        then_branch: &HirExpr,
        else_branch: Option<&HirExpr>,
    ) -> Result<Operand, Diagnostic> {
        let condition = self.emit_expr(condition)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let then_label = self.fresh_label("if.then");
        let else_label = self.fresh_label("if.else");
        let merge_label = self.fresh_label("if.end");
        self.terminate(format!(
            "br i1 {}, label %{then_label}, label %{else_label}",
            condition.value()?
        ));

        self.start_block(&then_label);
        let then_value = self.emit_expr(then_branch)?;
        let then_end = self.current_label.clone();
        let then_reaches_merge = !self.terminated;
        if then_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        self.start_block(&else_label);
        let else_value = if let Some(else_branch) = else_branch {
            self.emit_expr(else_branch)?
        } else {
            Operand::unit()
        };
        let else_end = self.current_label.clone();
        let else_reaches_merge = !self.terminated;
        if else_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        if !then_reaches_merge && !else_reaches_merge {
            self.terminated = true;
            return Ok(Operand::never());
        }
        self.start_block(&merge_label);
        if expression.ty == Ty::Unit {
            return Ok(Operand::unit());
        }
        if !then_reaches_merge {
            return Ok(else_value);
        }
        if !else_reaches_merge {
            return Ok(then_value);
        }
        let register = self.fresh_register();
        let ty = llvm_value_type(&expression.ty)?;
        self.instruction(format!(
            "{register} = phi {ty} [{}, %{then_end}], [{}, %{else_end}]",
            then_value.value()?,
            else_value.value()?
        ));
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(register),
        })
    }

    fn emit_borrow_address(&mut self, place: &HirPlace) -> Result<String, Diagnostic> {
        if place.projections.is_empty()
            && matches!(place.ty, Ty::Callable(_))
            && self.partial_captures.contains_key(&place.local)
        {
            let expression = HirExpr {
                ty: place.ty.clone(),
                kind: HirExprKind::Unit,
            };
            let callable =
                self.emit_stored_callable_read(&expression, place.local, HirReadKind::Copy)?;
            let environment_ty = llvm_value_type(&callable.ty)?;
            let environment = self.entry_alloca(&environment_ty, "borrowed callable environment");
            self.instruction(format!(
                "store {environment_ty} {}, ptr {environment}",
                callable.value()?
            ));
            return Ok(environment);
        }
        self.emit_place_address(place)
    }

    fn emit_place_address(&mut self, place: &HirPlace) -> Result<String, Diagnostic> {
        let mut root_pointer = self.locals.get(&place.local).cloned().ok_or_else(|| {
            Diagnostic::new(format!(
                "internal error: unknown local id {} in function `{}`",
                place.local, self.function.name
            ))
        })?;
        if place.indirect {
            let loaded = self.fresh_register();
            self.instruction(format!("{loaded} = load ptr, ptr {root_pointer}"));
            root_pointer = loaded;
        }
        if place.projections.is_empty() {
            return Ok(root_pointer);
        }
        let pointer = self.fresh_register();
        let indices = place
            .projections
            .iter()
            .map(|index| format!("i32 {index}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.instruction(format!(
            "{pointer} = getelementptr inbounds {}, ptr {root_pointer}, i32 0, {indices}",
            llvm_value_type(&place.root_ty)?
        ));
        Ok(pointer)
    }

    fn emit_callable_environment(
        &mut self,
        expression: &HirExpr,
        captures: &[HirArgument],
    ) -> Result<Operand, Diagnostic> {
        let cleanup_depth = self.drop_slots.len();
        let environment_ty = llvm_value_type(&expression.ty)?;
        let mut environment = "zeroinitializer".to_owned();
        for (index, capture) in captures.iter().enumerate() {
            let (field_ty, value) = match capture {
                HirArgument::Copy(value) | HirArgument::Move(value) => {
                    let value = self.emit_expr(value)?;
                    if self.terminated {
                        self.drop_slots.truncate(cleanup_depth);
                        return Ok(Operand::never());
                    }
                    if value.ty == Ty::Unit {
                        ("[0 x i8]".to_owned(), "zeroinitializer".to_owned())
                    } else {
                        self.hold_operand_for_early_exit(&value)?;
                        (llvm_value_type(&value.ty)?, value.value()?.to_owned())
                    }
                }
                HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                    ("ptr".to_owned(), self.emit_borrow_address(place)?)
                }
                HirArgument::CallableCaptureBorrow {
                    binding,
                    index,
                    callable_ty,
                    ..
                } => (
                    "ptr".to_owned(),
                    self.emit_callable_capture_borrow(*binding, *index, callable_ty)?,
                ),
            };
            let register = self.fresh_register();
            self.instruction(format!(
                "{register} = insertvalue {environment_ty} {environment}, {field_ty} {value}, {index}"
            ));
            environment = register;
        }
        self.release_drop_slots(cleanup_depth);
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(environment),
        })
    }

    fn emit_stored_callable_read(
        &mut self,
        expression: &HirExpr,
        source: LocalId,
        kind: HirReadKind,
    ) -> Result<Operand, Diagnostic> {
        let Ty::Callable(callable) = &expression.ty else {
            return Err(Diagnostic::new(
                "internal error: stored callable read has a non-callable type",
            ));
        };
        let stored =
            self.partial_captures.get(&source).cloned().ok_or_else(|| {
                Diagnostic::new("internal error: callable environment is missing")
            })?;
        if stored.len() != callable.captures.len() {
            return Err(Diagnostic::new(
                "internal error: callable environment layout does not match its type",
            ));
        }
        let environment_ty = llvm_value_type(&expression.ty)?;
        let mut environment = "zeroinitializer".to_owned();
        for (index, (capture, stored)) in callable.captures.iter().zip(stored).enumerate() {
            if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow) {
                let stored = stored.ok_or_else(|| {
                    Diagnostic::new(format!(
                        "internal error: borrowed callable capture {index} has no storage"
                    ))
                })?;
                let value = self.fresh_register();
                self.instruction(format!("{value} = load ptr, ptr {}", stored.pointer));
                let next = self.fresh_register();
                self.instruction(format!(
                    "{next} = insertvalue {environment_ty} {environment}, ptr {value}, {index}"
                ));
                environment = next;
                continue;
            }
            let (field_ty, value) = if capture.ty == Ty::Unit {
                ("[0 x i8]".to_owned(), "zeroinitializer".to_owned())
            } else {
                let stored = stored.ok_or_else(|| {
                    Diagnostic::new(format!(
                        "internal error: callable capture {index} has no storage"
                    ))
                })?;
                let field_ty = llvm_value_type(&capture.ty)?;
                let value = self.fresh_register();
                self.instruction(format!("{value} = load {field_ty}, ptr {}", stored.pointer));
                if kind == HirReadKind::Move && stored.drop_flag.is_some() {
                    self.deactivate_drop_slot_at(&stored.pointer);
                }
                (field_ty, value)
            };
            let next = self.fresh_register();
            self.instruction(format!(
                "{next} = insertvalue {environment_ty} {environment}, {field_ty} {value}, {index}"
            ));
            environment = next;
        }
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(environment),
        })
    }

    fn emit_callable_capture_borrow(
        &mut self,
        binding: LocalId,
        index: usize,
        callable_ty: &Ty,
    ) -> Result<String, Diagnostic> {
        if let Some(stored) = self
            .partial_captures
            .get(&binding)
            .and_then(|captures| captures.get(index))
            .cloned()
        {
            let stored = stored.ok_or_else(|| {
                Diagnostic::new(format!(
                    "internal error: borrowed callable capture {index} has no storage"
                ))
            })?;
            let pointer = self.fresh_register();
            self.instruction(format!("{pointer} = load ptr, ptr {}", stored.pointer));
            return Ok(pointer);
        }

        let environment = self.locals.get(&binding).cloned().ok_or_else(|| {
            Diagnostic::new(format!(
                "internal error: unknown callable environment local {binding}"
            ))
        })?;
        let environment_ty = llvm_value_type(callable_ty)?;
        let field = self.fresh_register();
        self.instruction(format!(
            "{field} = getelementptr inbounds {environment_ty}, ptr {environment}, i32 0, i32 {index}"
        ));
        let pointer = self.fresh_register();
        self.instruction(format!("{pointer} = load ptr, ptr {field}"));
        Ok(pointer)
    }

    fn relocate_callable_captures(
        &mut self,
        source: LocalId,
        destination: LocalId,
        callable_ty: &Ty,
    ) -> Result<(), Diagnostic> {
        let Ty::Callable(callable) = callable_ty else {
            return Err(Diagnostic::new(
                "internal error: relocated callable has no callable type",
            ));
        };
        let captures = self.partial_captures.remove(&source).ok_or_else(|| {
            Diagnostic::new(format!(
                "internal error: callable local {source} has no stored environment"
            ))
        })?;
        let mut relocated = Vec::with_capacity(captures.len());
        if captures.len() != callable.captures.len() {
            return Err(Diagnostic::new(
                "internal error: relocated callable capture shape changed",
            ));
        }
        for (capture, capture_ty) in captures.into_iter().zip(&callable.captures) {
            let Some(capture) = capture else {
                relocated.push(None);
                continue;
            };
            if matches!(capture_ty.mode, PassMode::Borrow | PassMode::MutBorrow) {
                let value = self.fresh_register();
                self.instruction(format!("{value} = load ptr, ptr {}", capture.pointer));
                let pointer = self.entry_alloca("ptr", "moved borrowed callable capture");
                self.instruction(format!("store ptr {value}, ptr {pointer}"));
                relocated.push(Some(StoredCapture {
                    ty: capture.ty,
                    pointer,
                    drop_flag: None,
                }));
                continue;
            }
            let ty = llvm_value_type(&capture.ty)?;
            let value = self.fresh_register();
            self.instruction(format!("{value} = load {ty}, ptr {}", capture.pointer));
            if capture.drop_flag.is_some() {
                self.deactivate_drop_slot_at(&capture.pointer);
            }
            let pointer = self.entry_alloca(&ty, "moved callable capture");
            self.instruction(format!("store {ty} {value}, ptr {pointer}"));
            let drop_flag = if self.program.needs_drop(&capture.ty) {
                self.register_drop_slot(None, capture.ty.clone(), pointer.clone())?;
                Some(
                    self.drop_slots
                        .last()
                        .expect("registered relocated callable capture slot")
                        .flag
                        .clone(),
                )
            } else {
                None
            };
            relocated.push(Some(StoredCapture {
                ty: capture.ty,
                pointer,
                drop_flag,
            }));
        }
        self.partial_captures.insert(destination, relocated);
        Ok(())
    }

    fn entry_alloca(&mut self, ty: &str, comment: &str) -> String {
        let pointer = self.fresh_register();
        self.entry_allocas.push_str("  ");
        self.entry_allocas.push_str(&pointer);
        self.entry_allocas.push_str(" = alloca ");
        self.entry_allocas.push_str(ty);
        if !comment.is_empty() {
            self.entry_allocas.push_str(" ; ");
            self.entry_allocas.push_str(comment);
        }
        self.entry_allocas.push('\n');
        pointer
    }

    fn fresh_register(&mut self) -> String {
        let register = format!("%v{}", self.next_register);
        self.next_register += 1;
        register
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!("{prefix}.{}", self.next_label);
        self.next_label += 1;
        label
    }

    fn instruction(&mut self, instruction: impl fmt::Display) {
        debug_assert!(!self.terminated);
        self.output.push_str("  ");
        self.output.push_str(&instruction.to_string());
        self.output.push('\n');
    }

    fn terminate(&mut self, instruction: impl fmt::Display) {
        self.instruction(instruction);
        self.terminated = true;
    }

    fn start_block(&mut self, label: &str) {
        self.output.push_str(label);
        self.output.push_str(":\n");
        self.current_label = label.to_owned();
        self.terminated = false;
    }
}

fn llvm_return_type(ty: &Ty) -> Result<String, Diagnostic> {
    if *ty == Ty::Unit {
        Ok("void".to_owned())
    } else {
        llvm_value_type(ty)
    }
}

fn llvm_value_type(ty: &Ty) -> Result<String, Diagnostic> {
    match ty {
        Ty::I32 | Ty::U32 => Ok("i32".to_owned()),
        Ty::I64 | Ty::U64 => Ok("i64".to_owned()),
        Ty::Bool => Ok("i1".to_owned()),
        Ty::Array(element, length) => Ok(format!("[{length} x {}]", llvm_value_type(element)?)),
        Ty::Pointer { .. } | Ty::Reference { .. } | Ty::Function(_) => Ok("ptr".to_owned()),
        Ty::Struct(name) | Ty::Enum(name) => Ok(format!("%{}", type_symbol(name))),
        Ty::Callable(_) => Ok(format!("%{}", type_symbol(&canonical_type_encoding(ty)))),
        Ty::Continuation { .. } => Ok("%salicin.continuation".to_owned()),
        Ty::EffectCallable { .. } => Ok("%salicin.effect_callable".to_owned()),
        Ty::Unit | Ty::Never | Ty::EffectRow { .. } | Ty::Error => Err(Diagnostic::new(format!(
            "internal error: `{ty}` has no first-class LLVM representation"
        ))),
    }
}

fn llvm_field_type(ty: &Ty) -> Result<String, Diagnostic> {
    if *ty == Ty::Unit {
        Ok("[0 x i8]".to_owned())
    } else {
        llvm_value_type(ty)
    }
}

fn llvm_layout_const(ty: &Ty, kind: LayoutQueryKind) -> Result<String, Diagnostic> {
    if matches!(ty, Ty::Unit | Ty::Never) {
        return Ok(match kind {
            LayoutQueryKind::Size => "0",
            LayoutQueryKind::Align => "1",
        }
        .to_owned());
    }
    let llvm_ty = llvm_value_type(ty)?;
    Ok(match kind {
        LayoutQueryKind::Size => {
            format!("ptrtoint (ptr getelementptr ({llvm_ty}, ptr null, i32 1) to i64)")
        }
        LayoutQueryKind::Align => format!(
            "ptrtoint (ptr getelementptr ({{ i8, {llvm_ty} }}, ptr null, i32 0, i32 1) to i64)"
        ),
    })
}

fn zero_const(ty: &Ty, program: &HirProgram) -> Option<ConstValue> {
    match ty {
        Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 => Some(ConstValue::Integer(0)),
        Ty::Bool => Some(ConstValue::Bool(false)),
        Ty::Unit => Some(ConstValue::Unit),
        Ty::Pointer { .. } | Ty::Reference { .. } => None,
        Ty::Array(element, length) => {
            let length = usize::try_from(*length).ok()?;
            Some(ConstValue::Aggregate(
                (0..length)
                    .map(|_| zero_const(element, program))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        Ty::Struct(name) => Some(ConstValue::Aggregate(
            program
                .struct_layout(name)?
                .fields
                .iter()
                .map(|field| zero_const(&field.ty, program))
                .collect::<Option<Vec<_>>>()?,
        )),
        Ty::Enum(name) => {
            let layout = program.enum_layout(name)?;
            let mut fields = vec![ConstValue::Integer(0)];
            fields.extend(
                layout
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| zero_const(&field.ty, program))
                    .collect::<Option<Vec<_>>>()?,
            );
            Some(ConstValue::Aggregate(fields))
        }
        Ty::Never
        | Ty::Function(_)
        | Ty::Callable(_)
        | Ty::Continuation { .. }
        | Ty::EffectCallable { .. }
        | Ty::EffectRow { .. }
        | Ty::Error => None,
    }
}

fn const_ir(value: &ConstValue, ty: &Ty, program: &HirProgram) -> Result<String, Diagnostic> {
    match (value, ty) {
        (ConstValue::Integer(value), ty) if ty.is_integer() => Ok(value.to_string()),
        (ConstValue::Bool(value), Ty::Bool) => Ok(if *value { "1" } else { "0" }.to_owned()),
        (ConstValue::Unit, Ty::Unit) => Ok("zeroinitializer".to_owned()),
        (ConstValue::LayoutQuery(queried, kind), Ty::U64) => llvm_layout_const(queried, *kind),
        (ConstValue::Aggregate(values), Ty::Array(element, length)) => {
            if values.len() as u64 != *length {
                return Err(Diagnostic::new(
                    "internal error: constant array length does not match its type",
                ));
            }
            let element_ty = llvm_value_type(element)?;
            let elements = values
                .iter()
                .map(|value| {
                    Ok(format!(
                        "{element_ty} {}",
                        const_ir(value, element, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("[{}]", elements.join(", ")))
        }
        (ConstValue::Aggregate(values), Ty::Struct(name)) => {
            let layout = program.struct_layout(name).ok_or_else(|| {
                Diagnostic::new(format!("internal error: missing struct layout `{name}`"))
            })?;
            let fields = values
                .iter()
                .zip(&layout.fields)
                .map(|(value, field)| {
                    Ok(format!(
                        "{} {}",
                        llvm_field_type(&field.ty)?,
                        const_ir(value, &field.ty, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("{{ {} }}", fields.join(", ")))
        }
        (ConstValue::Aggregate(values), Ty::Enum(name)) => {
            let layout = program.enum_layout(name).ok_or_else(|| {
                Diagnostic::new(format!("internal error: missing enum layout `{name}`"))
            })?;
            let mut types = vec![Ty::U32];
            types.extend(
                layout
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| field.ty.clone()),
            );
            let fields = values
                .iter()
                .zip(types)
                .map(|(value, ty)| {
                    Ok(format!(
                        "{} {}",
                        llvm_field_type(&ty)?,
                        const_ir(value, &ty, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("{{ {} }}", fields.join(", ")))
        }
        _ => Err(Diagnostic::new(format!(
            "internal error: constant value does not have type `{ty}`"
        ))),
    }
}
