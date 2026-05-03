export interface LLMProvider {
  generate(request: LLMRequest): Promise<LLMResponse>
  stream(request: LLMRequest): AsyncIterable<LLMStreamEvent>
  transcribeAudio?(request: LLMAudioTranscriptionRequest): Promise<LLMAudioTranscriptionResponse>
}

export interface LLMRequest {
  model: string
  messages: LLMMessage[]
  tools?: LLMToolDefinition[]
  structured_output?: Record<string, unknown> // JSON Schema
  temperature?: number
  max_tokens?: number
  signal?: AbortSignal
}

export interface LLMAudioTranscriptionRequest {
  model: string
  input_audio: {
    data: string
    format: string
  }
  signal?: AbortSignal
}

export interface LLMAudioTranscriptionResponse {
  text: string
  usage?: unknown
}

export interface LLMMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string | LLMContentBlock[]
  tool_call_id?: string // for role='tool'
  tool_calls?: LLMToolCall[] // for role='assistant' with tool calls
}

export interface LLMContentBlock {
  type: 'text' | 'image' | 'tool_use' | 'tool_result'
  text?: string
  // image fields
  media_type?: string
  data?: string // base64
  // tool fields
  tool_use_id?: string
  name?: string
  input?: Record<string, unknown>
  content?: string
}

export interface LLMResponse {
  content: string | unknown
  companion_text?: string
  tool_calls?: LLMToolCall[]
  usage: {
    input_tokens: number
    output_tokens: number
    cache_read_input_tokens?: number
    cache_creation_input_tokens?: number
  }
  finish_reason: string
}

export type LLMStreamEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'text_done'; text: string }
  | { type: 'tool_call_delta'; call_id: string; name?: string; arguments_delta: string }
  | { type: 'tool_call_done'; call_id: string; name: string; arguments: string }
  | { type: 'done'; response: LLMResponse }
  | { type: 'error'; error: string }

export interface LLMToolDefinition {
  name: string
  description: string
  parameters: Record<string, unknown> // JSON Schema
}

export interface LLMToolCall {
  call_id: string
  name: string
  arguments: Record<string, unknown>
}

export interface ProviderRegistry {
  register(name: string, provider: LLMProvider): void
  resolve(model: string): LLMProvider
  list(): string[]
}
