let Resource = struct { value: i32 }
let Choice = enum {
  Some(Resource),
  None,
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let conditional(flag: bool): () = {
  let value = Resource { value: 1 }
  if flag { consume(value) }
}

let inspect(move choice: Choice): i32 = { match choice
  { Some(_) -> 1 }
  { None -> 0 }
}

let early(): i32 = {
  let value = Resource { value: 1 }
  return(1)
}

let looped(): i32 = { loop {
  let value = Resource { value: 1 }
  break(1)
}
}

let main(): i32 = {
  do {
    let value = Resource { value: 1 }
  }
  consume(Resource { value: 1 })
  conditional(true)
  conditional(false)
  Resource { value: 1 }
  let mut replaced = Resource { value: 1 }
  replaced = Resource { value: 1 }
  early() + looped() + inspect(Choice.Some(Resource { value: 1 })) + 39
}
