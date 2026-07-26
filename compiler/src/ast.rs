use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
    /// Stable identity of the package whose target is being compiled.
    pub primary_package_identity: String,
    /// Definition-owner ID of the primary package in this resolved graph.
    pub primary_package: usize,
    /// Stable identities for every package represented by an item origin.
    pub package_identities: HashMap<usize, String>,
    /// Visibility is stored alongside top-level items until module lowering
    /// gives declarations stable module identities.
    pub item_visibilities: Vec<Visibility>,
    /// Source module provenance retained for semantic visibility checks that
    /// cannot be completed by syntactic path resolution (notably trait-method
    /// candidate lookup).
    pub item_origins: Vec<ItemOrigin>,
    pub uses: Vec<UseDecl>,
}

impl Program {
    pub fn new(items: Vec<Item>) -> Self {
        let item_visibilities = vec![Visibility::Private; items.len()];
        let item_origins = vec![ItemOrigin::default(); items.len()];
        Self {
            items,
            primary_package_identity: "source@0.0.0".to_owned(),
            primary_package: 0,
            package_identities: HashMap::from([(0, "source@0.0.0".to_owned())]),
            item_visibilities,
            item_origins,
            uses: Vec::new(),
        }
    }

    pub fn with_visibilities(items: Vec<Item>, item_visibilities: Vec<Visibility>) -> Self {
        Self::with_uses(items, item_visibilities, Vec::new())
    }

    pub fn with_uses(
        items: Vec<Item>,
        item_visibilities: Vec<Visibility>,
        uses: Vec<UseDecl>,
    ) -> Self {
        let item_origins = vec![ItemOrigin::default(); items.len()];
        Self::with_metadata(items, item_visibilities, item_origins, uses)
    }

