let option = core.option
let slice = core.memory.slice
let inspect = effect {
  let accepted(value: i32): bool
}

let read(value: borrow(i32)): i32 = { value }

let effect_greater_than_ten: with(inspect)(value: borrow(i32)): bool = {
  inspect.accepted(read(value))
}

let locate: with(inspect)(values: borrow(slice(i32))): option(u64) = {
  values.position(effect_greater_than_ten)
}

let main(): i32 = { 42 }
