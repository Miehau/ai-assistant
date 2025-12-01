# LLM Providers Architecture Review

## Overview

The LLM provider system has a good foundational design with an abstract base class and provider-specific implementations. However, incomplete migration from legacy to new patterns, SOLID violations, and code duplication create technical debt.

---

## Current Structure

```
src/lib/services/
├── base/
│   └── LLMService.ts      # Abstract base class
├── openai.ts              # OpenAI provider
├── anthropic.ts           # Anthropic/Claude provider
├── deepseek.ts            # DeepSeek provider
├── customProvider.ts      # Custom API endpoints
└── chat.ts                # Contains factory function
```

---

## SOLID Violations

### 1. Interface Segregation Principle (ISP)

**File**: `src/lib/services/base/LLMService.ts:41-44`

All providers forced to implement `transcribeAudio()`:

```typescript
abstract transcribeAudio(
  base64Audio: string,
  context: string
): Promise<string>;
```

**Reality**:
| Provider | Supports Audio? | Implementation |
|----------|-----------------|----------------|
| OpenAI | ✅ Yes | Actual implementation |
| Anthropic | ❌ No | `throw new LLMServiceError('not supported')` |
| DeepSeek | ❌ No | `throw new Error('Not implemented')` |
| CustomProvider | ❌ No | `throw new LLMServiceError('not supported')` |

**Fix Option 1** - Capability interface:
```typescript
interface AudioCapable {
  transcribeAudio(base64Audio: string, context: string): Promise<string>;
}

function hasAudioCapability(service: LLMService): service is LLMService & AudioCapable {
  return service.providerName === 'openai';
}
```

**Fix Option 2** - Optional method:
```typescript
abstract class LLMService {
  // Not abstract, default implementation
  async transcribeAudio(base64Audio: string, context: string): Promise<string> {
    throw new LLMServiceError(
      'Audio transcription not supported',
      this.providerName
    );
  }
}
```

---

### 2. Open/Closed Principle (OCP)

**File**: `src/lib/services/chat.ts:21-32`

Hard-coded switch statement for provider creation:

```typescript
function createLLMService(provider: string, apiKey: string): LLMService {
  switch (provider.toLowerCase()) {
    case 'openai':
      return new OpenAIService(apiKey);
    case 'anthropic':
      return new AnthropicService(apiKey);
    case 'deepseek':
      return new DeepSeekService(apiKey);
    default:
      throw new Error(`Unsupported provider: ${provider}`);
  }
}
```

Adding a new provider requires modifying this function.

**Fix** - Provider registry:
```typescript
// src/lib/services/llm/registry.ts
type ProviderFactory = (apiKey: string) => LLMService;

const providers = new Map<string, ProviderFactory>();

export function registerProvider(name: string, factory: ProviderFactory) {
  providers.set(name.toLowerCase(), factory);
}

export function createLLMService(provider: string, apiKey: string): LLMService {
  const factory = providers.get(provider.toLowerCase());
  if (!factory) {
    throw new Error(`Unsupported provider: ${provider}`);
  }
  return factory(apiKey);
}

// Each provider self-registers
// openai.ts
registerProvider('openai', (apiKey) => new OpenAIService(apiKey));
```

---

### 3. Liskov Substitution Principle (LSP)

**Files**: All provider implementations

Inconsistent return types for `createChatCompletion()`:

| Provider | Return Type |
|----------|-------------|
| OpenAI | `Promise<{ content: string; usage?: UsageData }>` |
| Anthropic | `Promise<{ content: string; usage?: UsageData }>` |
| DeepSeek | `Promise<string>` ❌ |
| CustomProvider | `Promise<string>` ❌ |

DeepSeek and CustomProvider return different types, breaking substitutability.

**Fix**: Standardize return type across all providers:
```typescript
interface ChatCompletionResult {
  content: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
  };
}

abstract createChatCompletion(...): Promise<ChatCompletionResult>;
```

---

## Code Duplication

### Streaming Logic (~50 lines × 3)

**Locations**:
- `openai.ts:237-287`
- `deepseek.ts:63-101`
- `customProvider.ts:106-159`

Nearly identical code:
```typescript
const reader = response.body?.getReader();
const decoder = new TextDecoder();
let fullResponse = '';

while (true) {
  const { done, value } = await reader.read();
  if (done) break;

  const lines = decoder.decode(value).split('\n');
  for (const line of lines) {
    if (line.startsWith('data: ')) {
      const data = line.slice(6);
      if (data === '[DONE]') continue;
      // Parse and callback...
    }
  }
}
```

