let Box = std.boxed.Box
let box_new = std.boxed.box_new

let main(): i32 = {
  let unit: Box(()) = box_new(())
  let inner = box_new(T: i32)(42)
  let outer = box_new(T: Box(i32))(inner)
  42
}
