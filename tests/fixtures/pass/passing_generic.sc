let token = struct { value: i32 }
let holder(comptime t: type) = struct { value: t }

extend(comptime t: type) holder(t) {
  let into(comptime m: (comptime p: parameters): parameters)(m self)(): t = { self.value }
}

let apply(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }
let modifier_identity(comptime m: (comptime p: parameters): parameters) = m
let forward(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = {
  apply(modifier_identity(m), t)(value)
}

let main(): i32 = {
  let number = 20
  let copied = forward(copy, i32)(number)
  let moved_number = apply(comptime m: move, comptime t: i32)(2)
  let token = token { value: 20 }
  let moved = forward(move, token)(token)
  let explicit = apply(move, token)(token { value: 0 })
  let from_method = holder { value: 0 }.into(move)()
  copied + moved_number + moved.value + explicit.value + from_method
}

test("passing_generic.sc") {
  main() == 42
}
