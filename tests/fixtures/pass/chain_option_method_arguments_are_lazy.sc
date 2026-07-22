use core.Option

let Adder = struct { base: i32 }

extend Adder {
  let add(move self)(value: i32): i32 = { self.base + value }
}

let side_effect(borrow(mut) count: i32): i32 = {
  count = count + 1
  1
}

let main(): i32 = {
  let mut count = 0
  let answer = Option(Adder).None?.add(side_effect(count)) ?? 42
  if count == 0 { answer } else { 0 }
}
