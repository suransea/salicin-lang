let slice = core.memory.slice

let main(): i32 = {
  let mut values: array(i32)(3) = [1, 2, 3]
  let destination: borrow(mut)(slice(i32)) = borrow(mut)(values)
  let source: borrow(slice(i32)) = borrow(values)
  destination.copy_from(source)
  0
}
