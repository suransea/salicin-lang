/// The normalized result of interpreting one test registration.
pub let outcome = enum {
  passed,
  failed(core.string.string),
}

/// Interprets exactly one unit-returning, string-throwing registration.
pub let run(
  move action: with(core.error.throwing(core.string.string))((): ()),
): outcome = {
  core.error.throwing(core.string.string).handle
    raise { (message) -> outcome.failed(message) }
    done { (_) -> outcome.passed }
    action {
      action()
    }
}
