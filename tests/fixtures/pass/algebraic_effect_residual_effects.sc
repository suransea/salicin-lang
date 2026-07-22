let Ask = effect {
  let value(): i32 with(throws(bool), unsafe)
}

let request(): i32 with(Ask, throws(bool), unsafe) = {
  Ask.value()
}

let run(): i32 with(throws(bool)) = {
  unsafe {
    Ask.handle(value: { (resume) -> resume(42) }) {
      request()
    }
  }
}

let main(): i32 = {
  let result: Result(i32, bool) = try { run() }
  result ?? 0
}
