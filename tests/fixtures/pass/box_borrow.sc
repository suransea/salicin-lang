let box = alloc.boxed.box

let resource = struct { value: i32 }

extend(resource) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let main(): i32 = {
  let mut boxed = box.new(resource { value: 10 })
  let first = do {
    let reference = boxed.as_ref()
    reference.read()
  }
  do {
    let reference = boxed.as_ref(mut)()
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

test("box_borrow.sc") {
  std.test.assert(main() == 42)
}
