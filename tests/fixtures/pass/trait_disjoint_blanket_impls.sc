let convert(comptime to: type) = trait {
  let convert(self: borrow(self))(): to
}

let cell(comptime t: type) = struct { value: t }

extend(cell(t), convert(i32)) {
  let convert(self: borrow(self))(): i32 = { 42 }
}

extend(cell(t), convert(i64)) {
  let convert(self: borrow(self))(): i64 = { 42 }
}

let main(): i32 = {
  let cell = cell { value: true }
  42
}

test("trait_disjoint_blanket_impls.sc") {
  main() == 42
}
