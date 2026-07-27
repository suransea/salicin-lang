let box = alloc.boxed.box

let resource = struct { value: i32 }

let main(): i32 = {
  let boxed = box.new(resource { value: 42 })
  let reference = boxed.as_ref()
  let value = boxed.into_inner()
  reference.value + value.value
}
