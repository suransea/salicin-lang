let view(comptime t: type)(comptime r: region): type = borrow(r)(t)

let lend = trait {
  let item(comptime r: region): type
}

let cell = struct { value: i32 }

extend cell: lend {
  let item = view(i32)
}

let require_i64(comptime t: type)(move value: t): ()
where t: lend(item(comptime r: region) = borrow(r)(i64)) = {}

let main(): () = {
  require_i64(cell { value: 42 })
}
