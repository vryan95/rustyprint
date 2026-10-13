# frozen_string_literal: true

module Rustyprint
  # A render that failed. Each kind of failure has its own subclass, so rescue
  # {Rustyprint::Error} for every failure, or a subclass for one kind.
  class Error < StandardError
    # @return [Symbol] the kind of failure: `:invalid_options`, `:limit_exceeded`, `:timeout` or
    #   `:internal`
    attr_reader :kind

    # @param message [String, nil] what went wrong
    # @param kind [Symbol] the kind of failure; the subclass's `KIND` by default
    def initialize(message = nil, kind: self.class::KIND)
      super(message)
      @kind = kind
    end
  end

  # The options are not valid, such as an unknown option or page size.
  class InvalidOptionsError < Error
    # The {Error#kind} of this class's errors.
    KIND = :invalid_options
  end

  # The document exceeds one of the limits in the options.
  class LimitExceededError < Error
    # The {Error#kind} of this class's errors.
    KIND = :limit_exceeded
  end

  # Rendering took longer than the deadline in the options.
  class TimeoutError < Error
    # The {Error#kind} of this class's errors.
    KIND = :timeout
  end

  # A bug in rustyprint.
  class InternalError < Error
    # The {Error#kind} of this class's errors.
    KIND = :internal
  end
end
