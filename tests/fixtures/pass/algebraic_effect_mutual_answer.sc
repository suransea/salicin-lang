let Tick = effect {
  let tick(): bool
}

let even(count: i32): bool with(Tick) = {
  if count == 0 { return true }
  if Tick.tick() { odd(count - 1) } else { false }
}

let odd(count: i32): bool with(Tick) = {
  if count == 0 { return false }
  if Tick.tick() { even(count - 1) } else { true }
}

let main(): i32 = {
  Tick.handle(
    tick: { (resume) -> resume(true) },
  ) {
    if odd(3) { 42 } else { 0 }
  }
}
