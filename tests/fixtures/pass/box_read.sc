let Box = std.boxed.Box

let read_box(T: type)(boxed: borrow(Box(T))): T
where T: Copy = { boxed.read() }

let main(): i32 = {
  let mut boxed = Box.new(T: i32)(0)
  boxed.write(20)
  let first = boxed.read()
  boxed.write(22)
  let second = boxed.read()
  let mut unit: Box(()) = Box.new(())
  let zero = Box.new(T: i32)(0)
  unit.write(())
  unit.write(())
  unit.read()
  first + second + read_box(zero)
}

test("box_read.sc") {
  main() == 42
}
