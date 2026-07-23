use std::collections::{HashMap, HashSet};

use crate::ast::{
    CompileParam, CompileParamKind, EnumDef, ExtendMember, Function, Item, ItemOrigin, PassMode,
    StructDef, Type,
};
use crate::core::LangItemKind;

use super::hir::{AccessBoundary, EnumLayout, StructLayout, Ty};
use super::source_rewrite::substitute_type_parameters;
use super::Analyzer;

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

impl Analyzer {
    pub(super) fn trait_method_candidates(
        &self,
        receiver: &Ty,
        member: &str,
        origin: &ItemOrigin,
    ) -> Vec<TraitImplKey> {
        self.trait_methods_by_receiver
            .get(&(receiver.clone(), member.to_owned()))
            .into_iter()
            .flatten()
            .filter(|candidate| {
                self.trait_impls
                    .get(*candidate)
                    .is_some_and(|implementation| {
                        Self::access_boundary_allows(origin, &implementation.access)
                    })
            })
            .cloned()
            .collect()
    }

    pub(super) fn trait_associated_function_candidates(
        &self,
        target: &Ty,
        member: &str,
        origin: &ItemOrigin,
    ) -> Vec<String> {
        let mut candidates = self
            .trait_impls
            .values()
            .filter_map(|implementation| {
                if implementation.key.self_ty != *target
                    || !Self::access_boundary_allows(origin, &implementation.access)
                {
                    return None;
                }
                let schema = self.traits.get(&implementation.key.trait_ref.name)?;
                let method_ids = schema
                    .method_overloads
                    .get(member)
                    .cloned()
                    .unwrap_or_else(|| vec![member.to_owned()]);
                Some(
                    method_ids
                        .into_iter()
                        .filter(|method_id| {
                            schema
                                .methods
                                .get(method_id)
                                .is_some_and(|function| !schema_function_has_receiver(function))
                        })
                        .filter_map(|method_id| implementation.methods.get(&method_id).cloned())
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect::<Vec<_>>();
        candidates.sort();
        candidates
    }

    pub(super) fn constructor_trait_associated_function_candidates(
        &self,
        target: &str,
        member: &str,
        origin: &ItemOrigin,
    ) -> Vec<String> {
        let mut candidates = self
            .constructor_trait_impl_methods
            .iter()
            .filter_map(|(key, methods)| {
                if key.target.name != target {
                    return None;
                }
                let schema = self.traits.get(&key.trait_ref.name)?;
                let method_ids = schema
                    .method_overloads
                    .get(member)
                    .cloned()
                    .unwrap_or_else(|| vec![member.to_owned()]);
                Some(
                    method_ids
                        .into_iter()
                        .filter(|method_id| {
                            schema
                                .methods
                                .get(method_id)
                                .is_some_and(|function| !schema_function_has_receiver(function))
                        })
                        .filter_map(|method_id| methods.get(&method_id).cloned())
                        .filter(|canonical| {
                            self.function_accesses
                                .get(canonical)
                                .is_some_and(|access| Self::access_boundary_allows(origin, access))
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect::<Vec<_>>();
        candidates.sort();
        candidates
    }

    pub(super) fn trait_method_function_candidates(
        &self,
        receiver: &Ty,
        member: &str,
        origin: &ItemOrigin,
    ) -> Vec<(TraitImplKey, String)> {
        self.trait_method_candidates(receiver, member, origin)
            .into_iter()
            .flat_map(|key| {
                let implementation = &self.trait_impls[&key];
                let schema = &self.traits[&key.trait_ref.name];
                let method_ids = schema
                    .method_overloads
                    .get(member)
                    .cloned()
                    .unwrap_or_else(|| vec![member.to_owned()]);
                method_ids
                    .into_iter()
                    .filter_map(|method_id| {
                        implementation
                            .methods
                            .get(&method_id)
                            .cloned()
                            .map(|canonical| (key.clone(), canonical))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub(super) fn constructor_trait_method_function_candidates(
        &self,
        receiver: &Ty,
        member: &str,
        origin: &ItemOrigin,
    ) -> Vec<(ConstructorTraitImplKey, String)> {
        let (receiver_name, receiver_kind) = match receiver {
            Ty::Struct(name) => (name, NominalKind::Struct),
            Ty::Enum(name) => (name, NominalKind::Enum),
            _ => return Vec::new(),
        };
        let Some(instance) = self.nominal_instances.get(receiver_name) else {
            return Vec::new();
        };
        let mut candidates = self
            .constructor_trait_impl_methods
            .iter()
            .filter_map(|(key, methods)| {
                if key.target.name != instance.key.template || key.target.kind != receiver_kind {
                    return None;
                }
                let schema = self.traits.get(&key.trait_ref.name)?;
                let method_ids = schema
                    .method_overloads
                    .get(member)
                    .cloned()
                    .unwrap_or_else(|| vec![member.to_owned()]);
                Some(
                    method_ids
                        .into_iter()
                        .filter(|method_id| {
                            schema
                                .methods
                                .get(method_id)
                                .is_some_and(schema_function_has_receiver)
                        })
                        .filter_map(|method_id| methods.get(&method_id).cloned())
                        .filter(|canonical| {
                            self.function_accesses
                                .get(canonical)
                                .is_some_and(|access| Self::access_boundary_allows(origin, access))
                        })
                        .map(|canonical| (key.clone(), canonical))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect::<Vec<_>>();
        candidates.sort_by(|(_, left), (_, right)| left.cmp(right));
        candidates
    }

    pub(super) fn has_inaccessible_constructor_trait_associated_function(
        &self,
        target: &str,
        member: &str,
        origin: &ItemOrigin,
    ) -> bool {
        self.constructor_trait_impl_methods
            .iter()
            .any(|(key, methods)| {
                if key.target.name != target {
                    return false;
                }
                let Some(schema) = self.traits.get(&key.trait_ref.name) else {
                    return false;
                };
                let method_ids = schema
                    .method_overloads
                    .get(member)
                    .cloned()
                    .unwrap_or_else(|| vec![member.to_owned()]);
                method_ids.into_iter().any(|method_id| {
                    schema
                        .methods
                        .get(&method_id)
                        .is_some_and(|function| !schema_function_has_receiver(function))
                        && methods.get(&method_id).is_some_and(|canonical| {
                            self.function_accesses
                                .get(canonical)
                                .is_some_and(|access| !Self::access_boundary_allows(origin, access))
                        })
                })
            })
    }

    pub(super) fn is_drop_impl(&self, key: &TraitImplKey) -> bool {
        key.trait_ref.name == self.lang_item_name(LangItemKind::Drop)
            && key.trait_ref.arguments.is_empty()
    }

    pub(super) fn has_inaccessible_trait_method(
        &self,
        receiver: &Ty,
        member: &str,
        origin: &ItemOrigin,
    ) -> bool {
        self.trait_methods_by_receiver
            .get(&(receiver.clone(), member.to_owned()))
            .is_some_and(|candidates| {
                candidates.iter().any(|candidate| {
                    self.trait_impls
                        .get(candidate)
                        .is_some_and(|implementation| {
                            !Self::access_boundary_allows(origin, &implementation.access)
                        })
                })
            })
    }
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
        Item::Struct(_) | Item::Enum(_) | Item::TypeAlias(_) | Item::TypeForm(_) => {
            TopLevelNamespace::Type
        }
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

pub(super) fn collect_nominal_type_dependencies(
    ty: &Type,
    nominal_names: &HashSet<String>,
    bound: &HashSet<&str>,
    output: &mut Vec<String>,
) {
    match ty {
        Type::Borrow { pointee, .. } => {
            collect_nominal_type_dependencies(pointee, nominal_names, bound, output)
        }
        Type::Array(element, _) => {
            collect_nominal_type_dependencies(element, nominal_names, bound, output)
        }
        Type::Function { groups, result, .. } => {
            for ty in groups.iter().flatten() {
                collect_nominal_type_dependencies(ty, nominal_names, bound, output);
            }
            collect_nominal_type_dependencies(result, nominal_names, bound, output);
        }
        Type::Named(name, _) if !bound.contains(name.as_str()) && nominal_names.contains(name) => {
            if !output.contains(name) {
                output.push(name.clone());
            }
        }
        Type::NamedArgs(name, arguments)
            if !bound.contains(name.as_str()) && nominal_names.contains(name) =>
        {
            if !output.contains(name) {
                output.push(name.clone());
            }
            for argument in arguments {
                collect_nominal_type_dependencies(&argument.ty, nominal_names, bound, output);
            }
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                collect_nominal_type_dependencies(&argument.ty, nominal_names, bound, output);
            }
        }
        Type::I32
        | Type::I64
        | Type::U32
        | Type::U64
        | Type::Bool
        | Type::Unit
        | Type::Named(_, _) => {}
    }
}

pub(super) fn trait_reference_patterns_overlap(
    left: &GenericTraitExtension,
    right: &GenericTraitExtension,
) -> bool {
    let (Type::Named(left_name, left_arguments), Type::Named(right_name, right_arguments)) =
        (&left.trait_ref, &right.trait_ref)
    else {
        return false;
    };
    if left_name != right_name || left_arguments.len() != right_arguments.len() {
        return false;
    }
    let left_variables = left
        .target_arguments
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<HashMap<_, _>>();
    let right_variables = right
        .target_arguments
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<HashMap<_, _>>();
    let equations = left_arguments
        .iter()
        .zip(right_arguments)
        .map(|(left, right)| {
            (
                impl_type_pattern(left, &left_variables, 0),
                impl_type_pattern(right, &right_variables, 1),
            )
        });
    type_patterns_unify(equations)
}

pub(super) fn source_function_shapes_match(expected: &Function, actual: &Function) -> bool {
    expected.groups.len() == actual.groups.len()
        && expected
            .groups
            .iter()
            .zip(&actual.groups)
            .all(|(left, right)| {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| left.mode == right.mode && left.ty == right.ty)
            })
        && expected.return_type == actual.return_type
        && expected.effects == actual.effects
}

pub(super) fn generic_trait_pattern_overlaps_concrete(
    generic: &GenericTraitExtension,
    concrete: &TraitRefKey,
    concrete_arguments: &[Type],
    concrete_target_arguments: &[Type],
) -> bool {
    let Type::Named(name, generic_arguments) = &generic.trait_ref else {
        return false;
    };
    if name != &concrete.name || generic_arguments.len() != concrete_arguments.len() {
        return false;
    }
    let substitutions = generic
        .target_arguments
        .iter()
        .cloned()
        .zip(concrete_target_arguments.iter().cloned())
        .collect::<HashMap<_, _>>();
    let equations = generic_arguments
        .iter()
        .zip(concrete_arguments)
        .map(|(generic, concrete)| {
            let mut generic = generic.clone();
            substitute_type_parameters(&mut generic, &substitutions);
            (
                impl_type_pattern(&generic, &HashMap::new(), 0),
                impl_type_pattern(concrete, &HashMap::new(), 1),
            )
        });
    type_patterns_unify(equations)
}

fn impl_type_pattern(
    source: &Type,
    variables: &HashMap<String, usize>,
    side: u8,
) -> ImplTypePattern {
    match source {
        Type::I32 => ImplTypePattern::I32,
        Type::I64 => ImplTypePattern::I64,
        Type::U32 => ImplTypePattern::U32,
        Type::U64 => ImplTypePattern::U64,
        Type::Bool => ImplTypePattern::Bool,
        Type::Unit => ImplTypePattern::Unit,
        Type::Borrow {
            mutable, pointee, ..
        } => ImplTypePattern::Named(
            if *mutable { "$mut_borrow" } else { "$borrow" }.to_owned(),
            vec![impl_type_pattern(pointee, variables, side)],
        ),
        Type::Array(element, length) => ImplTypePattern::Array(
            Box::new(impl_type_pattern(element, variables, side)),
            *length,
        ),
        Type::Function {
            groups,
            effects,
            result,
        } => {
            let mut arguments = groups
                .iter()
                .flatten()
                .map(|ty| impl_type_pattern(ty, variables, side))
                .collect::<Vec<_>>();
            arguments.push(impl_type_pattern(result, variables, side));
            ImplTypePattern::Named(
                format!(
                    "$function${}${}",
                    groups
                        .iter()
                        .map(Vec::len)
                        .map(|length| length.to_string())
                        .collect::<Vec<_>>()
                        .join("_"),
                    if effects.unsafe_effect {
                        "$unsafe"
                    } else {
                        "$pure"
                    }
                ),
                arguments,
            )
        }
        Type::Named(name, arguments) if arguments.is_empty() && variables.contains_key(name) => {
            ImplTypePattern::Variable(side, variables[name])
        }
        Type::Named(name, arguments) => ImplTypePattern::Named(
            name.clone(),
            arguments
                .iter()
                .map(|argument| impl_type_pattern(argument, variables, side))
                .collect(),
        ),
        Type::NamedArgs(name, arguments) => ImplTypePattern::Named(
            name.clone(),
            arguments
                .iter()
                .map(|argument| impl_type_pattern(&argument.ty, variables, side))
                .collect(),
        ),
    }
}

fn type_patterns_unify(
    equations: impl IntoIterator<Item = (ImplTypePattern, ImplTypePattern)>,
) -> bool {
    let mut pending = equations.into_iter().collect::<Vec<_>>();
    let mut substitutions = HashMap::<(u8, usize), ImplTypePattern>::new();
    while let Some((left, right)) = pending.pop() {
        let left = resolve_impl_pattern(left, &substitutions);
        let right = resolve_impl_pattern(right, &substitutions);
        if left == right {
            continue;
        }
        match (left, right) {
            (ImplTypePattern::Variable(side, index), term)
            | (term, ImplTypePattern::Variable(side, index)) => {
                if impl_pattern_contains_variable(&term, (side, index), &substitutions) {
                    return false;
                }
                substitutions.insert((side, index), term);
            }
            (
                ImplTypePattern::Array(left, left_length),
                ImplTypePattern::Array(right, right_length),
            ) if left_length == right_length => {
                pending.push((*left, *right));
            }
            (
                ImplTypePattern::Named(left, left_arguments),
                ImplTypePattern::Named(right, right_arguments),
            ) if left == right && left_arguments.len() == right_arguments.len() => {
                pending.extend(left_arguments.into_iter().zip(right_arguments));
            }
            _ => return false,
        }
    }
    true
}

fn resolve_impl_pattern(
    pattern: ImplTypePattern,
    substitutions: &HashMap<(u8, usize), ImplTypePattern>,
) -> ImplTypePattern {
    match pattern {
        ImplTypePattern::Variable(side, index) => substitutions
            .get(&(side, index))
            .cloned()
            .map(|replacement| resolve_impl_pattern(replacement, substitutions))
            .unwrap_or(ImplTypePattern::Variable(side, index)),
        pattern => pattern,
    }
}

fn impl_pattern_contains_variable(
    pattern: &ImplTypePattern,
    variable: (u8, usize),
    substitutions: &HashMap<(u8, usize), ImplTypePattern>,
) -> bool {
    match pattern {
        ImplTypePattern::Variable(side, index) => {
            let current = (*side, *index);
            current == variable
                || substitutions.get(&current).is_some_and(|replacement| {
                    impl_pattern_contains_variable(replacement, variable, substitutions)
                })
        }
        ImplTypePattern::Array(element, _) => {
            impl_pattern_contains_variable(element, variable, substitutions)
        }
        ImplTypePattern::Named(_, arguments) => arguments
            .iter()
            .any(|argument| impl_pattern_contains_variable(argument, variable, substitutions)),
        ImplTypePattern::I32
        | ImplTypePattern::I64
        | ImplTypePattern::U32
        | ImplTypePattern::U64
        | ImplTypePattern::Bool
        | ImplTypePattern::Unit => false,
    }
}

pub(super) fn substitute_self_type(ty: &mut Type, target: &str) {
    match ty {
        Type::Borrow { pointee, .. } => substitute_self_type(pointee, target),
        Type::Array(element, _) => substitute_self_type(element, target),
        Type::Function { groups, result, .. } => {
            for ty in groups.iter_mut().flatten() {
                substitute_self_type(ty, target);
            }
            substitute_self_type(result, target);
        }
        Type::Named(name, arguments) if name == "Self" && arguments.is_empty() => {
            *ty = Type::Named(target.to_owned(), Vec::new());
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                substitute_self_type(argument, target);
            }
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                substitute_self_type(&mut argument.ty, target);
            }
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
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
