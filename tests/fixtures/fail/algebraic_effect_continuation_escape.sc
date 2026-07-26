let ask = effect {
  let value(): i32
}

let main(): i32 = {
  ask.handle value { (resume) ->
      let escaped = resume
      42
    } action {
      ask.value()
    }
}
