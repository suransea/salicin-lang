let unsafety = core.unsafe.unsafety
let defer = core.control.defer

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

let set: with(unsafety)(counter: ptr(mut)(i32))(expected: i32, next: i32): () = {
  unsafe {
    if *counter == expected {
      *counter = next
    } else {
      *counter = 100
    }
  }
}

let increment: with(unsafety)(counter: ptr(mut)(i32)): () = {
  unsafe {
    *counter = *counter + 1
  }
}

let return_with_defer: with(unsafety)(counter: ptr(mut)(i32)): i32 = {
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

let main(): i32 = {
  unsafe {
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
  }
}

test("defer_control.sc") {
  std.test.assert(main() == 42)
}
