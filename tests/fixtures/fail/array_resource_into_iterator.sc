let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { () }
}

let main(): i32 = {
  let values: Array(Resource)(1) = [Resource { value: 42 }]
  for values { value -> () }
  42
}
