let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let add(left: i32)(right: i32): i32 = { left + right }

let consume(move resource: resource)(value: i32): i32 = {
  let observed = resource.value
  observed + value
}

let main(): i32 = {
  let named = add
  let add_forty = named(40)
  let moved_partial = add_forty

  let pending = consume(resource { value: 1 })
  let moved_resource_partial = pending

  let base = 0
  let closure = { (value: i32) -> base + value }
  let moved_closure = closure

  moved_closure(moved_partial(moved_resource_partial(1)))
}

test("callable_alias.sc") {
  std.test.assert(main() == 42)
}
