let Unsafe = std.unsafe.Unsafe

let Ask = effect {
  let value(): i32
}

let Resource = struct {
  counter: Ptr(mut)(i32),
  value: i32,
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: Resource): i32 = { resource.value }

let ignore(move action: (): i32): i32 = { 29 }

let once(move action: (): i32): i32 = { action() }

let repeat(move action: (): ()): () = {
  action()
  action()
}

let effect_once(E: effect)(move action: (): i32 with(E)): i32 with(E) = {
  action()
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }

  let abandoned = Resource { counter: counter, value: 100 }
  let ignored = ignore({ consume(abandoned) })

  let consumed = Resource { counter: counter, value: 5 }
  let invoked = once({ consume(consumed) })

  let mut calls = 0
  repeat({ () ->
    calls = calls + 1;
    ()
  })

  let captured = 0
  let effect_resource = Resource { counter: counter, value: 1 }
  let effectful = Ask.handle value { (resume) -> resume(3) } action {
      effect_once(Ask)({
        Ask.value() + captured + consume(effect_resource) - 1
      })
    }

  let unsafe_effect = unsafe {
    effect_once(Unsafe)({ *counter - *counter })
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  ignored + invoked + calls + effectful + unsafe_effect + drops
}

test("capturing_callable_bridge.sc") {
  main() == 42
}
