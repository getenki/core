// Browser fallback for @getenki/ai
// This module is for browser environments where native bindings are not available

class EnkiAgent {
  constructor(options = {}) {
    throw new Error(
      '@getenki/ai requires native bindings and cannot run in the browser. ' +
      'Please use this module only in Node.js environments or server-side code. ' +
      'For browser-based usage, consider using a server API instead.'
    )
  }
}

module.exports = {
  EnkiAgent,
}
