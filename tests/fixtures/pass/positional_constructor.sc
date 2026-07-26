let pair = struct { left: i32, right: i32 }
let pair(left: i32, right: i32): pair = { pair { left: left, right: right } }

let main(): i32 = {
  let pair = pair(40, 2)
  pair.left + pair.right
}

test("positional_constructor.sc") {
  main() == 42
}
