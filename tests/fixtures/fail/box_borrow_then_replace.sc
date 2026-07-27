let box = alloc.boxed.box

let resource = struct { value: i32 }

let main(): i32 = {
  let mut boxed = box.new(resource { value: 20 })
  let reference = boxed.as_ref()
  let previous = boxed.replace(resource { value: 22 })
  reference.value + previous.value
}
