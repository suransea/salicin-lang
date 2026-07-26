let abort = effect {
  let stop(value: i32): never
}

let main(): i32 = {
  abort.handle stop { (value, resume) -> resume(value) } action {
      abort.stop(42)
    }
}
