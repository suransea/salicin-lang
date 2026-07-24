let Abort = effect {
  let stop(value: i32): Never
}

let main(): i32 = {
  Abort.handle stop { (value, resume) -> resume(value) } action {
    Abort.stop(42)
  }
}
