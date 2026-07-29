let slice = core.memory.slice

let main(): i32 = {
  let mut values: array(i32)(3) = [1, 2, 3]
  let view: borrow(mut)(slice(i32)) = borrow(mut)(values)
  view.copy_within(0, 2, 2)
  0
}
