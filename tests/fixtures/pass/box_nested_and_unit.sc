let Box = std.boxed.Box

let main(): i32 = {
  let unit: Box(()) = Box.new(())
  let inner = Box.new(T: i32)(42)
  let outer = Box.new(T: Box(i32))(inner)
  42
}

test("box_nested_and_unit.sc") {
  main() == 42
}
