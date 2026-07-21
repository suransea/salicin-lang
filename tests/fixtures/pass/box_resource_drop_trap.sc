use alloc.boxed.box_new

let Resource = struct(value: i32)

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }
}

let main(): i32 = {
  let boxed = box_new(Resource(0))
  0
}
