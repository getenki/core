'use strict'

let cachedNativeBinding = null

function loadNativeBinding() {
  if (cachedNativeBinding == null) {
    cachedNativeBinding = require('./index.js')
  }
  return cachedNativeBinding
}

function getNativeEnkiAgent() {
  return loadNativeBinding().NativeEnkiAgent
}

const DEFAULT_AGENT_NAME = 'Agent'
const DEFAULT_MAX_ITERATIONS = 20

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

    const NativeEnkiAgent = getNativeEnkiAgent()

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

class RunContext {
  constructor(deps) {
    this.deps = deps
  }
}

class AgentRunResult {
  constructor(output) {
    this.output = output
  }
}

class Tool {
  constructor({ name, description = '', parametersJson, func, usesContext }) {
    this.name = requiredString(name, 'name')
    this.description = typeof description === 'string' ? description : ''
    this.parametersJson = validateJsonSchema(parametersJson)
    if (typeof func !== 'function') {
      throw new TypeError('func must be a function')
    }
    this.func = func
    this.usesContext = Boolean(usesContext)
  }

  static fromFunction(func, options = {}) {
    if (typeof func !== 'function') {
      throw new TypeError('func must be a function')
    }

    const {
      usesContext,
      name,
      description,
      parametersJson,
    } = options

    return new Tool({
      name: name ?? func.name ?? 'tool',
      description: description ?? getFunctionDescription(func),
      parametersJson: parametersJson ?? JSON.stringify({
        type: 'object',
        properties: {},
        additionalProperties: false,
      }),
      func,
      usesContext: Boolean(usesContext),
    })
  }

  asLowLevelTool() {
    return {
      name: this.name,
      description: this.description,
      parametersJson: this.parametersJson,
    }
  }
}

class MemoryModule {
  constructor({ name, record, recall, flush, consolidate }) {
    this.name = requiredString(name, 'name')
    if (typeof record !== 'function') {
      throw new TypeError('record must be a function')
    }
    if (typeof recall !== 'function') {
      throw new TypeError('recall must be a function')
    }
    if (flush != null && typeof flush !== 'function') {
      throw new TypeError('flush must be a function')
    }
    if (consolidate != null && typeof consolidate !== 'function') {
      throw new TypeError('consolidate must be a function')
    }

    this.record = record
    this.recall = recall
    this.flush = flush
    this.consolidate = consolidate
  }

  asLowLevelMemory() {
    return { name: this.name }
  }
}

class MemoryBackend {
  asMemoryModule() {
    return new MemoryModule({
      name: this.name ?? 'memory',
      record: this.record.bind(this),
      recall: this.recall.bind(this),
      flush: typeof this.flush === 'function' ? this.flush.bind(this) : undefined,
      consolidate:
        typeof this.consolidate === 'function' ? this.consolidate.bind(this) : undefined,
    })
  }
}

class LlmProviderBackend {}

class ToolHandler {
  constructor(tools) {
    this._tools = tools
    this._deps = undefined
  }

  setDeps(deps) {
    this._deps = deps
  }

  clearDeps() {
    this._deps = undefined
  }

  async execute(toolName, argsJson, agentDir, workspaceDir, sessionsDir) {
    const tool = this._tools.get(toolName)
    if (!tool) {
      throw new Error(`Unknown tool '${toolName}'`)
    }

    const parsedArgs = parseObjectJson(argsJson, `Tool '${toolName}' expected JSON object args`)
    const args = []
    if (tool.usesContext) {
      args.push(new RunContext(this._deps))
    }

    for (const parameter of getFunctionParameters(tool.func, tool.usesContext)) {
      if (Object.prototype.hasOwnProperty.call(parsedArgs, parameter.name)) {
        args.push(parsedArgs[parameter.name])
      } else if (parameter.hasDefault) {
        args.push(parameter.defaultValue)
      } else {
        throw new TypeError(`Missing required argument '${parameter.name}' for tool '${toolName}'`)
      }
    }

    void agentDir
    void workspaceDir
    void sessionsDir

    const result = await Promise.resolve(tool.func(...args))
    return stringifyToolResult(result)
  }
}

class MemoryHandler {
  constructor(memories) {
    this._memories = memories
  }

  async record(memoryName, sessionId, userMsg, assistantMsg) {
    const memory = this._get(memoryName)
    await Promise.resolve(memory.record(sessionId, userMsg, assistantMsg))
  }

  async recall(memoryName, sessionId, query, maxEntries) {
    const memory = this._get(memoryName)
    return (await Promise.resolve(memory.recall(sessionId, query, maxEntries))) ?? []
  }

