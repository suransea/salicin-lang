/// Structured non-local failure for one test registration.
pub let failure = effect {
  /// Aborts the current registration with an optional owned UTF-8 message.
  let abort(move message: core.option(core.string.string)): never
}

/// The normalized result of interpreting one test registration.
pub let outcome = enum {
  passed,
  failed(core.option(core.string.string)),
}

/// Interprets exactly one registration and normalizes the boolean migration
/// contract into the structured outcome model.
pub let run(
  move action: with(failure)((): bool),
): outcome = {
  failure.handle
    abort { (message) -> outcome.failed(message) }
    done {
      (value) ->
        if value {
          outcome.passed
        } else {
          outcome.failed(core.option.none)
        }
    }
    action {
      action()
    }
}
