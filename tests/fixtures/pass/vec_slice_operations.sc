let slice = core.memory.slice
let vec = alloc.vec.vec

let main(): i32 = {
  let source: array(i32)(3) = [1, 2, 3]
  let source_view: borrow(slice(i32)) = borrow(source)
  let mut values: vec(i32) = vec(i32).new()
  values.extend_from_slice(source_view)
  if values.len() != 3 || values.read(0) != 1 || values.read(2) != 3 {
    return 1
  }

  let extra: array(i32)(2) = [4, 5]
  let extra_view: borrow(slice(i32)) = borrow(extra)
  values.extend_from_slice(extra_view)
  if values.len() != 5 || values.read(3) != 4 || values.read(4) != 5 {
    return 2
  }

  values.copy_within(0, 4, 1)
  if values.read(0) != 1 || values.read(1) != 1 ||
    values.read(2) != 2 || values.read(3) != 3 || values.read(4) != 4 {
    return 3
  }
  values.copy_within(1, 5, 0)
  if values.read(0) != 1 || values.read(1) != 2 ||
    values.read(2) != 3 || values.read(3) != 4 || values.read(4) != 4 {
    return 4
  }

  let replacement: array(i32)(5) = [8, 9, 10, 11, 12]
  let replacement_view: borrow(slice(i32)) = borrow(replacement)
  values.copy_from(replacement_view)
  if values.len() != 5 || values.read(0) != 8 || values.read(4) != 12 {
    return 5
  }
  values.fill(14)
  if values.len() != 5 ||
    values.read(0) + values.read(1) + values.read(2) +
    values.read(3) + values.read(4) != 70 {
    return 6
  }

  let empty_source: array(i32)(0) = []
  let empty_view: borrow(slice(i32)) = borrow(empty_source)
  let mut empty: vec(i32) = vec(i32).new()
  empty.extend_from_slice(empty_view)
  empty.fill(42)
  empty.copy_within(0, 0, 0)
  if empty.is_empty() {
    42
  } else {
    0
  }
}

test("vec_slice_operations.sc") {
  std.test.assert(main() == 42)
}
