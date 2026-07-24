let Box = std.boxed.Box

let main(): i32 = {
  let mut boxed = Box.new(42)
  let mutable = boxed.as_ref(mut)()
  let shared = boxed.as_ref()
  mutable + shared
}