**Fix**: Extract to base class or utility:
```typescript
// src/lib/services/base/streaming.ts
export async function processSSEStream(
  response: Response,
  onChunk: (content: string) => void,
  parseChunk: (data: string) => string | null
): Promise<string> {
  const reader = response.body?.getReader();
  const decoder = new TextDecoder();
  let fullResponse = '';

  while (reader) {
    const { done, value } = await reader.read();
    if (done) break;

    const lines = decoder.decode(value).split('\n');
    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = line.slice(6);
        if (data === '[DONE]') continue;
        const content = parseChunk(data);
        if (content) {
          fullResponse += content;
          onChunk(content);
        }
      }
    }
  }

  return fullResponse;
}
```

---

## Legacy vs New API

### Current State

Each provider has TWO sets of methods:

**Legacy (deprecated but used)**:
```typescript
/**
 * @deprecated Use completion() instead
 */
async createChatCompletion(
  model: string,
  messages: any[],
  streamResponse: boolean,
  onStreamResponse: (chunk: string) => void,
  signal: AbortSignal
): Promise<...>
```

**New (exists but unused)**:
```typescript
async completion(options: CompletionOptions): Promise<LLMResponse>
async structuredCompletion<T>(options: StructuredCompletionOptions<T>): Promise<T>
```

**Problem**: ChatService bypasses types to use legacy methods:
```typescript
return (llmService as any).createChatCompletion(...)  // Line 433
```

### Migration Path

1. **Update ChatService to use new interface**:
```typescript
// Before
return (llmService as any).createChatCompletion(
  model.model_name,
  formattedMessages,
  streamResponse,
  onStreamResponse,
  signal
);

// After
return llmService.completion({
  model: model.model_name,
  messages: formattedMessages,
  stream: streamResponse,
  onChunk: onStreamResponse,
  signal
});
```

2. **Add streaming support to new interface**:
```typescript
interface CompletionOptions {
  model: string;
  messages: LLMMessage[];
  stream?: boolean;
  onChunk?: (chunk: string) => void;
  signal?: AbortSignal;
}
```

3. **Remove legacy methods** once migration complete.

---

## Error Handling Inconsistency

### Current Patterns

**OpenAI legacy** (line 56):
```typescript
throw new Error(`OpenAI API error: ${response.statusText}`);
```

**OpenAI new** (line 134):
```typescript
throw new LLMServiceError('OpenAI completion failed', this.providerName, error);
```

**Anthropic** (line 400):
```typescript
throw new LLMServiceError(`Anthropic streaming failed`, this.providerName, error);
```

**DeepSeek** (line 47):
```typescript
throw new Error(`DeepSeek API error: ${response.statusText}`);
```

### Fix

Standardize on `LLMServiceError`:
```typescript
class LLMServiceError extends Error {
  constructor(
    message: string,
    public provider: string,
    public cause?: unknown,
    public statusCode?: number
  ) {
    super(message);
    this.name = 'LLMServiceError';
  }
}
```

---

## Hard-Coded Configuration

### API URLs
```typescript
// openai.ts:49
"https://api.openai.com/v1/chat/completions"

// deepseek.ts:33
"https://api.deepseek.com/chat/completions"
```

### API Versions
```typescript
// anthropic.ts:137
'anthropic-version': '2023-06-01',
'anthropic-beta': 'structured-outputs-2025-11-13',
```

### Timeouts
| Provider | Timeout |
|----------|---------|
| OpenAI | 5 minutes |
| CustomProvider | 3 minutes |
| Anthropic | None |
| DeepSeek | None |

**Fix**: Centralized configuration:
```typescript
// src/lib/config/llm.ts
export const LLM_CONFIG = {
  openai: {
    baseUrl: 'https://api.openai.com/v1',
    timeout: 300000,
  },
  anthropic: {
    baseUrl: 'https://api.anthropic.com/v1',
    version: '2023-06-01',
    timeout: 300000,
  },
  deepseek: {
    baseUrl: 'https://api.deepseek.com',
    timeout: 300000,
  },
  defaults: {
    timeout: 300000,
  }
};
```

---

## Recommended Refactoring

### Target Structure
```
src/lib/services/llm/
├── base/
│   ├── LLMService.ts      # Abstract base (no audio requirement)
│   ├── streaming.ts       # Shared streaming utilities
│   └── errors.ts          # LLMServiceError
├── providers/
│   ├── OpenAIService.ts
│   ├── AnthropicService.ts
│   ├── DeepSeekService.ts
│   └── CustomProviderService.ts
├── capabilities/
│   └── AudioCapable.ts    # Optional audio interface
├── registry.ts            # Provider registration
├── factory.ts             # createLLMService()
└── config.ts              # URLs, timeouts, versions
```

### Priority Order

1. **Extract streaming logic** - Highest ROI, removes ~150 lines of duplication
2. **Fix return type inconsistency** - DeepSeek/CustomProvider
3. **Complete migration to new interface** - Remove legacy methods
4. **Implement provider registry** - Better extensibility
5. **Extract audio to capability** - Clean ISP violation
