let optimization = sort {
  size
  speed
}

let select(comptime mode: optimization)(value: i32): i32 = { value }

let main(): i32 = { select(optimization.speed)(42) }

test("finite_sort_declaration.sc") {
  main() == 42
}
