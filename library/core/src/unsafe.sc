/// Authority effect required for operations that can violate language safety.
pub let unsafety = effect {}

/// Runs an action that requires the unsafe authority effect.
pub let unsafe(comptime e: effects, comptime t: type): with(e)(move action: with(core.unsafe.unsafety, e)((): t)): t = {
  core.unsafe.unsafety.handle
    action {
      action()
    }
}
