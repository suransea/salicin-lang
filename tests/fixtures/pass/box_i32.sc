let Box = std.boxed.Box

let main(): i32 = {
  Box.new(42).read()
}

test("box_i32.sc") {
  main() == 42
}
