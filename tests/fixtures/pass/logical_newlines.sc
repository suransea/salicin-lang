let add(
  left: i32,
  right: i32,
): i32 = {
  left +
    right
}

let main(): i32 = {
  let values = [
    40,
    2,
  ]
  add(
    values[
      0
    ],
    values[
      1
    ],
  )
}

test("logical_newlines.sc") {
  main() == 42
}
