// Compile-time sorts used by generic parameters and calling conventions.
/// Sort of compile-time type values.
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

pub let type: sort(2)
/// Sort of compile-time lifetime regions.
pub let region: sort(2)
/// Sort of individual compile-time effect identities.
pub let effect: sort(2)
/// Sort of normalized compile-time effect rows.
pub let effects: sort(2)
/// Sort of compile-time schemas expanded into one runtime parameter group.
pub let parameters: sort(2)
/// Sort of compiler-consumed UTF-8 metadata strings.
