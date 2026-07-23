let Ask = effect {
  let value(): i32
  let stop(): i32
}

let run(action: (i32): i32 with(Ask))(input: i32): i32 = {
  Ask.handle(
    value: { (resume) -> resume(10) },
    stop: { (resume) -> 40 },
  ) {
    action(input)
  }
}

let left(input: i32): i32 with(Ask) = { Ask.value() + input }
let right(input: i32): i32 with(Ask) = { Ask.value() * 2 + input }
let abort(input: i32): i32 with(Ask) = { Ask.stop() + input }
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
