let box = alloc.boxed.box

let main(): i32 = {
  let mut boxed = box.new(40)
  let previous = boxed.replace(41)
  let pointer = boxed.into_raw()
  let observed = unsafe {
    *pointer
  }
  if observed != 41 {
    return(0)
  }
  let rebuilt = unsafe {
    box(i32).from_raw(pointer)
  }
  let current = rebuilt.into_inner()
  current - previous + 41
}

test("box_methods.sc") {
  main() == 42
}
