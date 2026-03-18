export interface EnkiAgentOptions {
  name?: string
  systemPromptPreamble?: string
  model?: string
  maxIterations?: number
  workspaceHome?: string
}

export declare class NativeEnkiAgent {
  constructor(
    name?: string | null,
    systemPromptPreamble?: string | null,
    model?: string | null,
    maxIterations?: number | null,
    workspaceHome?: string | null,
  )

  run(sessionId: string, userMessage: string): Promise<string>
}

export declare class EnkiAgent {
  constructor(options?: EnkiAgentOptions)
  run(sessionId: string, userMessage: string): Promise<string>
}

export interface AgentOptions {
  instructions?: string
  name?: string
  maxIterations?: number
  workspaceHome?: string
  tools?: Tool[]
  memories?: MemoryModule[]
  llm?: LlmProviderBackend | ((model: string, messages: Array<Record<string, unknown>>, tools: Array<Record<string, unknown>>) => unknown)
  lowLevelAgent?: unknown
}

export declare class RunContext<DepsT = unknown> {
  constructor(deps: DepsT)
  deps: DepsT
}

export declare class AgentRunResult {
  constructor(output: string)
  output: string
}

export interface ToolOptions {
  usesContext: boolean
  name?: string
  description?: string
  parametersJson?: string
}

export interface ToolShape {
  name: string
  description?: string
  parametersJson: string
  func: (...args: unknown[]) => unknown
  usesContext: boolean
}

export declare class Tool {
  constructor(shape: ToolShape)
  name: string
  description: string
  parametersJson: string
  func: (...args: unknown[]) => unknown
  usesContext: boolean
  static fromFunction(func: (...args: unknown[]) => unknown, options: ToolOptions): Tool
}

export interface MemoryModuleShape {
  name: string
  record: (sessionId: string, userMsg: string, assistantMsg: string) => unknown
  recall: (sessionId: string, query: string, maxEntries: number) => unknown
  flush?: (sessionId: string) => unknown
  consolidate?: (sessionId: string) => unknown
}

export declare class MemoryModule {
  constructor(shape: MemoryModuleShape)
  name: string
  record: MemoryModuleShape['record']
  recall: MemoryModuleShape['recall']
  flush?: MemoryModuleShape['flush']
  consolidate?: MemoryModuleShape['consolidate']
}

export declare class MemoryBackend {
  name?: string
  asMemoryModule(): MemoryModule
}

export declare abstract class LlmProviderBackend {
  abstract complete(
    model: string,
    messages: Array<Record<string, unknown>>,
    tools: Array<Record<string, unknown>>,
  ): unknown
}

export interface AgentRunOptions<DepsT = unknown> {
  deps?: DepsT
  sessionId?: string
}

export declare class Agent<DepsT = unknown> {
  constructor(model: string, options?: AgentOptions)
  toolPlain(func: (...args: unknown[]) => unknown, options?: Omit<ToolOptions, 'usesContext'>): (...args: unknown[]) => unknown
  tool(func: (...args: unknown[]) => unknown, options?: Omit<ToolOptions, 'usesContext'>): (...args: unknown[]) => unknown
  registerTool(tool: Tool | ToolShape): Tool
  registerMemory(memory: MemoryModule | MemoryModuleShape): MemoryModule
  run(userMessage: string, options?: AgentRunOptions<DepsT>): Promise<AgentRunResult>
}
