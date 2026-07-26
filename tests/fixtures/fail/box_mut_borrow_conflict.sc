let box = std.boxed.box

let main(): i32 = {
  let mut boxed = box.new(42)
  let mutable = boxed.as_ref(mut)()
  let shared = boxed.as_ref()
  mutable + shared
}
