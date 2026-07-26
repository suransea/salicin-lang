let resource = struct { value: i32 }
let wrapper = struct { resource: resource, value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let escape(): i32 = {
  let wrapper = wrapper { resource: resource { value: 0 }, value: return(42) }
  0
}

let main(): i32 = { escape() }

test("drop_partial_exit.sc") {
  main() == 42
}
