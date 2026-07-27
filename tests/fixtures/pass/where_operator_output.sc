let add = core.ops.add

let twice(comptime t: type)(copy value: t): t
where t: add(t, output = t),
  t: copyable = {
  let left = value
  let right = value
  left + right
}

let main(): i32 = { twice(21) }

test("where_operator_output.sc") {
  main() == 42
}
