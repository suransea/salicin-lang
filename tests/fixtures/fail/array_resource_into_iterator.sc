let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = { () }
}

let main(): i32 = {
  let values: array(resource)(1) = [resource { value: 42 }]
  for values { value -> () }
  42
}
