let resource = struct { value: i32 }

let main(): i32 = {
  let resource = resource { value: 42 }
  let pointer = ptr(borrow(resource))
  let copied = unsafe {
    *pointer
  }
  copied.value
}
