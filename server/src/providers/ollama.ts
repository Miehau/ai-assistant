import { OpenAIProvider } from './openai.js'

/**
 * Ollama provider — reuses OpenAI-compatible API with a custom base URL.
 * No API key required; passes 'ollama' as a dummy key.
 */
export class OllamaProvider extends OpenAIProvider {
  constructor(baseUrl: string = 'http://localhost:11434/v1') {
    super('ollama', baseUrl)
  }
}
