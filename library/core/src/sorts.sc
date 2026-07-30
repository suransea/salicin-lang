// Compile-time sorts used by generic parameters and calling conventions.
/// Constructs the universe at `level`. Universe levels start at one.
pub let sort(comptime level: usize): sort(level + 1) = builtin()

/// Returns the classifier of a compile-time value.
pub let sort_of(
  comptime level: usize,
  comptime classifier: sort(level),
  comptime value: classifier,
): sort(level) = builtin()

/// Returns the runtime type of an unevaluated expression.
pub let type_of(comptime t: type)
  (move expression: (): t): type = builtin()

/// Sort of compile-time type values.
pub let type: sort(2)
/// Sort of compile-time lifetime regions.
pub let region: sort(2)
/// Sort of individual compile-time effect identities.
pub let effect: sort(2)
/// Sort of normalized compile-time effect rows.
pub let effects: sort(2)
/// Sort of compile-time schemas expanded into one runtime parameter group.
pub let parameters: sort(2)
/// Sort of trait requirements consumed by compile-time constraint queries.
pub let constraint: sort(2)
/// Sort of parser-produced declarations consumed by syntax contracts.
pub let declaration: sort(2)

/// Compile-time relation between values classified by sorts.
pub let is(comptime right: sort(2)) = trait(comptime self: sort(2)) {
  let is(comptime left: self, comptime right: right): bool
}

extend(type, is(constraint)) {
  let is(
    comptime left: type,
    comptime right: constraint,
  ): bool = builtin()
}
