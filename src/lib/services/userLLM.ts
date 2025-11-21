import type { AgentResult } from '$lib/types/agent';
import { MODEL_CONFIG } from '$lib/config/models';
import { invoke } from '@tauri-apps/api/tauri';
import { modelService } from '$lib/models/modelService';

export class UserLLMService {
	async *formatResponse(
		userQuery: string,
		conversationHistory: any[],
		agentResult?: AgentResult,
		selectedModelName?: string
	): AsyncGenerator<string> {
		const systemPrompt = this.buildUserLLMPrompt(agentResult);

		const messages = [
			{ role: 'system', content: systemPrompt },
			...conversationHistory.map((m) => ({
				role: m.role,
				content: m.content
			})),
			{ role: 'user', content: userQuery }
		];

		// Add agent findings if available
		if (agentResult?.success) {
			messages.push({
				role: 'system',
				content: `Agent findings:\n${JSON.stringify(agentResult.data, null, 2)}\n\nFormat this naturally for the user.`
			});
		}

		// Call streaming LLM with selected model
		yield* this.callStreamingLLM(messages, selectedModelName);
	}

	private buildUserLLMPrompt(agentResult?: AgentResult): string {
		let prompt = `You are a helpful AI assistant. Provide clear, natural responses to user queries.`;

		if (agentResult) {
			if (agentResult.success) {
				prompt += `\n\nYou have access to real-time data gathered by an agent system. Present this information naturally and conversationally. If the data includes sources, mention them for transparency.`;
			} else if (agentResult.error) {
				prompt += `\n\nThe agent encountered an error: ${agentResult.error.message}. Explain this to the user in a helpful way and suggest alternatives if possible.`;
			}
		}

		return prompt;
	}

	private async *callStreamingLLM(messages: any[], selectedModelName?: string): AsyncGenerator<string> {
		// Get model info from Tauri backend
		const models = await invoke<any[]>('get_models');
		const modelName = selectedModelName || MODEL_CONFIG.userFacing.model;
		let modelInfo = models.find(m => m.model_name === modelName);

		// Fallback: If not found in database, check registry
		if (!modelInfo) {
			console.log(`Model ${modelName} not found in database, checking registry`);
			const registryModels = modelService.getAvailableModelsWithCapabilities();
			modelInfo = registryModels.find(m => m.model_name === modelName);

			if (!modelInfo) {
				throw new Error(`Model not found in database or registry: ${modelName}`);
			}
		}

		const provider = modelInfo.provider;
		const apiKey = await invoke<string | null>('get_api_key', { provider });
		if (!apiKey) {
			throw new Error(`No API key found for provider: ${provider}`);
		}

		if (provider === 'anthropic') {
			yield* this.callAnthropicStreaming(messages, apiKey, modelName);
		} else if (provider === 'openai') {
			yield* this.callOpenAIStreaming(messages, apiKey, modelName);
		} else {
			throw new Error(`Unsupported provider for user-facing LLM: ${provider}`);
		}
	}

	private async *callAnthropicStreaming(
		messages: any[],
		apiKey: string,
		modelName: string
	): AsyncGenerator<string> {
		// Combine ALL system messages into one (Anthropic only accepts a single system prompt)
		const systemMessages = messages.filter((m) => m.role === 'system');
		const combinedSystemContent = systemMessages.map(m => m.content).join('\n\n');
		const conversationMessages = messages.filter((m) => m.role !== 'system');

		const response = await fetch('https://api.anthropic.com/v1/messages', {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				'x-api-key': apiKey,
				'anthropic-version': '2023-06-01',
				'anthropic-dangerous-direct-browser-access': 'true'
			},
			body: JSON.stringify({
				model: modelName,
				system: combinedSystemContent || '',
				messages: conversationMessages,
				temperature: MODEL_CONFIG.userFacing.temperature,
				max_tokens: MODEL_CONFIG.userFacing.max_tokens,
				stream: true
			})
		});

		if (!response.ok) {
			const error = await response.text();
			throw new Error(`Anthropic API error: ${response.status} - ${error}`);
		}

		const reader = response.body?.getReader();
		const decoder = new TextDecoder();

		if (!reader) {
			throw new Error('No response body');
		}

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;

			const chunk = decoder.decode(value);
			const lines = chunk.split('\n').filter((line) => line.trim());

			for (const line of lines) {
				if (line.startsWith('data: ')) {
					const data = line.slice(6);
					if (data === '[DONE]') continue;

					try {
						const parsed = JSON.parse(data);
						if (parsed.type === 'content_block_delta' && parsed.delta?.text) {
							yield parsed.delta.text;
						}
					} catch (e) {
						// Skip invalid JSON
					}
				}
			}
		}
	}

	private async *callOpenAIStreaming(messages: any[], apiKey: string, modelName: string): AsyncGenerator<string> {
		const response = await fetch('https://api.openai.com/v1/chat/completions', {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
				Authorization: `Bearer ${apiKey}`
			},
			body: JSON.stringify({
				model: modelName,
				messages,
				temperature: MODEL_CONFIG.userFacing.temperature,
				max_tokens: MODEL_CONFIG.userFacing.max_tokens,
				stream: true
			})
		});

		if (!response.ok) {
			const error = await response.text();
			throw new Error(`OpenAI API error: ${response.status} - ${error}`);
		}

		const reader = response.body?.getReader();
		const decoder = new TextDecoder();

		if (!reader) {
			throw new Error('No response body');
		}

		while (true) {
			const { done, value } = await reader.read();
			if (done) break;

			const chunk = decoder.decode(value);
			const lines = chunk.split('\n').filter((line) => line.trim());

			for (const line of lines) {
				if (line.startsWith('data: ')) {
					const data = line.slice(6);
					if (data === '[DONE]') continue;

					try {
						const parsed = JSON.parse(data);
						const content = parsed.choices?.[0]?.delta?.content;
						if (content) {
							yield content;
						}
					} catch (e) {
						// Skip invalid JSON
					}
				}
			}
		}
	}
}

export const userLLMService = new UserLLMService();
