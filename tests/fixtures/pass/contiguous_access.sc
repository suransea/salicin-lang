let option = core.option
let vec = alloc.vec.vec

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let mut fixed = [10, 20, 12]
  let fixed_middle = do {
    match fixed.get(1)
      { option.some(value) -> read(value) }
      { option.none -> 0 }
  }
  let fixed_missing = do {
    match fixed.get(99)
      { option.some(value) -> read(value) }
      { option.none -> 10 }
  }
  let fixed_at = do {
    let value = fixed.at(0)
    read(value)
  }
  do {
    let view = fixed.as_slice(mut)()
    let first = view.at(mut)(0)
    first = 20
  }

  let fixed_view = fixed.as_slice()
  let slice_first = match fixed_view.first()
    { option.some(value) -> read(value) }
    { option.none -> 0 }
  let slice_last = match fixed_view.last()
    { option.some(value) -> read(value) }
    { option.none -> 0 }

  let mut dynamic = vec.new(t: i32)()
  let empty_before_push = dynamic.is_empty()
  let missing_before_push = do {
    match dynamic.first()
      { option.some(value) -> read(value) }
      { option.none -> 10 }
  }
  dynamic.push(10)
  dynamic.push(12)
  do {
    let last = dynamic.last(mut)()
    match last
      { option.some(value) -> value = 22 }
      { option.none -> () }
  }
  let dynamic_middle = do {
    match dynamic.get(1)
      { option.some(value) -> read(value) }
      { option.none -> 0 }
  }
  let dynamic_missing = do {
    match dynamic.get(99)
      { option.some(value) -> read(value) }
      { option.none -> 10 }
  }
  let dynamic_at = do {
    let value = dynamic.at(0)
    read(value)
  }

  let fixed_shape: bool = fixed.len() == 3 && !fixed.is_empty()
  let slice_access: bool = fixed_view.len() == 3 && !fixed_view.is_empty() && slice_first == 20 && slice_last == 12
  let dynamic_shape: bool = empty_before_push && missing_before_push == 10 && dynamic.len() == 2
  let dynamic_access: bool = dynamic_middle == 22 && dynamic_missing == 10 && dynamic_at == 10

  if !fixed_shape {
    1
  } else if fixed_middle != 20 {
    21
  } else if fixed_missing != 10 {
    22
  } else if fixed_at != 10 {
    fixed_at
  } else if !slice_access {
    3
  } else if !dynamic_shape {
    4
  } else if !dynamic_access {
    5
  } else {
    42
  }
}

test("contiguous_access.sc") {
  std.test.assert(main() == 42)
}
