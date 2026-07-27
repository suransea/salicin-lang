let box = alloc.boxed.box

let main(): i32 = {
  let unit: box(()) = box.new(())
  let inner = box.new(t: i32)(42)
  let outer = box.new(t: box(i32))(inner)
  42
}

test("box_nested_and_unit.sc") {
  main() == 42
}
