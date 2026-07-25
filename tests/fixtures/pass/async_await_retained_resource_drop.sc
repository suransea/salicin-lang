let Poll = std.async.Poll
let Future = std.async.Future
let Unsafe = std.unsafe.Unsafe

let Resource = struct {
  counter: Ptr(mut)(i32),
  value: i32
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

let Step = struct {
  counter: Ptr(mut)(i32),
  polled: bool
}

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
    if self.polled {
      Poll(i32).Ready(0)
    } else {
      self.polled = true
      Poll(i32).Pending
    }
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
  let mut future = async {
    let resource = Resource { counter: counter, value: 40 }
    let awaited = await Step { counter: counter, polled: false }
    resource.value + awaited
  }
  let pending = match future.poll()
    { Pending -> 0 }
    { Ready(_) -> 100 }
  let result = match future.poll()
    { Ready(value) -> value }
    { Pending -> 100 }

  do {
    let mut cancelled = async {
      let resource = Resource { counter: counter, value: 0 }
      let awaited = await Step { counter: counter, polled: false }
      resource.value + awaited
    }
    match cancelled.poll()
      { Pending -> () }
      { Ready(_) -> () }
  }

  let drops = *counter
  release(counter)
  pending + result + drops - 2
} }
