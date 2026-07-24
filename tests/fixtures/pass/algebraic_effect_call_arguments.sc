let Step = effect {
  let next(value: i32): i32
}

let combine(left: i32, right: i32): i32 with(Step) = {
  left - right + 46
}

let main(): i32 = {
  Step.handle next { (value, resume) -> resume(value) } action {
    combine(Step.next(19), Step.next(23))
  }
}
