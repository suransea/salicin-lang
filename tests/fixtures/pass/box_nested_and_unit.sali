use alloc.boxed.box_new

let main(): i32 = {
  let unit = box_new(())
  let inner = box_new(42)
  let outer = box_new(inner)
  42
}
