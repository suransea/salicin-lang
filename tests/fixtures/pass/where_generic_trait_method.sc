let convert(comptime to: type) = trait {
  let convert(self: borrow(self))(): to
}

let value = struct { value: i32 }

extend(value, convert(i32)) {
  let convert(self: borrow(self))(): i32 = { self.value }
}

let convert(comptime t: type)(value: borrow(t)): i32
= requires(t is convert(i32)) { value.convert() }

let main(): i32 = {
  let value = value { value: 42 }
  convert(value)
}

test("where_generic_trait_method.sc") {
  main() == 42
}
