use std::collections::{HashMap, HashSet};

use crate::ast::{
    CompileParam, CompileParamKind, EnumDef, ExtendMember, Function, Item, ItemOrigin, PassMode,
    StructDef, Type,
};

use super::hir::{AccessBoundary, EnumLayout, StructLayout, Ty};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct FunctionInstanceKey {
    pub(super) template: String,
    pub(super) arguments: Vec<Ty>,
}

#[derive(Debug, Clone)]
pub(super) struct FunctionInstanceInfo {
    pub(super) key: FunctionInstanceKey,
    pub(super) canonical: String,
}

pub(super) const MAX_FUNCTION_INSTANCES: usize = 256;
pub(super) const MAX_NOMINAL_INSTANCES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum NominalKind {
    Struct,
    Enum,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum TopLevelNamespace {
    Function,
    Type,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct NominalInstanceKey {
    pub(super) kind: NominalKind,
    pub(super) template: String,
    pub(super) arguments: Vec<Ty>,
}

#[derive(Debug, Clone)]
pub(super) struct NominalInstanceInfo {
    pub(super) key: NominalInstanceKey,
    pub(super) canonical: String,
}

#[derive(Clone)]
pub(super) struct GenericInherentExtension {
    pub(super) target_arguments: Vec<String>,
    pub(super) where_predicates: Vec<crate::ast::WherePredicate>,
    pub(super) members: Vec<ExtendMember>,
    pub(super) access: AccessBoundary,
    pub(super) origin: ItemOrigin,
}

#[derive(Clone)]
pub(super) struct GenericTraitExtension {
    pub(super) target_arguments: Vec<String>,
    pub(super) trait_ref: Type,
    pub(super) where_predicates: Vec<crate::ast::WherePredicate>,
    pub(super) members: Vec<ExtendMember>,
    pub(super) origin: ItemOrigin,
}

#[derive(Clone)]
pub(super) struct GenericConstructorTraitExtensionTarget {
    pub(super) target: TypeConstructorImplTarget,
    pub(super) self_constructor: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImplTypePattern {
    Variable(u8, usize),
    I32,
    I64,
    U32,
    U64,
    Bool,
    Unit,
    Array(Box<ImplTypePattern>, u64),
    Named(String, Vec<ImplTypePattern>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NominalInstanceState {
    Building,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolutionState {
    Resolving,
    Resolved,
}

#[derive(Clone, Default)]
pub(super) struct InherentMemberSet {
    pub(super) methods: HashMap<String, String>,
    pub(super) functions: HashMap<String, String>,
    pub(super) constants: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TraitRefKey {
    pub(super) name: String,
    pub(super) arguments: Vec<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TraitImplKey {
    pub(super) self_ty: Ty,
    pub(super) trait_ref: TraitRefKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TypeConstructorImplTarget {
    pub(super) name: String,
    pub(super) kind: NominalKind,
    pub(super) parameter_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ConstructorTraitRefKey {
    pub(super) name: String,
    pub(super) arguments: Vec<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ConstructorTraitImplKey {
    pub(super) target: TypeConstructorImplTarget,
    pub(super) trait_ref: ConstructorTraitRefKey,
}

#[derive(Debug, Clone)]
pub(super) struct TraitImplInfo {
    pub(super) key: TraitImplKey,
    pub(super) associated_types: HashMap<String, Ty>,
    pub(super) associated_type_sources: HashMap<String, Type>,
    pub(super) methods: HashMap<String, String>,
    pub(super) access: AccessBoundary,
}

#[derive(Debug, Clone)]
pub(super) struct TraitSchema {
    pub(super) self_parameter: CompileParam,
    pub(super) compile_parameters: Vec<CompileParam>,
    pub(super) where_predicates: Vec<crate::ast::WherePredicate>,
    pub(super) associated_types: Vec<String>,
    pub(super) associated_type_kinds: HashMap<String, CompileParamKind>,
    pub(super) methods: HashMap<String, Function>,
    pub(super) method_overloads: HashMap<String, Vec<String>>,
    pub(super) method_order: Vec<String>,
    pub(super) access: AccessBoundary,
    pub(super) valid: bool,
}

pub(super) fn schema_function_has_receiver(function: &Function) -> bool {
    function
        .groups
        .first()
        .is_some_and(|group| group.len() == 1 && group[0].name == "self")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FunctionShape {
    pub(super) groups: Vec<Vec<(PassMode, Ty)>>,
    pub(super) result: Ty,
    pub(super) effects: crate::ast::FunctionEffects,
}

pub(super) type ParameterLabelShape = Vec<Vec<String>>;
pub(super) type InherentOverloadKey = (String, String, bool);

pub(super) fn function_parameter_labels(function: &Function) -> ParameterLabelShape {
    function
        .groups
        .iter()
        .map(|group| {
            group
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect()
        })
        .collect()
}

pub(super) fn display_parameter_label_shape(groups: &ParameterLabelShape) -> String {
    groups
        .iter()
        .map(|group| format!("({})", group.join(", ")))
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn top_level_namespace(item: &Item) -> TopLevelNamespace {
    match item {
        Item::Function(_) => TopLevelNamespace::Function,
        Item::Struct(_) | Item::Enum(_) | Item::TypeAlias(_) => TopLevelNamespace::Type,
        Item::Global(_) | Item::Trait(_) | Item::Effect(_) | Item::Domain(_) | Item::Extend(_) => {
            TopLevelNamespace::Other
        }
    }
}

pub(super) fn overloaded_function_name(base: &str, groups: &ParameterLabelShape) -> String {
    let shape = groups
        .iter()
        .map(|group| format!("{}${}", group.len(), group.join("$")))
        .collect::<Vec<_>>()
        .join("$$");
    format!("{base}$overload${shape}")
}

pub(super) fn trait_method_identity(schema: &TraitSchema, function: &Function) -> Option<String> {
    if let Some(overloads) = schema.method_overloads.get(&function.name) {
        let identity =
            overloaded_function_name(&function.name, &function_parameter_labels(function));
        overloads.contains(&identity).then_some(identity)
    } else {
        schema
            .methods
            .contains_key(&function.name)
            .then(|| function.name.clone())
    }
}

#[derive(Clone)]
pub(super) struct NominalSnapshot {
    pub(super) struct_defs: HashMap<String, StructDef>,
    pub(super) enum_defs: HashMap<String, EnumDef>,
    pub(super) struct_layouts: HashMap<String, StructLayout>,
    pub(super) enum_layouts: HashMap<String, EnumLayout>,
    pub(super) nominal_accesses: HashMap<String, AccessBoundary>,
    pub(super) struct_order: Vec<String>,
    pub(super) enum_order: Vec<String>,
    pub(super) instance_names: HashMap<NominalInstanceKey, String>,
    pub(super) instances: HashMap<String, NominalInstanceInfo>,
    pub(super) states: HashMap<NominalInstanceKey, NominalInstanceState>,
    pub(super) invalid_recursive_nominals: HashSet<String>,
}
