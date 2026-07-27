use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{BinaryOp, ItemOrigin, PassMode, StructRepresentation, Type, UnaryOp, Visibility};

use super::ctfe_value::CtfeValue;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Ty {
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    Bool,
    Unit,
    Tuple(Vec<Ty>),
    Pointer {
        pointee: Box<Ty>,
        mutable: bool,
    },
    Reference {
        pointee: Box<Ty>,
        mutable: bool,
        region: Option<String>,
    },
    Slice(Box<Ty>),
    Array(Box<Ty>, u64),
    Struct(String),
    Enum(String),
    Never,
    Function(FunctionTy),
    Callable(CallableTy),
    Continuation {
        input: Box<Ty>,
        output: Box<Ty>,
    },
    EffectCallable {
        input: Box<Ty>,
        output: Box<Ty>,
        answer: Box<Ty>,
    },
    EffectRow {
        unsafe_effect: bool,
        throws_error: Option<Box<Ty>>,
        custom_effects: Vec<String>,
    },
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct FunctionTy {
    pub(super) groups: Vec<Vec<Ty>>,
    pub(super) unsafe_effect: bool,
    pub(super) throws_error: Option<Box<Ty>>,
    pub(super) custom_effects: Vec<String>,
    pub(super) result: Box<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CallableTy {
    pub(super) signature: FunctionTy,
    pub(super) captures: Vec<CallableCaptureTy>,
    pub(super) kind: CallableKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct CallableCaptureTy {
    pub(super) ty: Ty,
    pub(super) mode: PassMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum CallableKind {
    Partial {
        function: String,
        consumed_groups: usize,
        is_fn_mut: bool,
        is_fn_once: bool,
    },
    Closure {
        function: String,
        is_fn_mut: bool,
        is_fn_once: bool,
    },
}

impl Ty {
    pub(super) fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::I128
                | Self::ISize
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::U128
                | Self::USize
        )
    }

    pub(super) fn is_signed(&self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::ISize
        )
    }
}

pub(super) fn type_is_assignable(actual: &Ty, expected: &Ty) -> bool {
    if actual == expected {
        return true;
    }
    match (actual, expected) {
        (Ty::Function(actual), Ty::Function(expected)) => {
            function_type_is_assignable(actual, expected)
        }
        _ => false,
    }
}

pub(super) fn function_type_is_assignable(actual: &FunctionTy, expected: &FunctionTy) -> bool {
    actual.groups.len() == expected.groups.len()
        && actual
            .groups
            .iter()
            .zip(&expected.groups)
            .all(|(actual_group, expected_group)| {
                actual_group.len() == expected_group.len()
                    && actual_group.iter().zip(expected_group).all(
                        |(actual_parameter, expected_parameter)| {
                            type_is_assignable(expected_parameter, actual_parameter)
                        },
                    )
            })
        && type_is_assignable(&actual.result, &expected.result)
        && (!actual.unsafe_effect || expected.unsafe_effect)
        && match (&actual.throws_error, &expected.throws_error) {
            (None, _) => true,
            (Some(actual), Some(expected)) => type_is_assignable(actual, expected),
            (Some(_), None) => false,
        }
        && actual
            .custom_effects
            .iter()
            .all(|effect| expected.custom_effects.contains(effect))
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I8 => f.write_str("i8"),
            Self::I16 => f.write_str("i16"),
            Self::I32 => f.write_str("i32"),
            Self::I64 => f.write_str("i64"),
            Self::I128 => f.write_str("i128"),
            Self::ISize => f.write_str("isize"),
            Self::U8 => f.write_str("u8"),
            Self::U16 => f.write_str("u16"),
            Self::U32 => f.write_str("u32"),
            Self::U64 => f.write_str("u64"),
            Self::U128 => f.write_str("u128"),
            Self::USize => f.write_str("usize"),
            Self::Bool => f.write_str("bool"),
            Self::Unit => f.write_str("()"),
            Self::Tuple(fields) => {
                f.write_str("(")?;
                for (index, ty) in fields.iter().enumerate() {
                    if index != 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                if fields.len() == 1 {
                    f.write_str(",")?;
                }
                f.write_str(")")
            }
            Self::Pointer { pointee, mutable } => {
                write!(
                    f,
                    "{}({pointee})",
                    if *mutable { "ptr(mut)" } else { "ptr" }
                )
            }
            Self::Reference {
                pointee,
                mutable,
                region,
            } => {
                let qualifier = if *mutable { "borrow(mut)" } else { "borrow" };
                if let Some(region) = region {
                    write!(
                        f,
                        "{qualifier}({}) {pointee}",
                        display_region_argument(region)
                    )
                } else {
                    write!(f, "{qualifier} {pointee}")
                }
            }
            Self::Slice(element) => write!(f, "slice({element})"),
            Self::Array(element, length) => write!(f, "array({element})({length})"),
            Self::Struct(name) | Self::Enum(name) => f.write_str(name),
            Self::Never => f.write_str("never"),
            Self::Error => f.write_str("<error>"),
            Self::Function(function) => {
                for group in &function.groups {
                    f.write_str("(")?;
                    for (index, ty) in group.iter().enumerate() {
                        if index != 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{ty}")?;
                    }
                    f.write_str(")")?;
                }
                f.write_str(": ")?;
                write!(f, "{}", function.result)?;
                let mut effects = function.custom_effects.clone();
                if function.unsafe_effect {
                    effects.insert(0, "unsafe_effect".to_owned());
                }
                if let Some(error) = &function.throws_error {
                    effects.push(format!("throws({error})"));
                }
                if !effects.is_empty() {
                    write!(f, " with({})", effects.join(", "))?;
                }
                Ok(())
            }
            Self::Callable(callable) => write!(f, "{}", Ty::Function(callable.signature.clone())),
            Self::Continuation { input, output } => {
                write!(f, "Continuation({input}, {output})")
            }
            Self::EffectCallable {
                input,
                output,
                answer,
            } => write!(f, "EffectCallable({input}, {output}, {answer})"),
            Self::EffectRow {
                unsafe_effect,
                throws_error,
                custom_effects,
            } => {
                let mut effects = custom_effects.clone();
                if *unsafe_effect {
                    effects.insert(0, "unsafe_effect".to_owned());
                }
                if let Some(error) = throws_error {
                    effects.push(format!("throws({error})"));
                }
                if effects.is_empty() {
                    f.write_str("pure")
                } else {
                    write!(f, "with({})", effects.join(", "))
                }
            }
        }
    }
}

