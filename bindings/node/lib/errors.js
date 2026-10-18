'use strict'

// A render that failed. Each kind of failure has its own subclass, so catch RustyprintError for
// every failure, or check for a subclass for one kind.
class RustyprintError extends Error {
  constructor (message) {
    super(message)
    this.name = new.target.name
    // The kind of failure: 'invalid_options', 'limit_exceeded', 'timeout' or 'internal'.
    this.kind = new.target.kind
  }
}

// The options are not valid, such as an unknown option or page size.
class InvalidOptionsError extends RustyprintError {
  static kind = 'invalid_options'
}

// The document exceeds one of the limits in the options.
class LimitExceededError extends RustyprintError {
  static kind = 'limit_exceeded'
}

// Rendering took longer than the deadline in the options.
class TimeoutError extends RustyprintError {
  static kind = 'timeout'
}

// A bug in rustyprint.
class InternalError extends RustyprintError {
  static kind = 'internal'
}

const CLASSES = {
  invalid_options: InvalidOptionsError,
  limit_exceeded: LimitExceededError,
  timeout: TimeoutError,
  internal: InternalError
}

// The error for a kind of failure. 'invalid_argument', for arguments the binding or the library
// refuses, is a TypeError; an unknown kind is an InternalError.
function errorFor (kind, message) {
  if (kind === 'invalid_argument') return new TypeError(message)
  const Class = CLASSES[kind] ?? InternalError
  return new Class(message)
}

module.exports = {
  RustyprintError,
  InvalidOptionsError,
  LimitExceededError,
  TimeoutError,
  InternalError,
  errorFor
}
