let box_new = std.boxed.box_new
let box_ptr = std.boxed.box_ptr

let main(): i32 = {
  let boxed = box_new(42)
  let pointer = box_ptr(boxed)
  unsafe {
    *pointer
  }
}
