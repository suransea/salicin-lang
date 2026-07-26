let combine = trait {
  let combine(self: borrow(self))(left: i32)(right: i32): i32
}

let number = struct { value: i32 }

extend number: combine {
  let combine(self: borrow(self))(left: i32, right: i32): i32 = { left + right }
}

let main(): i32 = { 0 }
