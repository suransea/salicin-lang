let ask = effect {
  let value(): i32
  let stop(): i32
}

let run(action: with(ask)((i32): i32))(input: i32): i32 = {
  ask.handle value { (resume) -> resume(10) } stop { (resume) -> 40 } action {
      action(input)
    }
}

let left: with(ask)(input: i32): i32 = { ask.value() + input }
let right: with(ask)(input: i32): i32 = { ask.value() * 2 + input }
let abort: with(ask)(input: i32): i32 = { ask.stop() + input }
let select(order: borrow(mut)(i32)): bool = {
  order = order * 10 + 1
  false
}
let next_input(order: borrow(mut)(i32)): i32 = {
  order = order * 10 + 2
  2
}

let main(): i32 = {
  let chosen = left
  let left_runner = run(action: chosen)
  let mut order = 0
  let selected = run(action: if select(order) { left } else if true { right } else { abort })(next_input(order))
  let answer = left_runner(1) + selected + run(action: abort)(0) - 31
  if order == 12 { answer } else { 0 }
}

test("algebraic_effect_reusable_handler.sc") {
  main() == 42
}
