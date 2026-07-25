# Coherent Generic Trait Methods

Status: implemented by `TYPE-TRAIT-METHOD-1`

## Goal

A trait method may declare its own compile-time parameter groups independently of the trait and of
an implementation header. Concrete implementations, blanket implementations, constructor
implementations, and default methods must all expose one coherent method contract and select one
statically known implementation.

This task does not introduce runtime dictionaries, open-world dispatch, specialization priority, or
overlapping implementations.

## Binder Equivalence

Method compile-time parameters are binders. Their spelling is not part of the contract:

```salicin
let Convert(From: type) = trait {
  let convert(To: type)(self: borrow(Self))(move value: From): To
}

extend Boxed: Convert(i32) {
  let convert(Result: type)(self: borrow(Self))(move value: i32): Result = { ... }
}
```

The declaration and implementation are compared after alpha-normalizing every method binder by
group and position. Group count, parameter count, kind, defaults, and dependencies remain exact.
References in runtime parameter types, result types, effects, where predicates, and bodies are
renamed consistently.

Trait, implementation-header, and method binders occupy separate scopes. Redeclaring an outer
binder in a method is rejected even when its kind matches.

## Contract Checking

An implementation method must satisfy the declaration after substituting:

1. trait compile-time arguments;
2. `Self`;
3. associated type and constructor bindings;
4. implementation-header binders;
5. alpha-normalized method binders.

The receiver form, runtime parameter groups, passing modes, result type, and complete effect row
must then match exactly.

Method-level where predicates are part of the trait contract. An implementation may rename their
bound method parameters, but it may not add a stronger predicate, remove a required predicate, or
change an associated equality. Predicate comparison uses the same bounded constructor-equation
normalization as ordinary where clauses.

## Instantiation

A selected implementation template concatenates compile-time groups in this order:

1. implementation-header groups;
2. method groups.

Concrete dispatch supplies or infers only the method groups after the implementation header has
already been selected from the receiver and trait reference. The ordinary generic-function
instance cache owns monomorphization and recursion limits.

Default methods use the same template path. Their bodies are substituted with the selected
`Self`, trait arguments, associated bindings, and method arguments before validation and lowering.

## Coherence

Method generics do not participate in implementation overlap. Coherence is decided solely by the
implementation target, trait reference, and implementation-header where predicates. Two impls
cannot become distinguishable only through method compile-time arguments.

Overload identity remains the runtime parameter-label shape already declared by the trait. A method
generic parameter name is never an overload discriminator.

## Diagnostics

Diagnostics name the source trait and method. They distinguish:

- compile-time group or kind mismatch;
- runtime signature mismatch after alpha normalization;
- strengthened or missing method predicate;
- underconstrained method argument inference;
- ambiguous trait implementation selection.

Generated validation and monomorphization names must not appear.

## Acceptance Evidence

`TYPE-TRAIT-METHOD-1` is complete when tests cover:

- concrete and blanket implementations with differently named method binders;
- explicit and inferred type, effect, `usize`, `access`, and region method arguments;
- method-level associated equalities;
- default generic methods;
- constructor-trait generic methods;
- rejection of kind/group mismatches, binder capture, strengthened predicates, overlap, and
  underconstrained calls;
- cross-module static dispatch and deterministic monomorphization.

Compiler and native tests cover concrete, blanket, constructor, default, and cross-module method
templates. Type, effect, `usize`, `access`, and erased region binders are alpha-normalized before
contract comparison. Method where predicates and associated equalities are retained as part of the
implementation contract.