    pub fn with_metadata(
        items: Vec<Item>,
        item_visibilities: Vec<Visibility>,
        item_origins: Vec<ItemOrigin>,
        uses: Vec<UseDecl>,
    ) -> Self {
        assert_eq!(
            items.len(),
            item_visibilities.len(),
            "every program item must have a visibility"
        );
        assert_eq!(
            items.len(),
            item_origins.len(),
            "every program item must have source provenance"
        );
        Self {
            items,
            primary_package_identity: "source@0.0.0".to_owned(),
            primary_package: 0,
            package_identities: HashMap::from([(0, "source@0.0.0".to_owned())]),
            item_visibilities,
            item_origins,
            uses,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ItemOrigin {
    pub package: usize,
    pub module_path: Vec<String>,
    pub source: Option<Box<SourceLocation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    pub path: Option<String>,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub visibility: Visibility,
    pub path: Vec<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Private,
    Package,
    Public,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    Global(Binding),
    Struct(StructDef),
    Enum(EnumDef),
    Effect(EffectDef),
    Sort(SortDef),
    TypeForm(TypeFormDef),
    TypeAlias(TypeAliasDef),
    Trait(TraitDef),
    Extend(ExtendDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub target: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub operations: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortDef {
    pub name: String,
    pub members: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFormDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub values: Vec<String>,
    pub builtin: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    pub name: String,
    pub self_parameter: CompileParam,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub where_predicates: Vec<WherePredicate>,
    pub members: Vec<TraitMember>,
}

pub fn default_trait_self_parameter() -> CompileParam {
    CompileParam {
        name: "self".to_owned(),
        kind: Sort::Type,
        default: None,
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum TraitMember {
    Function(Function),
    AssociatedType {
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
        kind: AssociatedKind,
        default: Option<Type>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociatedKind {
    Type,
    Parameters,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtendDef {
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub target: Type,
    pub trait_ref: Option<Type>,
    pub where_predicates: Vec<WherePredicate>,
    pub members: Vec<ExtendMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtendMember {
    Function(Function),
    Const(Binding),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub representation: StructRepresentation,
    pub derives: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StructRepresentation {
    #[default]
    Salicin,
    C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub name: String,
    pub fields: VariantFields,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFields {
    Unit,
    Positional(Vec<Type>),
    Named(Vec<Field>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub visibility: Visibility,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// Present only when the complete initializer is `foreign(c, ...)`.
    /// Foreign functions have no Salicin body and always require the `Unsafe`
    /// effect at call sites.
    pub foreign: Option<ForeignFunction>,
    /// True only when the complete source initializer is the core-private
    /// compiler definition marker `builtin()`.
    pub builtin: bool,
    /// Compile-time groups retain their source grouping but are erased before
    /// runtime calling convention lowering.
    pub compile_groups: Vec<Vec<CompileParam>>,
    /// Parameter groups are retained in the AST so later lowering can implement
    /// partial application without changing the parser.
    pub groups: Vec<Vec<Param>>,
    pub return_type: Option<Type>,
    pub effects: FunctionEffects,
    pub where_predicates: Vec<WherePredicate>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignFunction {
    pub abi: ForeignAbi,
    pub link_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignAbi {
    C,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct FunctionEffects {
    pub unsafe_effect: bool,
    /// Error type propagated automatically by calls and handled by `try { ... }`.
    pub throws: Option<Box<Type>>,
    /// Nominal user-defined marker effects, canonicalized by module lowering.
    pub custom: Vec<Type>,
    /// Compile-time effect-row parameters awaiting generic instantiation.
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WherePredicate {
    pub subject: Type,
    pub trait_ref: Type,
    pub associated_types: Vec<AssociatedTypeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeBinding {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompileParam {
    pub name: String,
    pub kind: Sort,
    pub default: Option<CompileParamDefault>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CompileParamDefault {
    Name(String),
    Region(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// The classifier of a compile-time value.
///
/// Unlike runtime [`Type`] values, sorts are erased before code generation.
/// Constructor sorts retain source parameter-group boundaries because
/// `(T: type)(L: usize): type` and `(T: type, L: usize): type` are distinct
/// compile-time calling conventions.
pub enum Sort {
    Type,
    USize,
    Region,
    /// An immutable UTF-8 metadata string erased before runtime lowering.
    String,
    /// A single nominal effect identity such as `Unsafe` or `Throws(Error)`.
    Effect,
    /// A normalized, order-insensitive row of zero or more effect identities.
    Effects,
    Parameters,
    /// A variadic pack of `parameters` schemas used as repeated runtime groups
    /// by compiler-validated control contracts such as `match`.
    ParameterPack,
    /// A compile-time parameter-schema transformer with the exact sort
    /// `(P: parameters): parameters`.
    ParameterModifier,
    TypeConstructor {
        parameter_groups: Vec<Vec<Sort>>,
    },
    EffectConstructor {
        parameter_groups: Vec<Vec<Sort>>,
    },
    /// A value whose compile-time type is a source-declared closed type.
    Named(String),
}

impl Sort {
    pub fn constructor_parameter_count(&self) -> Option<usize> {
        match self {
            Self::TypeConstructor { parameter_groups }
            | Self::EffectConstructor { parameter_groups } => {
                Some(parameter_groups.iter().map(Vec::len).sum())
            }
            _ => None,
        }
    }

    pub fn is_access(&self) -> bool {
        matches!(self, Self::Named(name) if name == "access")
    }

    pub fn is_effect_classifier(&self) -> bool {
        matches!(self, Self::Effect | Self::Effects)
    }

    pub fn is_parameter_modifier(&self) -> bool {
        matches!(self, Self::ParameterModifier)
    }
}

/// A pure expression admitted in a compile-time `usize` position.
///
/// This deliberately does not reuse [`Expr`]: the restricted tree cannot
/// contain mutation, borrowing, handlers, loops, or other runtime-only forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StaticExpr {
    USize(u64),
    Bool(bool),
    Name(String),
    Unary(UnaryOp, Box<StaticExpr>),
    Binary(Box<StaticExpr>, BinaryOp, Box<StaticExpr>),
    Call {
        function: String,
        arguments: Vec<StaticExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum USizeConst {
    Literal(u64),
    Parameter(String),
    Expression(Box<StaticExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub mode: PassMode,
    /// An access compile-time parameter used by `borrow(A)` until generic
    /// instantiation selects shared or mutable borrowing.
    pub access: Option<String>,
    /// Compile-time parameter-schema modifiers written before the parameter
    /// core. Instantiation normalizes them from right to left.
    pub modifiers: Vec<String>,
    pub region: Option<String>,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassMode {
    Inferred,
    Copy,
    Move,
    Borrow,
    MutBorrow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
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
    Tuple(Vec<Type>),
    Borrow {
        mutable: bool,
        access: Option<String>,
        region: Option<String>,
        pointee: Box<Type>,
    },
    Array(Box<Type>, u64),
    ArrayApplication {
        constructor: String,
        element: Box<Type>,
        length: USizeConst,
    },
    CompileUSize(u64),
    Function {
        groups: Vec<Vec<Type>>,
        effects: FunctionEffects,
        result: Box<Type>,
    },
    Named(String, Vec<Type>),
    /// A parsed named type application whose argument labels still need to be
    /// normalized against the constructor's compile-time parameter names.
    NamedArgs(String, Vec<TypeArg>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeArg {
    pub label: Option<String>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub mutable: bool,
    pub name: String,
    pub annotation: Option<Type>,
    pub value: Expr,
    /// Source range of a local initializer. This stays outside `Expr` so
    /// closure identity remains stable during handler lowering.
    pub value_source: Option<Box<SourceSpan>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(Binding),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub label: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerChainCall {
    pub scrutinee: Box<Expr>,
    pub payload: String,
    pub error: String,
    pub member: String,
    pub groups: Vec<Vec<CallArg>>,
    pub success: Box<Expr>,
    pub residual: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegerPattern {
    pub magnitude: u128,
    pub negative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Wildcard,
    Integer(IntegerPattern),
    Bool(bool),
    Binding(String),
    Tuple(Vec<Pattern>),
    Constructor {
        path: Vec<String>,
        fields: PatternFields,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternFields {
    Unit,
    Positional(Vec<Pattern>),
    Named(Vec<PatternField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternField {
    pub name: String,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Source position for an executable expression root. This is transparent
    /// to language semantics and is preserved through source rewrites.
    Located {
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        value: Box<Expr>,
    },
    Unit,
    Tuple(Vec<Expr>),
    /// Compiler-internal representation of a compile-time type argument after
    /// substitution. User source does not parse directly to this node.
    Type(Type),
    Integer(u128),
    Bool(bool),
    Name(String),
    Unary(UnaryOp, Box<Expr>),
    Borrow {
        mutable: bool,
        access: Option<String>,
        value: Box<Expr>,
    },
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Coalesce(Box<Expr>, Box<Expr>),
    /// Selective-CPS form of `??`; produced after parsing so the typed
    /// lowering can choose the `Option` or `Result` success pattern.
    HandlerCoalesce {
        scrutinee: Box<Expr>,
        payload: String,
        success: Box<Expr>,
        fallback: Box<Expr>,
    },
    /// Selective-CPS form of a fully applied optional method call. The
    /// typed lowering chooses `Option` or `Result` wrapping after the lazy
    /// success and residual branches have already been transformed.
    HandlerChainCall(Box<HandlerChainCall>),
    Try(Box<Expr>),
    DoBlock {
        body: Box<Expr>,
    },
    Async {
        body: Box<Expr>,
    },
    Await(Box<Expr>),
    Throw(Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    CompoundAssign(Box<Expr>, BinaryOp, Box<Expr>),
    Call(Box<Expr>, Vec<CallArg>),
    StructLiteral {
        constructor: Box<Expr>,
        fields: Vec<CallArg>,
    },
    Member(Box<Expr>, String),
    ChainMember(Box<Expr>, String),
    Array(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Block(Vec<Stmt>, Option<Box<Expr>>),
    Unsafe(Box<Expr>),
    Closure(Vec<Param>, Box<Expr>),
    PatternClosure {
        pattern: Pattern,
        guard: Option<Box<Expr>>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Return(Option<Box<Expr>>),
    While {
        condition: Box<Expr>,
        body: Box<Expr>,
        post_test: bool,
    },
    Loop {
        body: Box<Expr>,
    },
    Break(Option<Box<Expr>>),
    Continue,
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

impl Expr {
    pub fn unlocated(&self) -> &Self {
        match self {
            Self::Located { value, .. } => value.unlocated(),
            _ => self,
        }
    }

    pub fn unlocated_mut(&mut self) -> &mut Self {
        match self {
            Self::Located { value, .. } => value.unlocated_mut(),
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
    Deref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
