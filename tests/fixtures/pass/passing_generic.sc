let Token = struct { value: i32 }
let Holder(T: type) = struct { value: T }

extend(T: type) Holder(T) {
  let into(M: (P: parameters): parameters)(M self)(): T = { self.value }
}

let apply(M: (P: parameters): parameters, T: type)(M value: T): T = { value }
let modifier_identity(M: (P: parameters): parameters) = M
let forward(M: (P: parameters): parameters, T: type)(M value: T): T = {
  apply(modifier_identity(M), T)(value)
}

let main(): i32 = {
  let number = 20
  let copied = forward(copy, i32)(number)
  let moved_number = apply(M: move, T: i32)(2)
  let token = Token { value: 20 }
  let moved = forward(move, Token)(token)
  let explicit = apply(move, Token)(Token { value: 0 })
  let from_method = Holder { value: 0 }.into(move)()
  copied + moved_number + moved.value + explicit.value + from_method
}

test("passing_generic.sc") {
  main() == 42
}
