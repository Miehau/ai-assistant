import { agentService } from './agent';
import { userLLMService } from './userLLM';
import { ORCHESTRATOR_CONFIG } from '$lib/config/models';

export interface OrchestratorEvent {
	type: 'agent_status' | 'agent_complete' | 'stream_chunk' | 'error';
	data: any;
}

export class Orchestrator {
	async *handleUserMessage(
		userMessage: string,
		conversationId: string,
		conversationHistory: any[],
		userId?: string,
		selectedModel?: string
	): AsyncGenerator<OrchestratorEvent> {
		try {
			// Decide whether to invoke agent
			const shouldInvokeAgent = this.shouldInvokeAgent(userMessage, conversationHistory);

			let agentResult;

			if (shouldInvokeAgent) {
				// Invoke agent
				yield {
					type: 'agent_status',
					data: { status: 'working', message: 'Agent analyzing your request...' }
				};

				agentResult = await agentService.processQuery(userMessage, conversationId, userId, selectedModel);

				yield {
					type: 'agent_complete',
					data: {
						success: agentResult.success,
						toolsUsed: agentResult.toolsUsed,
						iterations: agentResult.iterations,
						metadata: agentResult.metadata
					}
				};
			}

			// Stream user-facing response
			const stream = userLLMService.formatResponse(userMessage, conversationHistory, agentResult, selectedModel);

			for await (const chunk of stream) {
				yield { type: 'stream_chunk', data: chunk };
			}
		} catch (error: any) {
			yield {
				type: 'error',
				data: { message: error.message }
			};
		}
	}

	private shouldInvokeAgent(message: string, history: any[]): boolean {
		// For now, always invoke (as per config)
		if (ORCHESTRATOR_CONFIG.alwaysInvokeAgent) {
			return true;
		}

		// TODO: Implement smarter logic
		// - Check for real-time data keywords
		// - Check if previous agent results can answer
		// - Use classifier model

		const keywords = [
			'weather',
			'current',
			'latest',
			'api',
			'fetch',
			'get data',
			'search',
			'find',
			'lookup'
		];
		return keywords.some((kw) => message.toLowerCase().includes(kw));
	}
}

export const orchestrator = new Orchestrator();
