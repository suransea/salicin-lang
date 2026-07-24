let Box = std.boxed.Box

let main(): i32 = {
  let pointer = Box.new(42).into_raw()
  let rebuilt = Box(i32).from_raw(pointer)
  rebuilt.into_inner()
}
