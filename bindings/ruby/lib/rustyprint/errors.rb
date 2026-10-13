# frozen_string_literal: true

module Rustyprint
  # A render that failed. Each kind of failure has its own subclass.
  class Error < StandardError
    # The kind of failure: :invalid_options, :limit_exceeded, :timeout or :internal.
    attr_reader :kind

    def initialize(message = nil, kind: self.class::KIND)
      super(message)
      @kind = kind
    end
  end

  # The options are not valid, such as an unknown option or page size.
  class InvalidOptionsError < Error
    KIND = :invalid_options
  end

  # The document exceeds one of the limits in the options.
  class LimitExceededError < Error
    KIND = :limit_exceeded
  end

  # Rendering took longer than the deadline in the options.
  class TimeoutError < Error
    KIND = :timeout
  end

  # A bug in rustyprint.
  class InternalError < Error
    KIND = :internal
  end
end
