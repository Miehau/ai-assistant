export const MODEL_CONFIG = {
	agent: {
		provider: 'openai' as const,
		model: 'gpt-4-turbo',
		temperature: 0.1,
		response_format: { type: 'json_object' },
		max_tokens: 2000,
		stream: false
	},
	userFacing: {
		provider: 'anthropic' as const,
		model: 'claude-3-sonnet-20240229',
		temperature: 0.7,
		max_tokens: 4000,
		stream: true
	}
} as const;

export const ORCHESTRATOR_CONFIG = {
	maxAgentIterations: 10, // Prevent infinite loops
	agentTimeout: 30000, // 30s max for agent work
	enableAgentCache: true, // Cache tool results
	cacheTTL: 300000, // 5min cache
	alwaysInvokeAgent: true, // Start simple, optimize later
	costTracking: true // Track token usage & costs
} as const;
