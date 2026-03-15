import OpenAI from 'openai'
import { OpenAIProvider } from './openai.js'

/**
 * OpenRouter provider — reuses OpenAI-compatible API with OpenRouter's base URL
 * and required HTTP headers.
 */
export class OpenRouterProvider extends OpenAIProvider {
  constructor(apiKey: string) {
    super(apiKey, 'https://openrouter.ai/api/v1')

    // Re-create the client with default headers that OpenRouter requires
    this.client = new OpenAI({
      apiKey,
      baseURL: 'https://openrouter.ai/api/v1',
      defaultHeaders: {
        'HTTP-Referer': 'https://ai-frontend.app',
        'X-Title': 'AI Frontend',
      },
    })
  }
}
