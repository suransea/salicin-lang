let box = std.boxed.box

let main(): i32 = {
  let unit: box(()) = box.new(())
  let inner = box.new(comptime t: i32)(42)
  let outer = box.new(comptime t: box(i32))(inner)
  42
}

test("box_nested_and_unit.sc") {
  main() == 42
}
