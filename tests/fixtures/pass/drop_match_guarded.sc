let Resource = struct { value: i32 }
let Choice = enum { Pair(Resource, Resource), None }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let guard_false(move choice: Choice): i32 = { choice match {
  Pair(left, _) if left.value == 0 => do {
    consume(left)
    0
  },
  Pair(left, _) => do {
    consume(left)
    21
  },
  None => 0
}
}

let guard_true(move choice: Choice): i32 = { choice match {
  Pair(left, _) if left.value == 1 => do {
    consume(left)
    21
  },
  Pair(left, _) => do {
    consume(left)
    0
  },
  None => 0
}
}

let guard_return(move choice: Choice): i32 = { choice match {
  Pair(left, _) if return 42 => 0,
  Pair(left, _) => do {
    consume(left)
    0
  },
  None => 0
}
}

let main(): i32 = {
  let first = guard_false(Choice.Pair(Resource { value: 1 }, Resource { value: 1 }))
  let second = guard_true(Choice.Pair(Resource { value: 1 }, Resource { value: 1 }))
  let third = guard_return(Choice.Pair(Resource { value: 1 }, Resource { value: 1 }))
  first + second + third - 42
}
