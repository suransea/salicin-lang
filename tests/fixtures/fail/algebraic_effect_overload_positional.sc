let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let choose(): i32 with(Ask) = {
  Ask.value(42)
}

let main(): i32 = { 0 }
