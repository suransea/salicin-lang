let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move resource: resource): i32 = { resource.value }

let make() = {
  let resource = resource { value: 1 }
  let closure = { (value: i32) -> consume(resource) + value }
  closure
}

let main(): i32 = {
  let closure = make()
  closure(41)
}

test("closure_resource_return.sc") {
  main() == 42
}
