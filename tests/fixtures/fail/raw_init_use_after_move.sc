let Resource = struct { value: i32 }

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(Resource)(size_of(Resource), align_of(Resource))
  }
  let resource = Resource { value: 42 }
  unsafe {
    raw_init(pointer, resource)
  }
  resource.value
}
