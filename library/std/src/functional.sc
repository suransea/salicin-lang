/// Type constructors whose payload can be transformed.
pub let functor = trait(comptime self: (comptime value: type): type) {
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: self(a))
    (transform: (a): b with(e)): self(b) with(e)
}

/// Functors that can inject values and apply wrapped functions.
pub let applicative = trait(comptime self: (comptime value: type): type)
where self: functor {
  let pure(comptime a: type)(value: a): self(a)

  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: self((a): b with(e)))
    (value: self(a)): self(b) with(e)
}

/// Applicatives that can sequence dependent computations.
pub let monad = trait(comptime self: (comptime value: type): type)
where self: applicative {
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: self(a))
    (next: (a): self(b) with(e)): self(b) with(e)
}

/// Implements `functor` for `option`.
extend(core.option.option, functor) {
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.option(a))
    (transform: (a): b with(e)): core.option(b) with(e) = {
    match self
      { some(value) -> core.option.some(transform(value)) }
      { none -> core.option.none }
  }
}

/// Implements `applicative` for `option`.
extend(core.option.option, applicative) {
  let pure(comptime a: type)(value: a): core.option(a) = {
    core.option.some(value)
  }

  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.option((a): b with(e)))
    (value: core.option(a)): core.option(b) with(e) = {
    match self
      { some(transform) -> match value
        { some(value) -> core.option.some(transform(value)) }
        { none -> core.option.none } }
      { none -> core.option.none }
  }
}

/// Implements `monad` for `option`.
extend(core.option.option, monad) {
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.option(a))
    (next: (a): core.option(b) with(e)): core.option(b) with(e) = {
    match self
      { some(value) -> next(value) }
      { none -> core.option.none }
  }
}

/// Implements `functor` for `result(error)`.
extend(core.result.result(error), functor) {
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.result(error)(a))
    (transform: (a): b with(e)): core.result(error)(b) with(e) = {
    match self
      { ok(value) -> core.result.ok(transform(value)) }
      { err(error) -> core.result.err(error) }
  }
}

/// Implements `applicative` for `result(error)`.
extend(core.result.result(error), applicative) {
  let pure(comptime a: type)(value: a): core.result(error)(a) = {
    core.result.ok(value)
  }

  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.result(error)((a): b with(e)))
    (value: core.result(error)(a)): core.result(error)(b) with(e) = {
    match self
      { ok(transform) -> match value
        { ok(value) -> core.result.ok(transform(value)) }
        { err(error) -> core.result.err(error) } }
      { err(error) -> core.result.err(error) }
  }
}

/// Implements `monad` for `result(error)`.
extend(core.result.result(error), monad) {
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: core.result(error)(a))
    (next: (a): core.result(error)(b) with(e)): core.result(error)(b) with(e) = {
    match self
      { ok(value) -> next(value) }
      { err(error) -> core.result.err(error) }
  }
}
