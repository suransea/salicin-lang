let read(value: borrow(i32)): i32 = { value }

let forward(comptime r: region)(value: borrow(r)(i32)): borrow(r)(i32) = { value }

let inferred_forward(value: borrow(i32)): borrow(i32) = { value }

let generic_read(comptime t: type)(value: borrow(t)): t
where t: copyable = { value }

let forward_mut(value: borrow(mut)(i32)): borrow(mut)(i32) = { value }

let write(value: borrow(mut)(i32))(replacement: i32): i32 = {
  value = replacement
  value
}

let main(): i32 = {
  let mut number = 20
  let before = do {
    let reference: borrow(i32) = do {
      let inner: borrow(i32) = borrow(number)
      inner
    }
    let forwarded = forward(reference)
    let inferred = inferred_forward(forwarded)
    read(inferred) + generic_read(reference) + generic_read(borrow(reference)) +
      generic_read(borrow(number)) - 60
  }
  let after = do {
    let reference: borrow(mut)(i32) = borrow(mut)(number)
    let forwarded = forward_mut(reference)
    write(forwarded)(22)
  }
  before + after
}

test("borrow_value_parameter.sc") {
  main() == 42
}
