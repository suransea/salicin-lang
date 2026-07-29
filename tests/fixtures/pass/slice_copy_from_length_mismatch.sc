let slice = core.memory.slice

let main(): i32 = {
  let source: array(i32)(2) = [1, 2]
  let mut destination: array(i32)(3) = [3, 4, 5]
  let source_view: borrow(slice(i32)) = borrow(source)
  let destination_view: borrow(mut)(slice(i32)) = borrow(mut)(destination)
  destination_view.copy_from(source_view)
  0
}
