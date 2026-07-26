let pair = struct {
  left: i32,
  right: i32,
}

let select(value: pair): i32 = {
  match value
    { pair(right: right, left: 40) -> right }
    { _ -> 0 }
}

let main(): i32 = {
  select(pair { right: 42, left: 40 })
}

test("struct_match.sc") {
  main() == 42
}
