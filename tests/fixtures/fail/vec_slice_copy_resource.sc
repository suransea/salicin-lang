let vec = alloc.vec.vec
let resource = struct { value: i32 }

let main(): i32 = {
  let source: array(resource)(1) = [resource { value: 42 }]
  let mut values: vec(resource) = vec(resource).new()
  values.extend_from_slice(borrow(source))
  0
}
