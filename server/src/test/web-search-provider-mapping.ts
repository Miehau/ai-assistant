import assert from 'node:assert/strict'
import { mapToolsToAnthropic } from '../providers/anthropic.js'
import { resolveOpenAIModelForWebSearch } from '../providers/openai.js'
import type { LLMToolDefinition } from '../providers/types.js'

const webSearchTool: LLMToolDefinition = {
  name: 'web_search',
  description: 'Provider-native web search',
  parameters: { type: 'object', properties: {} },
}

const fetchTool: LLMToolDefinition = {
  name: 'web.fetch',
  description: 'Fetch URL content',
  parameters: {
    type: 'object',
    properties: { url: { type: 'string' } },
    required: ['url'],
  },
}

assert.equal(resolveOpenAIModelForWebSearch('gpt-5.4-mini'), 'gpt-5-search-api')
assert.equal(resolveOpenAIModelForWebSearch('gpt-4o-mini'), 'gpt-4o-mini-search-preview')
assert.equal(resolveOpenAIModelForWebSearch('gpt-4o'), 'gpt-4o-search-preview')
assert.equal(resolveOpenAIModelForWebSearch('gpt-5-search-api'), 'gpt-5-search-api')

const anthropicTools = mapToolsToAnthropic([webSearchTool, fetchTool])

assert.deepEqual(anthropicTools[0], {
  type: 'web_search_20250305',
  name: 'web_search',
  max_uses: 5,
})
assert.equal(anthropicTools[1].name, 'web__fetch')

console.log('Web search provider mapping tests passed')
