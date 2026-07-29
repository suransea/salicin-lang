let ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let choose: with(ask)(): i32 = {
  ask.value(42)
}

let main(): i32 = { 0 }
