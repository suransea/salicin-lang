let Box = std.boxed.Box

let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
  }}

let main(): i32 = {
  let boxed = Box.new(Resource { value: 1 })
  42
}
