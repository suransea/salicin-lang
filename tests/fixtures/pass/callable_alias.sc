let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let add(left: i32)(right: i32): i32 = { left + right }

let consume(move resource: Resource)(value: i32): i32 = {
  let observed = resource.value
  observed + value
}

let main(): i32 = {
  let named = add
  let add_forty = named(40)
  let moved_partial = add_forty

  let pending = consume(Resource { value: 1 })
  let moved_resource_partial = pending

  let base = 0
  let closure = { (value: i32) -> base + value }
  let moved_closure = closure

  moved_closure(moved_partial(moved_resource_partial(1)))
}
