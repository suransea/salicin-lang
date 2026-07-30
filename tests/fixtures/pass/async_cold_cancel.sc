let unsafety = core.unsafe.unsafety

let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: resource): () = { () }

let relocate(comptime t: type)(move value: t): t
= requires(t is movable) {
  value
}

let allocate: with(unsafety)(): ptr(mut)(i32) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release: with(unsafety)(counter: ptr(mut)(i32)): () = {
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
}

let main(): i32 = {
  unsafe {
    let counter = allocate()
    *counter = 0

    do {
      let resource = resource { counter: counter }
      let future = async {
        consume(resource)
      }
      let moved = relocate(future)
      ()
    }

    let drops = *counter
    release(counter)
    41 + drops
  }
}

test("async_cold_cancel.sc") {
  std.test.assert(main() == 42)
}
