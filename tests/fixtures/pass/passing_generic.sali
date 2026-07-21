let Token = struct(value: i32)
let Holder(T: type) = struct(value: T)

extend(T: type) Holder(T) {
  let into(P: passing)(P self)(): T = self.value
}

let identity(P: passing, T: type)(P value: T): T = value
let forward(P: passing, T: type)(P value: T): T = identity(P, T)(value)

let main(): i32 = {
  let number = 20
  let copied = forward(copy, i32)(number)
  let moved_number = identity(P: move, T: i32)(2)
  let token = Token(20)
  let moved = forward(move, Token)(token)
  let automatic = identity(Token(0))
  let from_method = Holder(0).into(move)()
  copied + moved_number + moved.value + automatic.value + from_method
}
