let Ask = effect {
  let value(): i32
}

let main(): i32 = {
  Ask.handle value { (resume) ->
    let escaped = resume
    42
  } action {
    Ask.value()
  }
}
