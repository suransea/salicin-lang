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
where t: movable = {
  value
}

let allocate(): ptr(mut)(i32) with(unsafety) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release(counter: ptr(mut)(i32)): () with(unsafety) = {
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
  main() == 42
}
