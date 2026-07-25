/// Authority effect required for operations that can violate language safety.
pub let Unsafe = effect {}

/// Runs an action that requires the unsafe authority effect.
pub let unsafe(E: effect, T: type)
  (move action: (): T with(core.unsafe.Unsafe, E)): T with(E) = {
  core.unsafe.Unsafe.handle
    action {
      action()
    }
}