impl Ty {
    pub(super) fn is_sized_value(&self) -> bool {
        match self {
            Self::Slice(_) => false,
            Self::Array(element, _) => element.is_sized_value(),
            Self::Tuple(fields) => fields.iter().all(Self::is_sized_value),
            Self::Pointer { .. } | Self::Reference { .. } => true,
            _ => true,
        }
    }
}

fn display_region_argument(region: &str) -> String {
    if region.starts_with('$') {
        "'_".to_owned()
    } else if region.chars().next().is_some_and(char::is_uppercase) {
        region.to_owned()
    } else {
        format!("'{region}")
    }
}

#[cfg(test)]
mod tests {
    use super::Ty;

    #[test]
    fn internal_region_names_are_anonymous_in_diagnostics() {
        let ty = Ty::Reference {
            pointee: Box::new(Ty::I32),
            mutable: false,
            region: Some("$function$region$binder$0".to_owned()),
        };

        assert_eq!(ty.to_string(), "borrow('_) i32");
    }
}

pub(super) type LocalId = usize;

#[derive(Debug, Clone)]
pub(super) struct FieldLayout {
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) access: AccessBoundary,
}

#[derive(Debug, Clone)]
pub(super) struct StructLayout {
    pub(super) name: String,
    pub(super) source_name: String,
    pub(super) representation: StructRepresentation,
    pub(super) fields: Vec<FieldLayout>,
}

