let pair = struct { left: i32, right: i32 }

let main(): i32 = {
  let pair = pair { left: 40, right: 2 }
  pair.left + pair.right
}

test("struct_fields.sc") {
  main() == 42
}
