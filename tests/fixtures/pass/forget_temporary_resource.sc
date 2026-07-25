let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trap = 1 / self.value
  }}

let main(): i32 = {
  forget(Resource { value: 0 })
  42
}

test("forget_temporary_resource.sc") {
  main() == 42
}
