let option = core.option
let slice = core.memory.slice
let inspect = effect {
  let accepted(value: i32): bool
}

let read(value: borrow(i32)): i32 = { value }

let effect_greater_than_ten(value: borrow(i32)): bool with(inspect) = {
  inspect.accepted(read(value))
}

let locate(values: borrow(slice(i32))): option(u64) with(inspect) = {
  values.position(effect_greater_than_ten)
}

let main(): i32 = { 42 }
