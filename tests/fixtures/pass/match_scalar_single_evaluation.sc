let next(counter: ptr(mut)(i32)): i32 = {
  unsafe {
    *counter = *counter + 1
    41
  }
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *counter = 0
  }
  let selected = match next(counter)
    { 40 -> 0 }
    { value if value == 41 -> value }
    { _ -> 0 }
  let evaluations = unsafe {
    *counter
  }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  selected + evaluations
}

test("match_scalar_single_evaluation.sc") {
  std.test.assert(main() == 42)
}
