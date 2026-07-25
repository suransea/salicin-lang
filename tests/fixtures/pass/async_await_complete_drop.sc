let Poll = std.async.Poll
let Future = std.async.Future
let Unsafe = std.unsafe.Unsafe

let Step = struct { counter: Ptr(mut)(i32) }
let Resource = struct { counter: Ptr(mut)(i32) }

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

let consume(move resource: Resource): () = { () }

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(40)
  }
}

let allocate(): Ptr(mut)(i32) with(Unsafe) = { unsafe {
  raw_alloc(i32)(size_of(i32), align_of(i32))
} }

let release(counter: Ptr(mut)(i32)): () with(Unsafe) = { unsafe {
  raw_dealloc(counter, size_of(i32), align_of(i32))
} }

let main(): i32 = { unsafe {
  let counter = allocate()
  *counter = 0

  let value = do {
    let resource = Resource { counter: counter }
    let mut future = async {
      let value = await Step { counter: counter }
      consume(resource)
      value
    }
    match future.poll()
      { Pending -> 0 }
      { Ready(value) -> value }
  }

  let drops = *counter
  release(counter)
  value + drops
} }

test("async_await_complete_drop.sc") {
  main() == 42
}
