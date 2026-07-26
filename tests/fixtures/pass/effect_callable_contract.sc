let effect_callable = std.effect.effect_callable

let abandon(move action: effect_callable(i32, i32, i32)): () = { () }

let main(): i32 = { 42 }

test("effect_callable_contract.sc") {
  main() == 42
}
