let Resource = struct(value: i32)

let main(): i32 = {
  let resource = Resource(42)
  let pointer = Ptr(borrow resource)
  let copied = unsafe {
    *pointer
  }
  copied.value
}
