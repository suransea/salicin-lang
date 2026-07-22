let Combine = trait {
  let combine(borrow self)(left: i32)(right: i32): i32
}

let Number = struct { value: i32 }

extend Number: Combine {
  let combine(borrow self)(left: i32, right: i32): i32 = { left + right }
}

let main(): i32 = { 0 }
