let Poll = std.async.Poll
let Future = std.async.Future
let Unsafe = std.unsafe.Unsafe

let Step = struct { counter: Ptr(mut)(i32) }

extend Step: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(41)
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
    let mut future = async {
      await Step { counter: counter }
    }
    match future.poll()
      { Pending -> 0 }
      { Ready(value) -> value }
  }

  let drops = *counter
  release(counter)
  value + drops
} }
