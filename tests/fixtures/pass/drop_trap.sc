let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }}

let main(): i32 = {
  let value = Resource { value: 0 }
  0
}
