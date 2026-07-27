/// Authority effect required for operations that can violate language safety.
pub let unsafety = effect {}

/// Runs an action that requires the unsafe authority effect.
pub let unsafe(comptime e: effects, comptime t: type)
  (move action: (): t with(core.unsafe.unsafety, e)): t with(e) = {
  core.unsafe.unsafety.handle
    action {
      action()
    }
}
