let poll = core.async.poll
let future = core.async.future
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

    let resource = resource { counter: counter }
    let mut future = async {
      consume(resource)
    }
    let result = future.poll()
    let ready = match result
      { ready(_) -> 1 }
      { pending -> 0 }

    let drops = *counter
    release(counter)
    40 + ready + drops
  }
}

test("async_ready_poll.sc") {
  main() == 42
}
