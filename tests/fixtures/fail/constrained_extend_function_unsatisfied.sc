let resource = struct { value: i32 }
let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t)
where t: copyable {
  let new(copy value: t): cell(t) = { cell { value: value } }
}

let main(): i32 = { cell.new(resource { value: 42 }).value.value }
