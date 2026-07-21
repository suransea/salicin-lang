let Decide = effect {
  let accept(): bool
}

let Resource = struct(value: i32)
let Event = enum { Value(Resource), Empty }

let classify(event: Event): i32 with(Decide) = {
  event match {
    Value(resource) if Decide.accept() => resource.value,
    Value(resource) => resource.value,
    Empty => 0,
  }
}

let main(): i32 = {
  Decide.handle(
    accept: { (resume) -> resume(false) },
  ) {
    classify(Event.Value(Resource(42)))
  }
}
