//! Typed intermediate forms for Salicin's erased, compile-time language.
//!
//! Source syntax deliberately uses ordinary declarations and calls for both
//! phases.  These types keep the compiler implementation from representing
//! static values as accidental runtime types, and give trait solving an
//! explicit goal vocabulary independent of the parser AST.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ast::{
    AssociatedTypeBinding, CompileParam, FunctionEffects, Sort, StaticFragmentKind, Type,
    WherePredicate,
};

/// A normalized value in the compile-time language.
///
/// `Symbolic` is used while checking a generic definition.  Concrete
/// instantiation must eliminate symbolic values before runtime lowering.
#[derive(Debug, Clone, PartialEq)]
pub enum StaticValue {
    Type(Type),
    USize(u64),
    Region(String),
    /// One effect identity, represented by a singleton normalized row while
    /// legacy monomorphization still uses marker-shaped source types.
    Effect(FunctionEffects),
    /// A normalized row containing zero or more effect identities.
    Effects(FunctionEffects),
    ParameterSchema(Vec<Vec<crate::ast::Param>>),
    TypeConstructor {
        name: String,
        sort: Sort,
    },
    EffectConstructor {
        name: String,
        sort: Sort,
    },
    Finite {
        sort: String,
        member: String,
    },
    Symbolic {
        name: String,
        sort: Sort,
    },
}

