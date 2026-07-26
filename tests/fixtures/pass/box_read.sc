let box = std.boxed.box

let read_box(comptime t: type)(boxed: borrow(box(t))): t
where t: copyable = { boxed.read() }

let main(): i32 = {
  let mut boxed = box.new(comptime t: i32)(0)
  boxed.write(20)
  let first = boxed.read()
  boxed.write(22)
  let second = boxed.read()
  let mut unit: box(()) = box.new(())
  let zero = box.new(comptime t: i32)(0)
  unit.write(())
  unit.write(())
  unit.read()
  first + second + read_box(zero)
}

test("box_read.sc") {
  main() == 42
}
