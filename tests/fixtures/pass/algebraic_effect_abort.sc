let Abort = effect {
  let stop(): i32
}

let main(): i32 = {
  let mut reached = 0
  let result = Abort.handle stop { (resume) -> 42 } action {
    let value = Abort.stop()
    reached = 1;
    value
  }
  result + reached
}
