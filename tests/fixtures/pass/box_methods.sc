let Box = std.boxed.Box

let main(): i32 = {
  let mut boxed = Box.new(40)
  let previous = boxed.replace(41)
  let pointer = boxed.into_raw()
  let observed = unsafe {
    *pointer
  }
  if observed != 41 {
    return(0)
  }
  let rebuilt = unsafe {
    Box(i32).from_raw(pointer)
  }
  let current = rebuilt.into_inner()
  current - previous + 41
}
