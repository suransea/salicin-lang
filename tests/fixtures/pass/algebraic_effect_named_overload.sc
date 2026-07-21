let Ask = effect {
  let value(left: i32): i32
  let value(right: i32): i32
}

let choose(): i32 with(Ask) = {
  Ask.value(left: 19) + Ask.value(right: 23)
}

let main(): i32 = {
  Ask.handle(
    value: { (left, resume) -> resume(left) },
    value: { (right, resume) -> resume(right) }
  ) {
    choose()
  }
}
