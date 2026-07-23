let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let finish(move resource: Resource)(value: i32): i32 = {
  consume(resource)
  value
}

let finish_curried(move resource: Resource)(left: i32)(right: i32): i32 = {
  consume(resource)
  left + right
}

let invoke(): i32 = {
  let pending = finish(Resource { value: 1 })
  pending(42)
}

let continue_partial(): i32 = {
  let first = finish_curried(Resource { value: 1 })
  let second = first(20)
  second(22)
}

let abandon(): () = {
  let pending = finish(Resource { value: 1 })
}

let conditional(flag: bool): () = {
  let pending = finish(Resource { value: 1 })
  if flag { pending(0); }
}

let early(): i32 = {
  let pending = finish(Resource { value: 1 })
  pending(return 42)
}

let main(): i32 = {
  let first = invoke()
  let second = continue_partial()
  abandon()
  conditional(true)
  conditional(false)
  let third = early()
  first + second + third - 84
}
