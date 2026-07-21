let Boxed = struct(value: i32)

let consume(move boxed: Boxed): bool = { true }

let use_value(run: bool): i32 = {
  let boxed = Boxed(value: 42)
  let consumed = run && consume(boxed)
  boxed.value
}

let main(): i32 = { use_value(false) }
