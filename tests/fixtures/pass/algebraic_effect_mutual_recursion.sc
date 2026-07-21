let Tick = effect {
  let tick(): i32
}

let even(count: i32): i32 with(Tick) = {
  if count == 0 { return 0 }
  Tick.tick() + odd(count - 1)
}

let odd(count: i32): i32 with(Tick) = {
  if count == 0 { return 0 }
  Tick.tick() + even(count - 1)
}

let main(): i32 = {
  let value = 14
  Tick.handle(
    tick: { (resume) -> resume(value) },
  ) {
    even(3)
  }
}
