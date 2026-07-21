let Stop = effect {
  let stop(): i32
}

let program(): i32 with(Stop) = {
  let value = Stop.stop()
  value + 1
}

let main(): i32 = {
  let result = Stop.handle(
    stop: { (resume) -> 40 },
  ) {
    program() + 1
  }
  result + 2
}
