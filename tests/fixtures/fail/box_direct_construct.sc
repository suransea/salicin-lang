let Box = std.boxed.Box

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  let boxed = Box(i32) { pointer: pointer }
  0
}
