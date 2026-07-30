let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  do {
    let mut pair = (resource { counter: counter }, resource { counter: counter })
    consume(pair.0)
    pair.0 = resource { counter: counter }
  }
  match (resource { counter: counter }, resource { counter: counter })
    { (left, right) ->
      consume(left)
      consume(right)
    }
  match (resource { counter: counter }, resource { counter: counter })
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
  std.test.assert(main() == 42)
}
