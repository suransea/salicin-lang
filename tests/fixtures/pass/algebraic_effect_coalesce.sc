use std.Option
use std.Result

let Query = effect {
  let option(present: bool): Option(bool)
  let result(present: bool): Result(())(bool)
  let fallback(): bool
}

let program(): i32 with(Query) = {
  let option_some = if Query.option(true) ?? Query.fallback() { 10 } else { 0 }
  let option_none = if Query.option(false) ?? Query.fallback() { 10 } else { 0 }
  let result_ok = if Query.result(true) ?? Query.fallback() { 10 } else { 0 }
  let result_err = if Query.result(false) ?? Query.fallback() { 10 } else { 0 }
  option_some + option_none + result_ok + result_err
}

let main(): i32 = {
  let mut fallbacks = 0
  let result = Query.handle(
    option: { (present, resume) ->
      resume(if present { Option.Some(true) } else { Option.None })
    },
    result: { (present, resume) ->
      resume(if present { Result.Ok(true) } else { Result.Err(()) })
    },
    fallback: { (resume) ->
      fallbacks += 1;
      resume(true)
    },
  ) {
    program()
  }
  result + fallbacks
}
