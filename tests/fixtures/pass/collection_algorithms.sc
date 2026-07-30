let option = core.option
let slice = core.memory.slice
let vec = alloc.vec.vec
let read(value: borrow(i32)): i32 = { value }

let greater_than_ten(value: borrow(i32)): bool = { read(value) > 10 }
let greater_than_seventeen(value: borrow(i32)): bool = { read(value) > 17 }
let greater_than_twenty(value: borrow(i32)): bool = { read(value) > 20 }
let greater_than_two(value: borrow(i32)): bool = { read(value) > 2 }
let greater_than_zero(value: borrow(i32)): bool = { read(value) > 0 }
let less_than_eighteen(value: borrow(i32)): bool = { read(value) < 18 }

let add(total: i32, value: borrow(i32)): i32 = {
  total + read(value)
}

let main(): i32 = {
  let values: array(i32)(4) = [3, 9, 12, 18]
  let view: borrow(slice(i32)) = borrow(values)

  let found_value = match view.find(greater_than_ten)
    { option.some(value) -> read(value) }
    { option.none -> 0 }
  let position = view.position(greater_than_ten)
  let position_value = position ?? 99
  if found_value != 12 || position_value != 2 {
    return 1
  }
  if !view.contains(9) || view.contains(7) {
    return 2
  }
  if !view.any(greater_than_seventeen) || view.any(greater_than_twenty) {
    return 3
  }
  if !view.all(greater_than_two) || view.all(less_than_eighteen) {
    return 4
  }
  if view.fold(0)(add) != 42 {
    return 5
  }

  let empty_values: array(i32)(0) = []
  let empty: borrow(slice(i32)) = borrow(empty_values)
  if empty.find(greater_than_zero).is_some() ||
    empty.position(greater_than_zero).is_some() ||
    empty.any(greater_than_zero) ||
    !empty.all(greater_than_zero) ||
    empty.contains(0) ||
    empty.fold(42)(add) != 42 {
    return 6
  }

  if values.position(greater_than_ten)!! != 2 ||
    values.find(greater_than_ten).is_none() ||
    !values.contains(12) ||
    values.fold(0)(add) != 42 {
    return 7
  }

  let mut dynamic: vec(i32) = vec(i32).new()
  dynamic.push(3)
  dynamic.push(9)
  dynamic.push(12)
  dynamic.push(18)
  if dynamic.position(greater_than_ten)!! != 2 ||
    dynamic.find(greater_than_ten).is_none() ||
    !dynamic.any(greater_than_seventeen) ||
    !dynamic.all(greater_than_two) ||
    !dynamic.contains(18) ||
    dynamic.fold(0)(add) != 42 {
    return 8
  }

  42
}

test("collection_algorithms.sc") {
  std.test.assert(main() == 42)
}
