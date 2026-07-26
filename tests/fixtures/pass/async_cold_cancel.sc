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

let relocate(T: type)(move value: T): T
where T: Move = {
  value
}

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

    do {
      let resource = Resource { counter: counter }
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
