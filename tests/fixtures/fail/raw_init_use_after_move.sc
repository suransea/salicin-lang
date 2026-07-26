let resource = struct { value: i32 }

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(resource)(size_of(resource), align_of(resource))
  }
  let resource = resource { value: 42 }
  unsafe {
    raw_init(pointer, resource)
  }
  resource.value
}
