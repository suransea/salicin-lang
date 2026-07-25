let Slice = std.Slice

let Resource = struct {
  value: i32,
  drops: Ptr(mut)(i32),
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

let read(value: borrow(Resource)): i32 = { value.value }

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let total = do {
    let values: Array(Resource)(3) = [
      Resource { value: 9, drops: drops },
      Resource { value: 12, drops: drops },
      Resource { value: 21, drops: drops },
    ]
    let slice: borrow(Slice(Resource)) = borrow(values)
    let mut sum = 0
    for slice.iter() { value ->
      sum = sum + read(value)
    }
    sum
  }
  let drop_count = unsafe { *drops }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  total + drop_count - 3
}

test("slice_iterator_resource.sc") {
  main() == 42
}
