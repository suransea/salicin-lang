let Abort = effect {
  let stop(value: i32): Never
}

let fail(): Never with(Abort) = {
  Abort.stop(42)
}

let main(): i32 = {
  Abort.handle stop { (value) -> value } action {
    fail()
  }
}
