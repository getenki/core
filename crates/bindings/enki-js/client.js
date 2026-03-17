'use strict'

const { NativeEnkiAgent, ...nativeBinding } = require('./index.js')

class EnkiAgent {
  constructor(options = {}) {
    if (options == null || typeof options !== 'object' || Array.isArray(options)) {
      throw new TypeError('EnkiAgent options must be an object')
    }

    const {
      name,
      systemPromptPreamble,
      model,
      maxIterations,
      workspaceHome,
    } = options

    this._native = new NativeEnkiAgent(
      optionalString(name, 'name'),
      optionalString(systemPromptPreamble, 'systemPromptPreamble'),
      optionalString(model, 'model'),
      optionalPositiveInteger(maxIterations, 'maxIterations'),
      optionalString(workspaceHome, 'workspaceHome'),
    )
  }

  run(sessionId, userMessage) {
    return this._native.run(
      requiredString(sessionId, 'sessionId'),
      requiredString(userMessage, 'userMessage'),
    )
  }
}

function requiredString(value, field) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new TypeError(`${field} must be a non-empty string`)
  }
  return value
}

function optionalString(value, field) {
  if (value == null) {
    return undefined
  }
  if (typeof value !== 'string') {
    throw new TypeError(`${field} must be a string`)
  }
  return value
}

function optionalPositiveInteger(value, field) {
  if (value == null) {
    return undefined
  }
  if (!Number.isInteger(value) || value < 1) {
    throw new TypeError(`${field} must be a positive integer`)
  }
  return value
}

module.exports = {
  ...nativeBinding,
  EnkiAgent,
  NativeEnkiAgent,
}
