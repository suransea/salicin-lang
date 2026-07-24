let Box = std.boxed.Box

let main(): i32 = {
  let mut boxed = Box.new(40)
  let previous = boxed.replace(41)
  let pointer = boxed.as_mut_ptr()
  let observed = unsafe {
    *pointer
  }
  if observed != 41 {
    return(0)
  }
  let current = boxed.into_inner()
  current - previous + 41
}
