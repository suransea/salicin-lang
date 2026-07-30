let unsafety = core.unsafe.unsafety

let ask = effect {
  let value(): i32
}

let resource = struct {
  counter: ptr(mut)(i32),
  value: i32,
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: resource): i32 = { resource.value }

let ignore(move action: (): i32): i32 = { 29 }

let once(move action: (): i32): i32 = { action() }

let repeat(move action: (): ()): () = {
  action()
  action()
}

let effect_once(comptime e: effects): with(e)(move action: with(e)((): i32)): i32 = {
  action()
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }

  let abandoned = resource { counter: counter, value: 100 }
  let ignored = ignore({ consume(abandoned) })

  let consumed = resource { counter: counter, value: 5 }
  let invoked = once({ consume(consumed) })

  let mut calls = 0
  repeat({ () ->
    calls = calls + 1;
    ()
  })

  let captured = 0
  let effect_resource = resource { counter: counter, value: 1 }
  let effectful = ask.handle value { (resume) -> resume(3) } action {
      effect_once(ask)({
        ask.value() + captured + consume(effect_resource) - 1
      })
    }

  let unsafety = unsafe {
    effect_once(unsafety)({ *counter - *counter })
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  ignored + invoked + calls + effectful + unsafety + drops
}

test("capturing_callable_bridge.sc") {
  std.test.assert(main() == 42)
}
