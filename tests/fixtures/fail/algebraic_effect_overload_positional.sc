let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let choose(): i32 with(ask) = {
  ask.value(42)
}

let main(): i32 = { 0 }
