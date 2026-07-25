let Unsafe = std.unsafe.Unsafe
let defer = std.control.defer

let allocate(): Ptr(mut)(i32) with(Unsafe) = { unsafe {
  raw_alloc(i32)(size_of(i32), align_of(i32))
} }

let release(counter: Ptr(mut)(i32)): () with(Unsafe) = { unsafe {
  raw_dealloc(counter, size_of(i32), align_of(i32))
} }

let set(counter: Ptr(mut)(i32))(expected: i32, next: i32): () with(Unsafe) = {
  unsafe {
    if *counter == expected {
      *counter = next
    } else {
      *counter = 100
    }
  }
}

let increment(counter: Ptr(mut)(i32)): () with(Unsafe) = {
  unsafe {
    *counter = *counter + 1
  }
}

let return_with_defer(counter: Ptr(mut)(i32)): i32 with(Unsafe) = {
  defer {
    unsafe {
      increment(counter)
    }
  }
  let value = unsafe {
    *counter
  }
  return(value)
}

let main(): i32 = { unsafe {
  let counter = allocate()
  *counter = 0

  do {
    defer {
      unsafe {
        set(counter)(4, 40)
      }
    }
    defer {
      unsafe {
        set(counter)(0, 4)
      }
    }
    ()
  }

  let mut iteration = 0
  loop {
    iteration = iteration + 1
    defer {
      unsafe {
        increment(counter)
      }
    }
    if iteration < 2 {
      continue()
    }
    break()
  }

  let value = return_with_defer(counter)
  let final = *counter
  release(counter)
  if final == 43 {
    value
  } else {
    0
  }
} }
