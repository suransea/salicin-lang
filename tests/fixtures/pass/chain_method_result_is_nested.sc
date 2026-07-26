let option = std.option

let boxed = struct { value: i32 }

extend boxed {
  let optional(move self)(): option(i32) = { option(i32).some(self.value) }
}

let main(): i32 = {
  let nested = option(boxed).some(boxed { value: 42 })?.optional()
  match nested
    { some(inner) -> inner ?? 0 }
    { none -> 0 }
}

test("chain_method_result_is_nested.sc") {
  main() == 42
}
