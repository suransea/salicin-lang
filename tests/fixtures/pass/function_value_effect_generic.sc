let Unsafe = std.effect.Unsafe

let increment(value: i32): i32 = { value + 1 }
let dangerous(value: i32): i32 with(Unsafe) = { value + 1 }

let apply(E: effect)
  (action: (i32): i32 with(E))
  (value: i32): i32 with(E) = { action(value) }

let main(): i32 = { apply(increment)(20) + unsafe {
  apply(dangerous)(20)
}
}
