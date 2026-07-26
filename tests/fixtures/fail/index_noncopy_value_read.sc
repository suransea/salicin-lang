let index = std.ops.index

let resource = struct { value: i32 }
extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {}
}

let bag = struct { value: resource }
extend bag: index(i32) {
  let output = resource
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: i32): borrow(a)(resource) = {
    borrow(a)(self.value)
  }
}

let main(): resource = {
  let bag = bag { value: resource { value: 42 } }
  bag[0]
}
