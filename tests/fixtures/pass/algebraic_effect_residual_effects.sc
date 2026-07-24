let Result = std.Result

let Throws = std.effect.Throws
let Unsafe = std.effect.Unsafe

let Supply = effect {
  let seed(): i32
}

let Ask = effect {
  let value(): i32 with(Supply, Throws(bool), Unsafe)
}

let request(): i32 with(Ask, Supply, Throws(bool), Unsafe) = {
  Ask.value()
}

let run(): i32 with(Supply, Throws(bool)) = {
  unsafe {
    Ask.handle value { (resume) -> resume(42) } action {
      request()
    }
  }
}

let main(): i32 = {
  let result: Result(bool)(i32) = try {
    Supply.handle seed { (resume) -> resume(0) } action { run() }
  }
  result ?? 0
}