#[derive(Debug, Clone)]
pub(super) struct VariantLayout {
    pub(super) name: String,
    pub(super) fields: Vec<FieldLayout>,
    pub(super) payload_offset: usize,
    pub(super) named: bool,
}

#[derive(Debug, Clone)]
pub(super) struct EnumLayout {
    pub(super) name: String,
    pub(super) variants: Vec<VariantLayout>,
}

#[derive(Debug, Clone)]
pub(super) struct HirProgram {
    pub(super) structs: Vec<StructLayout>,
    pub(super) enums: Vec<EnumLayout>,
    pub(super) globals: Vec<HirGlobal>,
    pub(super) normalized_globals: HashMap<String, CtfeValue>,
    pub(super) exported_globals: HashMap<String, String>,
    pub(super) functions: Vec<HirFunction>,
    pub(super) exported_functions: HashMap<String, String>,
    pub(super) foreign_functions: Vec<HirForeignFunction>,
    pub(super) drop_methods: HashMap<Ty, String>,
    pub(super) array_types: HashSet<Ty>,
    pub(super) tuple_types: HashSet<Ty>,
    pub(super) continuation_adapters: Vec<ContinuationAdapter>,
    pub(super) effect_callable_adapters: Vec<EffectCallableAdapter>,
    pub(super) async_states: HashMap<String, AsyncStateLayout>,
}

