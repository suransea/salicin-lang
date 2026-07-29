let slice = core.memory.slice
let vec = alloc.vec.vec

let main(): i32 = {
  let source: array(i32)(2) = [1, 2]
  let source_view: borrow(slice(i32)) = borrow(source)
  let mut values: vec(i32) = vec(i32).new()
  values.push(3)
  values.copy_from(source_view)
  0
}