  async flush(memoryName, sessionId) {
    const memory = this._get(memoryName)
    if (memory.flush) {
      await Promise.resolve(memory.flush(sessionId))
    }
  }

  async consolidate(memoryName, sessionId) {
    const memory = this._get(memoryName)
    if (memory.consolidate) {
      await Promise.resolve(memory.consolidate(sessionId))
    }
  }

  _get(memoryName) {
    const memory = this._memories.get(memoryName)
    if (!memory) {
      throw new Error(`Unknown memory '${memoryName}'`)
    }
    return memory
  }
}

class LlmHandler {
  constructor(provider) {
    this._provider = provider
  }

  async complete(model, messagesJson, toolsJson) {
    const messages = parseArrayJson(messagesJson)
    const tools = parseArrayJson(toolsJson)
    const result =
      this._provider instanceof LlmProviderBackend
        ? await Promise.resolve(this._provider.complete(model, messages, tools))
        : await Promise.resolve(this._provider(model, messages, tools))

    return typeof result === 'string' ? result : JSON.stringify(result)
  }
}

class Agent {
  constructor(model, options = {}) {
    this.model = requiredString(model, 'model')

    if (options == null || typeof options !== 'object' || Array.isArray(options)) {
      throw new TypeError('Agent options must be an object')
    }

    const {
      instructions = '',
      name = DEFAULT_AGENT_NAME,
      maxIterations = DEFAULT_MAX_ITERATIONS,
      workspaceHome,
      tools = [],
      memories = [],
      llm,
      lowLevelAgent,
    } = options

    this.instructions = optionalString(instructions, 'instructions') ?? ''
    this.name = optionalString(name, 'name') ?? DEFAULT_AGENT_NAME
    this.maxIterations = optionalPositiveInteger(maxIterations, 'maxIterations') ?? DEFAULT_MAX_ITERATIONS
    this.workspaceHome = optionalString(workspaceHome, 'workspaceHome')
    this._lowLevelAgent = lowLevelAgent ?? Agent._LowLevelEnkiAgent
    this._tools = new Map()
    this._memories = new Map()
    this._toolHandler = new ToolHandler(this._tools)
    this._memoryHandler = new MemoryHandler(this._memories)
    this._llmHandler = llm == null ? null : new LlmHandler(llm)
    this._backend = null
    this._dirty = true

    for (const tool of tools) {
      this.registerTool(tool)
    }
    for (const memory of memories) {
      this.registerMemory(memory)
    }
  }

  toolPlain(func, options = {}) {
    this.registerTool(Tool.fromFunction(func, { ...options, usesContext: false }))
    return func
  }

  tool(func, options = {}) {
    if (getFunctionParameters(func, false).length === 0) {
      throw new TypeError(`Tool '${func.name || 'anonymous'}' must accept a RunContext argument`)
    }
    this.registerTool(Tool.fromFunction(func, { ...options, usesContext: true }))
    return func
  }

  registerTool(tool) {
    const normalized = tool instanceof Tool ? tool : new Tool(tool)
    this._tools.set(normalized.name, normalized)
    this._dirty = true
    return normalized
  }

  registerMemory(memory) {
    const normalized = memory instanceof MemoryModule ? memory : new MemoryModule(memory)
    this._memories.set(normalized.name, normalized)
    this._dirty = true
    return normalized
  }

  async run(userMessage, options = {}) {
    const backend = this._ensureBackend()
    const message = requiredString(userMessage, 'userMessage')
    const sessionId = requiredString(options.sessionId ?? createSessionId(), 'sessionId')

    this._toolHandler.setDeps(options.deps)
    try {
      const output = await backend.run(sessionId, message)
      return new AgentRunResult(output)
    } finally {
      this._toolHandler.clearDeps()
    }
  }

