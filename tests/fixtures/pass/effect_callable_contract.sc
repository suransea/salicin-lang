let EffectCallable = std.effect.EffectCallable

let abandon(move action: EffectCallable(i32, i32, i32)): () = { () }

let main(): i32 = { 42 }

test("effect_callable_contract.sc") {
  main() == 42
}
