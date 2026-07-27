let rewrap = trait {
  let rewrap(move self)(): self
}

let cell(comptime t: type) = struct { value: t }

extend(cell(t)) {
  let wrap(move value: t): self = { self { value: value } }
  let replace(move self)(move value: t): self = { self { value: value } }
}

extend(cell(t), rewrap) {
  let rewrap(move self)(): self = { self { value: self.value } }
}

let main(): i32 = {
  let first = cell.wrap(20).value
  let cell = cell.wrap(0)
  let second = cell.replace(22).rewrap().value
  first + second
}

test("self_expression_generic.sc") {
  main() == 42
}
