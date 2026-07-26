let box = std.boxed.box

let main(): i32 = {
  let pointer = box.new(42).into_raw()
  let rebuilt = box(i32).from_raw(pointer)
  rebuilt.into_inner()
}
