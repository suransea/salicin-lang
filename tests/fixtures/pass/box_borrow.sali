use alloc.boxed.{Box, box_as_ref}

let Resource = struct(value: i32)

extend Resource {
  let read(borrow self)(): i32 = self.value
}

let main(): i32 = {
  let mut boxed = Box.new(Resource(10))
  let first = do {
    let reference = box_as_ref(boxed)
    reference.read()
  }
  do {
    let reference = box_as_ref(A: mut)(boxed)
    reference.value = 20
  }
  let second = do {
    let reference = boxed.as_ref()
    reference.read()
  }
  do {
    let reference = boxed.as_ref(mut)()
    reference.value = 22
  }
  let third = do {
    let reference = boxed.as_ref()
    reference.read()
  }
  first - 10 + second + third
}
