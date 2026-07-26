let Resource = struct { counter: Ptr(mut)(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let mut pair = (Resource { counter: counter }, Resource { counter: counter })
    consume(pair.0)
    pair.0 = Resource { counter: counter }
  }
  match (Resource { counter: counter }, Resource { counter: counter })
    { (left, right) ->
      consume(left)
      consume(right)
    }
  match (Resource { counter: counter }, Resource { counter: counter })
    { (left, _) if false -> consume(left) }
    { (left, right) ->
      consume(left)
      consume(right)
    }
  let drops = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  35 + drops
}

test("tuple_resource_drop.sc") {
  main() == 42
}
