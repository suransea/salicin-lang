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

let main(): i32 = {
  let chosen = left
  let left_runner = run(action: chosen)
  left_runner(1) + run(action: right)(2) + run(action: abort)(0) - 31
}
