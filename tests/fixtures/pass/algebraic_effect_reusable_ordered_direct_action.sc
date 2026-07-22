let Ask = effect {
  let value(): i32
}

let run(seed: i32)(move action: (): i32 with(Ask)): i32 = {
  Ask.handle(value: { (resume) -> resume(20) }) {
    action() + seed
  }
}

let prepare(borrow(mut) order: i32): i32 = {
  order = order + 1
  20
}

let main(): i32 = {
  let mut order = 0
  run(prepare(order)) { () ->
    order = order * 2
    Ask.value() + order
  }
}
