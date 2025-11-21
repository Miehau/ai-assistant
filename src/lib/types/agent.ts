import type { z } from 'zod';

// Agent-specific types
export interface AgentMessage {
	role: 'system' | 'user' | 'assistant';
	content: string;
	timestamp: number;
}

export interface AgentResult {
	success: boolean;
	data?: Record<string, any>;
	toolsUsed: ToolExecution[];
	iterations: number;
	conversationContext?: string;
	error?: AgentError;
	metadata: AgentMetadata;
}

export interface AgentError {
	type: 'tool_failure' | 'max_iterations' | 'invalid_response' | 'timeout';
	message: string;
	recoverable: boolean;
	details?: any;
}

export interface AgentMetadata {
	tokensUsed: number;
	latencyMs: number;
	cost?: number;
	modelUsed: string;
}

export interface ToolExecution {
	id: string;
	tool: string;
	parameters: Record<string, any>;
	result: any;
	success: boolean;
	duration: number;
	timestamp: number;
	error?: string;
}

// User-facing LLM types
export interface UserLLMContext {
	userQuery: string;
	conversationHistory: any[];
	agentResult?: AgentResult;
	systemInstructions: string;
}