impl StaticValue {
    pub fn sort(&self) -> Sort {
        match self {
            Self::Type(_) => Sort::Type,
            Self::USize(_) => Sort::USize,
            Self::Region(_) => Sort::Region,
            Self::Effect(_) => Sort::Effect,
            Self::Effects(_) => Sort::Effects,
            Self::ParameterSchema(_) => Sort::Parameters,
            Self::TypeConstructor { sort, .. }
            | Self::EffectConstructor { sort, .. }
            | Self::Symbolic { sort, .. } => sort.clone(),
            Self::Finite { sort, .. } => Sort::Named(sort.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum StaticProducer {
    SyntaxElaborator,
    ReflectionEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticPhase {
    Elaboration,
    TypeChecking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticScopePolicy {
    /// The fragment must be consumed and normalized in the lexical operation
    /// that produced it. It can mention that operation's binders but cannot be
    /// stored, returned, or substituted into another context.
    ImmediateContext,
    /// A future contextual fragment must carry an explicit environment
    /// classifier before it may cross an elaboration boundary.
    ExplicitClassifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticEqualityPolicy {
    Opaque,
    NormalizedStructural,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticNormalForm(String);

impl StaticNormalForm {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn constraint_goal() -> Self {
        Self::new("constraint_goal")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticResourceLimits {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_bindings: usize,
}

/// One complete compiler-owned static-sort contract. Adding a future sort
/// requires registering all policies together; there are no permissive
/// defaults for scope, equality, normalization, producers, or resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticSortDescriptor {
    pub kind: StaticFragmentKind,
    pub universe_level: u64,
    pub source_constructible: bool,
    pub runtime_lowerable: bool,
    pub phase: StaticPhase,
    pub scope: StaticScopePolicy,
    pub equality: StaticEqualityPolicy,
    pub normal_form: StaticNormalForm,
    pub producers: BTreeSet<StaticProducer>,
    pub limits: StaticResourceLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticFragmentFacts {
    pub kind: StaticFragmentKind,
    pub producer: StaticProducer,
    pub phase: StaticPhase,
    pub node_count: usize,
    pub depth: usize,
    pub binding_count: usize,
    pub escapes_producer_context: bool,
    pub producer_classifier: Option<StaticEnvironmentClassifier>,
    pub consumer_classifier: Option<StaticEnvironmentClassifier>,
    pub runtime_lowering_requested: bool,
    pub comparison_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticEnvironmentClassifier {
    pub owner: String,
    pub bindings: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticSortError {
    InvalidDescriptor(String),
    Duplicate(StaticFragmentKind),
    Unknown(StaticFragmentKind),
    ProducerForbidden(StaticFragmentKind),
    WrongPhase(StaticFragmentKind),
    ScopeEscape(StaticFragmentKind),
    ScopeClassifierMismatch(StaticFragmentKind),
    RuntimeLowering(StaticFragmentKind),
    OpaqueComparison(StaticFragmentKind),
    ResourceLimit {
        kind: StaticFragmentKind,
        resource: &'static str,
        actual: usize,
        limit: usize,
    },
}

impl fmt::Display for StaticSortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDescriptor(message) => formatter.write_str(message),
            Self::Duplicate(kind) => write!(
                formatter,
                "static sort `{}` is already registered",
                kind.as_str()
            ),
            Self::Unknown(kind) => write!(
                formatter,
                "static sort `{}` is not registered",
                kind.as_str()
            ),
            Self::ProducerForbidden(kind) => write!(
                formatter,
                "producer is not permitted to construct `{}` fragments",
                kind.as_str()
            ),
            Self::WrongPhase(kind) => write!(
                formatter,
                "`{}` fragment crossed its declared compilation phase",
                kind.as_str()
            ),
            Self::ScopeEscape(kind) => write!(
                formatter,
                "`{}` fragment escaped its producing lexical context",
                kind.as_str()
            ),
            Self::ScopeClassifierMismatch(kind) => write!(
                formatter,
                "`{}` fragment producer and consumer environment classifiers do not match",
                kind.as_str()
            ),
            Self::RuntimeLowering(kind) => write!(
                formatter,
                "`{}` fragment cannot be lowered as a runtime value",
                kind.as_str()
            ),
            Self::OpaqueComparison(kind) => write!(
                formatter,
                "`{}` fragments have opaque equality and cannot be compared",
                kind.as_str()
            ),
            Self::ResourceLimit {
                kind,
                resource,
                actual,
                limit,
            } => write!(
                formatter,
                "`{}` fragment {resource} {actual} exceeds limit {limit}",
                kind.as_str()
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticSortModel {
    descriptors: BTreeMap<StaticFragmentKind, StaticSortDescriptor>,
}

impl StaticSortModel {
    pub fn edition_2026() -> Self {
        let mut model = Self::default();
        model
            .register(StaticSortDescriptor {
                kind: StaticFragmentKind::constraint(),
                universe_level: 2,
                source_constructible: false,
                runtime_lowerable: false,
                phase: StaticPhase::Elaboration,
                scope: StaticScopePolicy::ImmediateContext,
                equality: StaticEqualityPolicy::Opaque,
                normal_form: StaticNormalForm::constraint_goal(),
                producers: BTreeSet::from([StaticProducer::SyntaxElaborator]),
                limits: StaticResourceLimits {
                    max_nodes: 4_096,
                    max_depth: 128,
                    max_bindings: 512,
                },
            })
            .expect("the edition static-sort contract is valid");
        model
    }

    pub fn register(&mut self, descriptor: StaticSortDescriptor) -> Result<(), StaticSortError> {
        validate_descriptor(&descriptor)?;
        if self.descriptors.contains_key(&descriptor.kind) {
            return Err(StaticSortError::Duplicate(descriptor.kind));
        }
        self.descriptors.insert(descriptor.kind.clone(), descriptor);
        Ok(())
    }

    pub fn descriptor(
        &self,
        kind: &StaticFragmentKind,
    ) -> Result<&StaticSortDescriptor, StaticSortError> {
        self.descriptors
            .get(kind)
            .ok_or_else(|| StaticSortError::Unknown(kind.clone()))
    }

    pub fn fragment_kind(&self, name: &str) -> Option<StaticFragmentKind> {
        self.descriptors
            .keys()
            .find(|kind| kind.as_str() == name)
            .cloned()
    }

    pub fn validate_fragment(&self, facts: &StaticFragmentFacts) -> Result<(), StaticSortError> {
        let descriptor = self.descriptor(&facts.kind)?;
        if !descriptor.producers.contains(&facts.producer) {
            return Err(StaticSortError::ProducerForbidden(facts.kind.clone()));
        }
        if descriptor.phase != facts.phase {
            return Err(StaticSortError::WrongPhase(facts.kind.clone()));
        }
        if facts.escapes_producer_context && descriptor.scope == StaticScopePolicy::ImmediateContext
        {
            return Err(StaticSortError::ScopeEscape(facts.kind.clone()));
        }
        if descriptor.scope == StaticScopePolicy::ExplicitClassifier
            && (facts.producer_classifier.is_none()
                || facts.producer_classifier != facts.consumer_classifier)
        {
            return Err(StaticSortError::ScopeClassifierMismatch(facts.kind.clone()));
        }
        if facts.runtime_lowering_requested || descriptor.runtime_lowerable {
            return Err(StaticSortError::RuntimeLowering(facts.kind.clone()));
        }
        if facts.comparison_requested && descriptor.equality == StaticEqualityPolicy::Opaque {
            return Err(StaticSortError::OpaqueComparison(facts.kind.clone()));
        }
        validate_limit(
            descriptor,
            "node count",
            facts.node_count,
            descriptor.limits.max_nodes,
        )?;
        validate_limit(
            descriptor,
            "depth",
            facts.depth,
            descriptor.limits.max_depth,
        )?;
        validate_limit(
            descriptor,
            "binding count",
            facts.binding_count,
            descriptor.limits.max_bindings,
        )?;
        Ok(())
    }
}

fn validate_descriptor(descriptor: &StaticSortDescriptor) -> Result<(), StaticSortError> {
    let name = descriptor.kind.as_str();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        || descriptor.universe_level == 0
        || descriptor.normal_form.as_str().is_empty()
        || descriptor.source_constructible
        || descriptor.runtime_lowerable
        || descriptor.producers.is_empty()
        || descriptor.limits.max_nodes == 0
        || descriptor.limits.max_depth == 0
        || descriptor.limits.max_bindings == 0
    {
        return Err(StaticSortError::InvalidDescriptor(format!(
            "static sort `{name}` must be compiler-owned, erased, named in ASCII snake_case, and declare nonzero producer/resource contracts"
        )));
    }
    Ok(())
}

fn validate_limit(
    descriptor: &StaticSortDescriptor,
    resource: &'static str,
    actual: usize,
    limit: usize,
) -> Result<(), StaticSortError> {
    if actual > limit {
        return Err(StaticSortError::ResourceLimit {
            kind: descriptor.kind.clone(),
            resource,
            actual,
            limit,
        });
    }
    Ok(())
}

/// An associated-type equation attached to a trait constraint.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectionEquation {
    pub name: String,
    pub parameter_groups: Vec<Vec<CompileParam>>,
    pub value: Type,
}

impl From<&AssociatedTypeBinding> for ProjectionEquation {
    fn from(binding: &AssociatedTypeBinding) -> Self {
        Self {
            name: binding.name.clone(),
            parameter_groups: binding.compile_groups.clone(),
            value: binding.ty.clone(),
        }
    }
}

impl From<&ProjectionEquation> for AssociatedTypeBinding {
    fn from(equation: &ProjectionEquation) -> Self {
        Self {
            name: equation.name.clone(),
            compile_groups: equation.parameter_groups.clone(),
            ty: equation.value.clone(),
        }
    }
}

/// A logical proposition consumed by the trait solver.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Constraint {
    Implements {
        subject: Type,
        trait_ref: Type,
        projections: Vec<ProjectionEquation>,
    },
    Equal {
        sort: Sort,
        left: Type,
        right: Type,
    },
}

impl TryFrom<&WherePredicate> for Constraint {
    type Error = StaticSortError;

    fn try_from(predicate: &WherePredicate) -> Result<Self, Self::Error> {
        let (nodes, depth) = predicate_metrics(predicate);
        StaticSortModel::edition_2026().validate_fragment(&StaticFragmentFacts {
            kind: StaticFragmentKind::constraint(),
            producer: StaticProducer::SyntaxElaborator,
            phase: StaticPhase::Elaboration,
            node_count: nodes,
            depth,
            binding_count: predicate.associated_types.len(),
            escapes_producer_context: false,
            producer_classifier: None,
            consumer_classifier: None,
            runtime_lowering_requested: false,
            comparison_requested: false,
        })?;
        Ok(Self::Implements {
            subject: predicate.subject.clone(),
            trait_ref: predicate.trait_ref.clone(),
            projections: predicate
                .associated_types
                .iter()
                .map(ProjectionEquation::from)
                .collect(),
        })
    }
}

fn predicate_metrics(predicate: &WherePredicate) -> (usize, usize) {
    let mut nodes = 1usize;
    let mut depth = 1usize;
    for ty in std::iter::once(&predicate.subject)
        .chain(std::iter::once(&predicate.trait_ref))
        .chain(predicate.associated_types.iter().map(|binding| &binding.ty))
    {
        let (ty_nodes, ty_depth) = type_metrics(ty);
        nodes = nodes.saturating_add(ty_nodes);
        depth = depth.max(ty_depth.saturating_add(1));
    }
    (nodes, depth)
}

fn type_metrics(ty: &Type) -> (usize, usize) {
    let children = match ty {
        Type::Tuple(fields) | Type::Named(_, fields) => fields.iter().collect::<Vec<_>>(),
        Type::NamedArgs(_, arguments) => arguments.iter().map(|argument| &argument.ty).collect(),
        Type::Borrow { pointee, .. }
        | Type::Array(pointee, _)
        | Type::ArrayApplication {
            element: pointee, ..
        } => vec![pointee.as_ref()],
        Type::Function { groups, result, .. } => groups
            .iter()
            .flatten()
            .chain(std::iter::once(result.as_ref()))
            .collect(),
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => Vec::new(),
    };
    let mut nodes = 1usize;
    let mut depth = 1usize;
    for child in children {
        let (child_nodes, child_depth) = type_metrics(child);
        nodes = nodes.saturating_add(child_nodes);
        depth = depth.max(child_depth.saturating_add(1));
    }
    (nodes, depth)
}

/// A solver query under a set of assumed constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Goal {
    pub assumptions: Vec<Constraint>,
    pub conclusion: Constraint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalResult {
    Proven,
    NoSolution,
    Ambiguous,
}

impl Goal {
    pub fn new(assumptions: impl IntoIterator<Item = Constraint>, conclusion: Constraint) -> Self {
        Self {
            assumptions: assumptions.into_iter().collect(),
            conclusion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_values_report_their_sort_without_runtime_lowering() {
        assert_eq!(StaticValue::USize(4).sort(), Sort::USize);
        assert_eq!(StaticValue::USize(10).sort(), Sort::USize);
        assert_eq!(
            StaticValue::Finite {
                sort: "optimization".into(),
                member: "speed".into(),
            }
            .sort(),
            Sort::Named("optimization".into())
        );
        let constructor_sort = Sort::TypeConstructor {
            parameter_groups: vec![vec![Sort::Type], vec![Sort::USize]],
        };
        assert_eq!(
            StaticValue::TypeConstructor {
                name: "array".into(),
                sort: constructor_sort.clone(),
            }
            .sort(),
            constructor_sort
        );
    }

    #[test]
    fn static_sort_model_registers_complete_contracts_without_a_declaration_placeholder() {
        let model = StaticSortModel::edition_2026();
        let descriptor = model.descriptor(&StaticFragmentKind::constraint()).unwrap();
        assert_eq!(descriptor.scope, StaticScopePolicy::ImmediateContext);
        assert_eq!(descriptor.equality, StaticEqualityPolicy::Opaque);
        assert_eq!(descriptor.normal_form, StaticNormalForm::constraint_goal());
        assert!(!descriptor.source_constructible);
        assert!(!descriptor.runtime_lowerable);
        assert!(model
            .descriptor(&StaticFragmentKind::new("declaration"))
            .is_err());

        let mut extended = model.clone();
        let mut pattern = descriptor.clone();
        pattern.kind = StaticFragmentKind::new("pattern");
        pattern.phase = StaticPhase::TypeChecking;
        pattern.scope = StaticScopePolicy::ExplicitClassifier;
        pattern.equality = StaticEqualityPolicy::NormalizedStructural;
        pattern.normal_form = StaticNormalForm::new("pattern_ir");
        pattern.producers = BTreeSet::from([StaticProducer::ReflectionEngine]);
        extended.register(pattern.clone()).unwrap();
        assert_eq!(extended.descriptor(&pattern.kind).unwrap(), &pattern);
        let classifier = StaticEnvironmentClassifier {
            owner: "module::function".into(),
            bindings: BTreeSet::from(["t".into()]),
        };
        let mut contextual = StaticFragmentFacts {
            kind: pattern.kind.clone(),
            producer: StaticProducer::ReflectionEngine,
            phase: StaticPhase::TypeChecking,
            node_count: 1,
            depth: 1,
            binding_count: 1,
            escapes_producer_context: true,
            producer_classifier: Some(classifier.clone()),
            consumer_classifier: Some(classifier),
            runtime_lowering_requested: false,
            comparison_requested: false,
        };
        extended.validate_fragment(&contextual).unwrap();
        contextual.consumer_classifier = None;
        assert!(matches!(
            extended.validate_fragment(&contextual),
            Err(StaticSortError::ScopeClassifierMismatch(_))
        ));
        assert!(matches!(
            extended.register(pattern),
            Err(StaticSortError::Duplicate(_))
        ));
    }

    #[test]
    fn fragment_validation_rejects_wrong_producer_phase_scope_equality_and_budget() {
        let model = StaticSortModel::edition_2026();
        let valid = StaticFragmentFacts {
            kind: StaticFragmentKind::constraint(),
            producer: StaticProducer::SyntaxElaborator,
            phase: StaticPhase::Elaboration,
            node_count: 10,
            depth: 3,
            binding_count: 2,
            escapes_producer_context: false,
            producer_classifier: None,
            consumer_classifier: None,
            runtime_lowering_requested: false,
            comparison_requested: false,
        };
        model.validate_fragment(&valid).unwrap();
        for (facts, expected) in [
            (
                {
                    let mut facts = valid.clone();
                    facts.producer = StaticProducer::ReflectionEngine;
                    facts
                },
                "producer",
            ),
            (
                {
                    let mut facts = valid.clone();
                    facts.phase = StaticPhase::TypeChecking;
                    facts
                },
                "phase",
            ),
            (
                {
                    let mut facts = valid.clone();
                    facts.escapes_producer_context = true;
                    facts
                },
                "escaped",
            ),
            (
                {
                    let mut facts = valid.clone();
                    facts.comparison_requested = true;
                    facts
                },
                "opaque",
            ),
            (
                {
                    let mut facts = valid.clone();
                    facts.node_count = 4_097;
                    facts
                },
                "exceeds",
            ),
        ] {
            let error = model.validate_fragment(&facts).unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn where_predicates_lower_to_trait_constraints_with_projection_equations() {
        let predicate = WherePredicate {
            subject: Type::Named("t".into(), Vec::new()),
            trait_ref: Type::Named("iterator".into(), Vec::new()),
            associated_types: vec![AssociatedTypeBinding {
                name: "item".into(),
                compile_groups: Vec::new(),
                ty: Type::I32,
            }],
        };
        assert!(matches!(
            Constraint::try_from(&predicate).unwrap(),
            Constraint::Implements {
                subject: Type::Named(subject, _),
                trait_ref: Type::Named(trait_name, _),
                projections,
            } if subject == "t"
                && trait_name == "iterator"
                && projections[0].name == "item"
                && projections[0].value == Type::I32
        ));
    }

    #[test]
    fn constraint_normalization_enforces_structural_depth_before_solver_use() {
        let mut subject = Type::I32;
        for _ in 0..129 {
            subject = Type::Tuple(vec![subject]);
        }
        let predicate = WherePredicate {
            subject,
            trait_ref: Type::Named("copy".into(), Vec::new()),
            associated_types: Vec::new(),
        };
        assert!(matches!(
            Constraint::try_from(&predicate),
            Err(StaticSortError::ResourceLimit {
                resource: "depth",
                ..
            })
        ));
    }
}
