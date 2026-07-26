/// Authority effect required for operations that can violate language safety.
pub let unsafe_effect = effect {}

/// Runs an action that requires the unsafe authority effect.
pub let unsafe(comptime e: effects, comptime t: type)
  (move action: (): t with(core.unsafe.unsafe_effect, e)): t with(e) = {
  core.unsafe.unsafe_effect.handle
    action {
      action()
    }
}
