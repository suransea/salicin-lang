let Poll = std.async.Poll
let Future = std.async.Future
let Unsafe = std.unsafe.Unsafe

let Resource = struct { counter: Ptr(mut)(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: Resource): () = { () }

let allocate(): Ptr(mut)(i32) with(Unsafe) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release(counter: Ptr(mut)(i32)): () with(Unsafe) = {
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
}

let main(): i32 = {
  unsafe {
    let counter = allocate()
    *counter = 0

    let resource = Resource { counter: counter }
    let mut future = async {
      consume(resource)
    }
    let result = future.poll()
    let ready = match result
      { Ready(_) -> 1 }
      { Pending -> 0 }

    let drops = *counter
    release(counter)
    40 + ready + drops
  }
}

test("async_ready_poll.sc") {
  main() == 42
}
