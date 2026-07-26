let option = std.option
let result = std.result

let query = effect {
  let option(present: bool): option(bool)
  let result(present: bool): result(())(bool)
  let fallback(): bool
}

let program(): i32 with(query) = {
  let option_some = if query.option(true) ?? query.fallback() { 10 } else { 0 }
  let option_none = if query.option(false) ?? query.fallback() { 10 } else { 0 }
  let result_ok = if query.result(true) ?? query.fallback() { 10 } else { 0 }
  let result_err = if query.result(false) ?? query.fallback() { 10 } else { 0 }
  option_some + option_none + result_ok + result_err
}

let main(): i32 = {
  let mut fallbacks = 0
  let result = query.handle option { (present, resume) ->
      resume(if present { option.some(true) } else { option.none })
    } result { (present, resume) ->
      resume(if present { result.ok(true) } else { result.err(()) })
    } fallback { (resume) ->
      fallbacks += 1;
      resume(true)
    } action {
      program()
    }
  result + fallbacks
}

test("algebraic_effect_coalesce.sc") {
  main() == 42
}
