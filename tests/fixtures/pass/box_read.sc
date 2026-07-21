use alloc.boxed.{Box, box_read, box_write}

let read_box(T: type)(borrow boxed: Box(T)): T
where T: Copy = { boxed.read() }

let main(): i32 = {
  let mut boxed = Box.new(0)
  boxed.write(20)
  let first = boxed.read()
  box_write(boxed)(22)
  let second = box_read(boxed)
  let mut unit = Box.new(())
  let zero = Box.new(0)
  unit.write(())
  box_write(unit)(())
  unit.read()
  first + second + read_box(zero)
}
