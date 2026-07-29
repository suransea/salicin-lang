let slice = core.memory.slice

let main(): i32 = {
  let values: array(i32)(1) = [42]
  let view: borrow(slice(i32)) = borrow(values)
  let pointer = raw_slice_ptr(view)
  0
}
