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