#[derive(Debug, Clone)]
pub(super) struct AsyncStateLayout {
    pub(super) owned_capture_fields: Vec<usize>,
    pub(super) starting_fields: Vec<usize>,
    pub(super) suspended_fields: Vec<usize>,
    pub(super) chained_fields: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct HirForeignFunction {
    pub(super) name: String,
    pub(super) link_name: String,
    pub(super) params: Vec<Ty>,
    pub(super) result: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct ContinuationAdapter {
    pub(super) name: String,
    pub(super) callable_ty: Ty,
    pub(super) function: String,
    pub(super) captures: Vec<CallableCaptureTy>,
    pub(super) input: Ty,
    pub(super) output: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct EffectCallableAdapter {
    pub(super) name: String,
    pub(super) callable_ty: Ty,
    pub(super) function: String,
    pub(super) captures: Vec<CallableCaptureTy>,
    pub(super) input: Ty,
    pub(super) output: Ty,
    pub(super) answer: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeHandlerAction {
    pub(super) effect: String,
    pub(super) input: Ty,
    pub(super) output: Ty,
    pub(super) answer: Ty,
    pub(super) accepts_input: bool,
}

impl HirProgram {
    pub(super) fn function_symbol(&self, name: &str) -> String {
        self.exported_functions
            .get(name)
            .cloned()
            .unwrap_or_else(|| super::names::function_symbol(name))
    }

    pub(super) fn global_symbol(&self, name: &str) -> String {
        self.exported_globals
            .get(name)
            .cloned()
            .unwrap_or_else(|| super::names::global_symbol(name))
    }

    pub(super) fn foreign_link_name(&self, name: &str) -> Option<&str> {
        self.foreign_functions
            .iter()
            .find(|function| function.name == name)
            .map(|function| function.link_name.as_str())
    }

    pub(super) fn struct_layout(&self, name: &str) -> Option<&StructLayout> {
        self.structs.iter().find(|layout| layout.name == name)
    }

    pub(super) fn enum_layout(&self, name: &str) -> Option<&EnumLayout> {
        self.enums.iter().find(|layout| layout.name == name)
    }

    pub(super) fn is_uninhabited(&self, ty: &Ty) -> bool {
        *ty == Ty::Never
            || matches!(ty, Ty::Enum(name) if self.enum_layout(name).is_some_and(|layout| layout.variants.is_empty()))
    }

    pub(super) fn needs_drop(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Array(element, _) => self.needs_drop(element),
            Ty::Tuple(fields) => fields.iter().any(|field| self.needs_drop(field)),
            Ty::Pointer { .. } | Ty::Reference { .. } | Ty::Slice(_) => false,
            Ty::Struct(name) => {
                self.drop_methods.contains_key(ty)
                    || self.struct_layout(name).is_some_and(|layout| {
                        layout.fields.iter().any(|field| self.needs_drop(&field.ty))
                    })
            }
            Ty::Enum(name) => {
                self.drop_methods.contains_key(ty)
                    || self.enum_layout(name).is_some_and(|layout| {
                        layout.variants.iter().any(|variant| {
                            variant
                                .fields
                                .iter()
                                .any(|field| self.needs_drop(&field.ty))
                        })
                    })
            }
            Ty::Function(_) | Ty::EffectRow { .. } => false,
            Ty::Callable(callable) => callable.captures.iter().any(|capture| {
                !matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)
                    && self.needs_drop(&capture.ty)
            }),
            Ty::Continuation { .. } | Ty::EffectCallable { .. } => true,
            Ty::I8
            | Ty::I16
            | Ty::I32
            | Ty::I64
            | Ty::I128
            | Ty::ISize
            | Ty::U8
            | Ty::U16
            | Ty::U32
            | Ty::U64
            | Ty::U128
            | Ty::USize
            | Ty::Bool
            | Ty::Unit
            | Ty::Never
            | Ty::Error => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct HirGlobal {
    pub(super) name: String,
    pub(super) ty: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct HirFunction {
    pub(super) name: String,
    pub(super) params: Vec<HirParam>,
    pub(super) result: Ty,
    pub(super) body: HirExpr,
}

#[derive(Debug, Clone)]
pub(super) struct HirParam {
    pub(super) id: LocalId,
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) mode: PassMode,
}

#[derive(Debug, Clone)]
pub(super) struct HirBinding {
    pub(super) id: LocalId,
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) mutable: bool,
    pub(super) value: HirExpr,
}

#[derive(Debug, Clone)]
pub(super) enum HirStmt {
    Let(HirBinding),
    Expr(HirExpr),
}

#[derive(Debug, Clone)]
pub(super) struct HirExpr {
    pub(super) ty: Ty,
    pub(super) kind: HirExprKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct HirPlace {
    pub(super) local: LocalId,
    pub(super) root_ty: Ty,
    pub(super) projections: Vec<usize>,
    pub(super) dynamic_index: Option<HirDynamicIndex>,
    pub(super) ty: Ty,
    pub(super) capability: LocalCapability,
    pub(super) root_mutable: bool,
    pub(super) loan: Option<LoanId>,
    pub(super) indirect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct HirDynamicIndex {
    pub(super) local: LocalId,
    pub(super) length: u64,
    pub(super) array_ty: Ty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HirReadKind {
    Copy,
    Inspect,
    Move,
}

/// Describes whether an assignment replaces a value that is definitely live.
///
/// This is intentionally independent of `copyable` and of any future `droppable`
/// decision.  The cleanup planner can use it later to decide whether an old
/// value may need cleanup before the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AssignmentKind {
    Initialize,
    Overwrite,
    MaybeOverwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AccessKind {
    Auto,
    Copy,
    Move,
    SharedBorrow,
    MutBorrow,
}

#[derive(Debug, Clone)]
pub(super) enum HirArgument {
    Copy(HirExpr),
    Move(HirExpr),
    SharedBorrow(HirPlace),
    MutBorrow(HirPlace),
    CallableCaptureBorrow {
        binding: LocalId,
        index: usize,
        callable_ty: Ty,
        capture_ty: Ty,
        mutable: bool,
    },
}

pub(super) enum ReferenceCallSource<'a> {
    Place(&'a HirPlace),
    Expression(&'a HirExpr),
}

#[derive(Debug, Clone)]
pub(super) struct HirPatternBinding {
    pub(super) id: LocalId,
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) path: Vec<usize>,
    pub(super) moves: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HirMatcher {
    Variant(usize),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LayoutQueryKind {
    Size,
    Align,
}

#[derive(Debug, Clone)]
pub(super) struct HirMatchArm {
    pub(super) matcher: HirMatcher,
    pub(super) bindings: Vec<HirPatternBinding>,
    pub(super) guard: Option<HirExpr>,
    pub(super) body: HirExpr,
}

#[derive(Debug, Clone)]
pub(super) enum HirExprKind {
    Integer(i128),
    Bool(bool),
    Unit,
    Tuple(Vec<HirExpr>),
    Array(Vec<HirExpr>),
    Index {
        base: Box<HirExpr>,
        index: HirIndex,
        length: u64,
        moves: bool,
    },
    Read {
        place: HirPlace,
        kind: HirReadKind,
    },
    Global(String),
    Function(String),
    Unary(UnaryOp, Box<HirExpr>),
    Binary(Box<HirExpr>, BinaryOp, Box<HirExpr>),
    Assign {
        place: HirPlace,
        value: Box<HirExpr>,
        assignment: AssignmentKind,
        root_initialized: bool,
    },
    Call {
        function: String,
        arguments: Vec<HirArgument>,
        consumed_callable: Option<LocalId>,
        /// Preserves an uninhabited callee result even when contextual
        /// coercion gives the enclosing expression a different type.
        diverges: bool,
    },
    IndirectCall {
        callee: Box<HirExpr>,
        arguments: Vec<HirArgument>,
        diverges: bool,
    },
    TailCall {
        function: String,
        arguments: Vec<HirArgument>,
        consumed_callable: Option<LocalId>,
        result: Ty,
    },
    TailInvokeContinuation {
        continuation: Box<HirExpr>,
        argument: Box<HirExpr>,
        result: Ty,
    },
    EraseContinuation {
        binding: LocalId,
        callable_ty: Ty,
        adapter: String,
    },
    InvokeContinuation {
        continuation: Box<HirExpr>,
        argument: Box<HirExpr>,
    },
    EraseEffectCallable {
        binding: LocalId,
        callable_ty: Ty,
        adapter: String,
    },
    InvokeEffectCallable {
        action: Box<HirExpr>,
        input: Box<HirExpr>,
        continuation: Box<HirExpr>,
    },
    Partial {
        function: String,
        consumed_groups: usize,
        captures: Vec<HirArgument>,
    },
    PartialCapture {
        binding: LocalId,
        index: usize,
        moves: bool,
        callable_ty: Ty,
    },
    LocalClosure(ClosureInfo),
    ConstructStruct {
        name: String,
        fields: Vec<(usize, HirExpr)>,
    },
    ConstructEnum {
        name: String,
        variant: usize,
        fields: Vec<(usize, HirExpr)>,
    },
    Field {
        base: Box<HirExpr>,
        index: usize,
    },
    Borrow {
        place: HirPlace,
        mutable: bool,
    },
    RawAddress {
        place: HirPlace,
    },
    RawOffset {
        pointer: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    RawBorrow {
        pointer: Box<HirExpr>,
        anchor: HirPlace,
    },
    RawSlice {
        pointer: Box<HirExpr>,
        length: Box<HirExpr>,
        anchor: HirPlace,
    },
    RawSliceLen(Box<HirExpr>),
    RawSliceAt {
        slice: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    ReferenceRead(Box<HirExpr>),
    ReferenceAssign {
        reference: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    RawLoad(Box<HirExpr>),
    RawStore {
        pointer: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    RawInit {
        pointer: Box<HirExpr>,
        value: Box<HirExpr>,
    },
    RawTake(Box<HirExpr>),
    Forget(Box<HirExpr>),
    RawTrap,
    RawAlloc {
        size: Box<HirExpr>,
        align: Box<HirExpr>,
    },
    RawDealloc {
        pointer: Box<HirExpr>,
        size: Box<HirExpr>,
        align: Box<HirExpr>,
    },
    LayoutQuery {
        queried: Ty,
        kind: LayoutQueryKind,
    },
    Block(Vec<HirStmt>, Option<Box<HirExpr>>),
    If {
        condition: Box<HirExpr>,
        then_branch: Box<HirExpr>,
        else_branch: Option<Box<HirExpr>>,
    },
    Return(Option<Box<HirExpr>>),
    While {
        condition: Box<HirExpr>,
        body: Box<HirExpr>,
        post_test: bool,
    },
    Loop {
        body: Box<HirExpr>,
    },
    Break(Option<Box<HirExpr>>),
    Continue,
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
}

#[derive(Debug, Clone)]
pub(super) enum HirIndex {
    Static(u64),
    Dynamic(Box<HirExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ParamSig {
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) mode: PassMode,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionSig {
    pub(super) groups: Vec<Vec<ParamSig>>,
    pub(super) unsafe_effect: bool,
    pub(super) throws_error: Option<Ty>,
    pub(super) custom_effects: Vec<String>,
    pub(super) result: Option<Ty>,
}

impl FunctionSig {
    pub(super) fn function_ty(&self) -> Option<Ty> {
        Some(Ty::Function(FunctionTy {
            groups: self
                .groups
                .iter()
                .map(|group| group.iter().map(|param| param.ty.clone()).collect())
                .collect(),
            unsafe_effect: self.unsafe_effect,
            throws_error: self.throws_error.clone().map(Box::new),
            custom_effects: self.custom_effects.clone(),
            result: Box::new(self.result.clone()?),
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum LocalCapability {
    Owned,
    SharedParam,
    MutParam,
}

pub(super) type LoanId = usize;

#[derive(Debug, Clone)]
pub(super) struct PartialInfo {
    pub(super) function: String,
    pub(super) consumed_groups: usize,
    pub(super) capture_count: usize,
    pub(super) is_fn_mut: bool,
    pub(super) is_fn_once: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ClosureInfo {
    pub(super) function: String,
    pub(super) groups: Vec<Vec<ParamSig>>,
    pub(super) unsafe_effect: bool,
    pub(super) throws_error: Option<Ty>,
    pub(super) custom_effects: Vec<String>,
    pub(super) result: Ty,
    pub(super) captures: Vec<ClosureCapture>,
    pub(super) capture_names: Vec<String>,
    pub(super) is_fn_mut: bool,
    pub(super) is_fn_once: bool,
}

#[derive(Clone, Default)]
pub(super) struct ClosureEffectContext {
    pub(super) unsafe_depth: usize,
    pub(super) throws_error: Option<Ty>,
    pub(super) custom_effects: HashSet<String>,
    pub(super) custom_effect_sources: HashMap<String, Type>,
    pub(super) lexical_handler_effects: HashSet<String>,
    pub(super) lexical_handler_effect_sources: HashMap<String, Type>,
    pub(super) infer_effects: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosureCaptureMode {
    Shared,
    Mutable,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosureCapturePolicy {
    Lexical,
    HandlerOwned,
    AsyncOwned,
}

#[derive(Debug, Clone)]
pub(super) struct ClosureCapture {
    pub(super) place: HirPlace,
    pub(super) mode: ClosureCaptureMode,
    pub(super) by_value: bool,
    pub(super) value: Option<Box<HirExpr>>,
    pub(super) forwarded: Option<ForwardedClosureCapture>,
}

#[derive(Debug, Clone)]
pub(super) struct ForwardedClosureCapture {
    pub(super) binding: LocalId,
    pub(super) index: usize,
    pub(super) callable_ty: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct ClosureCaptureUse {
    pub(super) name: String,
    pub(super) mode: ClosureCaptureMode,
}

#[derive(Debug, Clone)]
pub(super) struct AccessBoundary {
    pub(super) visibility: Visibility,
    pub(super) origin: ItemOrigin,
}