  _ensureBackend() {
    if (this._backend != null && !this._dirty) {
      return this._backend
    }

    const toolSpecs = Array.from(this._tools.values(), (tool) => tool.asLowLevelTool())
    const memorySpecs = Array.from(this._memories.values(), (memory) => memory.asLowLevelMemory())
    const LowLevelAgent = this._lowLevelAgent

    if (this._llmHandler && toolSpecs.length > 0 && memorySpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withToolsMemoryAndLlm', 'with_tools_memory_and_llm'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          tools: toolSpecs,
          toolHandler: this._toolHandler,
          memories: memorySpecs,
          memoryHandler: this._memoryHandler,
          llmHandler: this._llmHandler,
        },
        'custom tools, memory, and llm',
      )
    } else if (this._llmHandler && toolSpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withToolsAndLlm', 'with_tools_and_llm'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          tools: toolSpecs,
          handler: this._toolHandler,
          llmHandler: this._llmHandler,
        },
        'custom tools and llm',
      )
    } else if (this._llmHandler && memorySpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withMemoryAndLlm', 'with_memory_and_llm'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          memories: memorySpecs,
          handler: this._memoryHandler,
          llmHandler: this._llmHandler,
        },
        'custom memory and llm',
      )
    } else if (this._llmHandler) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withLlm', 'with_llm'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          llmHandler: this._llmHandler,
        },
        'custom llm',
      )
    } else if (toolSpecs.length > 0 && memorySpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withToolsAndMemory', 'with_tools_and_memory'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          tools: toolSpecs,
          toolHandler: this._toolHandler,
          memories: memorySpecs,
          memoryHandler: this._memoryHandler,
        },
        'custom tools and memory',
      )
    } else if (toolSpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withTools', 'with_tools'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          tools: toolSpecs,
          handler: this._toolHandler,
        },
        'custom tools',
      )
    } else if (memorySpecs.length > 0) {
      this._backend = callFactory(
        LowLevelAgent,
        ['withMemory', 'with_memory'],
        {
          name: this.name,
          systemPromptPreamble: this.instructions,
          model: this.model,
          maxIterations: this.maxIterations,
          workspaceHome: this.workspaceHome,
          memories: memorySpecs,
          handler: this._memoryHandler,
        },
        'custom memory',
      )
    } else {
      this._backend = new LowLevelAgent(
        this.name,
        this.instructions,
        this.model,
        this.maxIterations,
        this.workspaceHome,
      )
    }

    this._dirty = false
    return this._backend
  }
}

Object.defineProperty(Agent, '_LowLevelEnkiAgent', {
  configurable: true,
  enumerable: true,
  get() {
    return getNativeEnkiAgent()
  },
})

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

function validateJsonSchema(value) {
  const json = requiredString(value, 'parametersJson')
  JSON.parse(json)
  return json
}

function getFunctionDescription(func) {
  return typeof func.description === 'string' ? func.description : ''
}

function parseObjectJson(value, errorMessage) {
  const parsed = value ? JSON.parse(value) : {}
  if (parsed == null) {
    return {}
  }
  if (typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new TypeError(errorMessage)
  }
  return parsed
}

function parseArrayJson(value) {
  const parsed = value ? JSON.parse(value) : []
  return Array.isArray(parsed) ? parsed : []
}

function callFactory(LowLevelAgent, methodNames, args, featureName) {
  for (const methodName of methodNames) {
    if (typeof LowLevelAgent[methodName] === 'function') {
      return LowLevelAgent[methodName](args)
    }
  }

  throw new Error(`NativeEnkiAgent does not support ${featureName}`)
}

function createSessionId() {
  return `session-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function getFunctionParameters(func, usesContext) {
  const source = func.toString().replace(/\s+/g, ' ')
  const match =
    source.match(/^[^(]*\(([^)]*)\)/) ??
    source.match(/^([^=()]+?)\s*=>/)

  const rawParameters = match?.[1]
    ? match[1].split(',').map((part) => part.trim()).filter(Boolean)
    : []

  const parameters = rawParameters.map((parameter) => {
    const [namePart, defaultPart] = parameter.split('=').map((part) => part.trim())
    return {
      name: namePart,
      hasDefault: defaultPart != null,
      defaultValue: defaultPart == null ? undefined : parseDefaultValue(defaultPart),
    }
  })

  return usesContext ? parameters.slice(1) : parameters
}

function parseDefaultValue(value) {
  if (value === 'true') {
    return true
  }
  if (value === 'false') {
    return false
  }
  if (value === "''" || value === '""') {
    return ''
  }
  if (/^-?\d+(\.\d+)?$/.test(value)) {
    return Number(value)
  }
  return undefined
}

function stringifyToolResult(value) {
  if (typeof value === 'string') {
    return value
  }
  if (value == null) {
    return ''
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return String(value)
  }
  return JSON.stringify(value)
}

module.exports.Agent = Agent
module.exports.AgentRunResult = AgentRunResult
module.exports.EnkiAgent = EnkiAgent
module.exports.LlmProviderBackend = LlmProviderBackend
module.exports.MemoryBackend = MemoryBackend
module.exports.MemoryModule = MemoryModule
module.exports.NativeEnkiAgent = undefined
module.exports.RunContext = RunContext
module.exports.Tool = Tool

Object.defineProperty(module.exports, 'NativeEnkiAgent', {
  configurable: true,
  enumerable: true,
  get() {
    return getNativeEnkiAgent()
  },
})
