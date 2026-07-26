//! Typed intermediate forms for Salicin's erased, compile-time language.
//!
//! Source syntax deliberately uses ordinary declarations and calls for both
//! phases.  These types keep the compiler implementation from representing
//! static values as accidental runtime types, and give trait solving an
//! explicit goal vocabulary independent of the parser AST.

use crate::ast::{
    AssociatedTypeBinding, CompileParam, FunctionEffects, Sort, Type, WherePredicate,
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
    /// A decoded UTF-8 metadata literal. It has no runtime text representation.
    String(String),
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
            Self::String(_) => Sort::String,
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

impl From<&WherePredicate> for Constraint {
    fn from(predicate: &WherePredicate) -> Self {
        Self::Implements {
            subject: predicate.subject.clone(),
            trait_ref: predicate.trait_ref.clone(),
            projections: predicate
                .associated_types
                .iter()
                .map(ProjectionEquation::from)
                .collect(),
        }
    }
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
        assert_eq!(
            StaticValue::String("arithmetic".into()).sort(),
            Sort::String
        );
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
                name: "Array".into(),
                sort: constructor_sort.clone(),
            }
            .sort(),
            constructor_sort
        );
    }

    #[test]
    fn where_predicates_lower_to_trait_constraints_with_projection_equations() {
        let predicate = WherePredicate {
            subject: Type::Named("T".into(), Vec::new()),
            trait_ref: Type::Named("Iterator".into(), Vec::new()),
            associated_types: vec![AssociatedTypeBinding {
                name: "Item".into(),
                compile_groups: Vec::new(),
                ty: Type::I32,
            }],
        };
        assert!(matches!(
            Constraint::from(&predicate),
            Constraint::Implements {
                subject: Type::Named(subject, _),
                trait_ref: Type::Named(trait_name, _),
                projections,
            } if subject == "T"
                && trait_name == "Iterator"
                && projections[0].name == "Item"
                && projections[0].value == Type::I32
        ));
    }
}
