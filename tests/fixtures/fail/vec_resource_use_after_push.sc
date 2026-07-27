let vec = std.vec.vec

let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {}}

let main(): i32 = {
  let mut values: vec(resource) = vec(resource).new()
  let resource = resource { value: 42 }
  values.push(resource)
  resource.value
}
