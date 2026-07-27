let resource = struct { value: i32 }
let wrapper = struct { resource: resource }
let choice = enum {
  some(wrapper),
  none,
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    self.value = 0
  }
}

let main(): i32 = {
  let value = choice.some(wrapper { resource: resource { value: 42 } })
  42
}

test("drop_glue.sc") {
  main() == 42
}
