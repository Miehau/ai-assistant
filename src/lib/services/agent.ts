import type { AgentResult, AgentMessage } from "$lib/types/agent";
import type { ToolCall } from "$lib/types/tools";
import { toolExecutor } from "./toolExecutor";
import { toolRegistry } from "./toolRegistry";
import { buildAgentSystemPrompt, buildAnthropicAgentSystemPrompt } from "./agentPrompt";
import { MODEL_CONFIG, ORCHESTRATOR_CONFIG } from "$lib/config/models";
import { invoke } from "@tauri-apps/api/tauri";
import { modelService } from "$lib/models/modelService";

interface AgentConversationState {
  history: AgentMessage[];
  toolCache: Map<string, { result: any; timestamp: number }>;
  createdAt: number;
}

export class AgentService {
  private conversationStates = new Map<string, AgentConversationState>();

  async processQuery(
    userQuery: string,
    conversationId: string,
    userId?: string,
    selectedModel?: string,
  ): Promise<AgentResult> {
    const startTime = Date.now();
    const state = this.getOrCreateState(conversationId);
    const toolsUsed: any[] = [];
    let iterations = 0;
    let totalTokens = 0;

    try {
      // Add user message to agent history
      state.history.push({
        role: "user",
        content: userQuery,
        timestamp: Date.now(),
      });

      // Agent loop
      while (iterations < ORCHESTRATOR_CONFIG.maxAgentIterations) {
        iterations++;

        // Call agent LLM
        const response = await this.callAgentLLM(state.history, selectedModel);
        console.warn("Agent response:", response.content, "stopReason:", response.stopReason);
        totalTokens += response.tokensUsed;

        // Check if this is Anthropic native tool use
        if (response.stopReason === "tool_use") {
          // Native Anthropic tool use - parse tool_use blocks
          const contentBlocks = JSON.parse(response.content);
          const toolUseBlocks = contentBlocks.filter((block: any) => block.type === "tool_use");

          if (toolUseBlocks.length === 0) {
            console.error("stop_reason is tool_use but no tool_use blocks found");
            continue;
          }

          // Add assistant message with tool_use blocks to history
          state.history.push({
            role: "assistant",
            content: response.content, // Keep as JSON string of blocks
            timestamp: Date.now(),
          });

          // Execute tools
          // Add defensive logging for undefined inputs
          toolUseBlocks.forEach((block: any) => {
            if (!block.input) {
              console.warn(`Tool ${block.name} called with undefined input. Block:`, block);
            }
          });

          const calls: ToolCall[] = toolUseBlocks.map((block: any) => ({
            tool: block.name,
            parameters: block.input || {}, // Default to empty object if undefined
          }));

          const context = {
            conversationId,
            userId,
            toolCallId: `${conversationId}-${iterations}`,
            cache: state.toolCache,
          };

          console.log("Executing tools:", calls);
          const results = await toolExecutor.executeBatch(calls, context);
          console.log("Tool execution results:", results);

          // Format tool results as tool_result blocks
          const toolResultBlocks = toolUseBlocks.map((block: any, i: number) => ({
            type: "tool_result",
            tool_use_id: block.id,
            content: JSON.stringify(results[i]),
          }));

          // Add tool results to history as user message
          state.history.push({
            role: "user",
            content: JSON.stringify(toolResultBlocks),
            timestamp: Date.now(),
          });

          // Track tool usage
          calls.forEach((call, i) => {
            toolsUsed.push({
              id: `${iterations}-${i}`,
              tool: call.tool,
              parameters: call.parameters,
              result: results[i].data,
              success: results[i].success,
              duration: results[i].metadata.duration,
              timestamp: Date.now(),
            });

            // Cache successful results
            if (results[i].success && ORCHESTRATOR_CONFIG.enableAgentCache) {
              const cacheKey = `${call.tool}:${JSON.stringify(call.parameters)}`;
              state.toolCache.set(cacheKey, {
                result: results[i],
                timestamp: Date.now(),
              });
            }
          });

          // Check if all tools failed
          if (results.every((r) => !r.success)) {
            const allNonRetriable = results.every((r) => !r.error?.retriable);
            if (allNonRetriable) {
              return {
                success: false,
                toolsUsed,
                iterations,
                error: {
                  type: "tool_failure",
                  message: "All tool executions failed",
                  recoverable: false,
                  details: results.map((r) => r.error),
                },
                metadata: {
                  tokensUsed: totalTokens,
                  latencyMs: Date.now() - startTime,
                  modelUsed: MODEL_CONFIG.agent.model,
                },
              };
            }
          }

          continue; // Loop to get next response
        }

        // Not tool_use - this is the final response or JSON-based (OpenAI)
        // Try parsing as JSON first (for OpenAI)
        const parsed = this.parseAgentResponse(response.content);

        if (parsed.success) {
          // JSON-based response (OpenAI path)
          // Check if final response
          if (parsed.tool === "respond") {
            console.log("Final response received:", parsed.parameters.data);
            return {
              success: true,
              data: parsed.parameters.data,
              toolsUsed,
              iterations,
              conversationContext: parsed.parameters.summary,
              metadata: {
                tokensUsed: totalTokens,
                latencyMs: Date.now() - startTime,
                modelUsed: MODEL_CONFIG.agent.model,
              },
            };
          }

          // Execute tool(s) - OpenAI JSON path
          const calls: ToolCall[] = parsed.tools || [
            { tool: parsed.tool, parameters: parsed.parameters },
          ];
          const context = {
            conversationId,
            userId,
            toolCallId: `${conversationId}-${iterations}`,
            cache: state.toolCache,
          };

          console.log("Executing tools:", calls);
          const results = await toolExecutor.executeBatch(calls, context);
          console.log("Tool execution results:", results);

          // Add to history and tracking
          state.history.push({
            role: "assistant",
            content: JSON.stringify(parsed),
            timestamp: Date.now(),
          });

          state.history.push({
            role: "user",
            content: JSON.stringify({ results }),
            timestamp: Date.now(),
          });

          calls.forEach((call, i) => {
            toolsUsed.push({
              id: `${iterations}-${i}`,
              tool: call.tool,
              parameters: call.parameters,
              result: results[i].data,
              success: results[i].success,
              duration: results[i].metadata.duration,
              timestamp: Date.now(),
            });

            // Cache successful results
            if (results[i].success && ORCHESTRATOR_CONFIG.enableAgentCache) {
              const cacheKey = `${call.tool}:${JSON.stringify(call.parameters)}`;
              state.toolCache.set(cacheKey, {
                result: results[i],
                timestamp: Date.now(),
              });
            }
          });

          // Check if all tools failed
          if (results.every((r) => !r.success)) {
            const allNonRetriable = results.every((r) => !r.error?.retriable);
            if (allNonRetriable) {
              return {
                success: false,
                toolsUsed,
                iterations,
                error: {
                  type: "tool_failure",
                  message: "All tool executions failed",
                  recoverable: false,
                  details: results.map((r) => r.error),
                },
                metadata: {
                  tokensUsed: totalTokens,
                  latencyMs: Date.now() - startTime,
                  modelUsed: MODEL_CONFIG.agent.model,
                },
              };
            }
          }

          continue;
        }

        // Not JSON and not tool_use - this is Anthropic's final text response
        const contentBlocks = JSON.parse(response.content);
        const textBlock = contentBlocks.find((block: any) => block.type === "text");
        const finalText = textBlock?.text || JSON.stringify(contentBlocks);

        console.log("Final text response from Anthropic:", finalText);
        return {
          success: true,
          data: { response: finalText },
          toolsUsed,
          iterations,
          metadata: {
            tokensUsed: totalTokens,
            latencyMs: Date.now() - startTime,
            modelUsed: MODEL_CONFIG.agent.model,
          },
        };
      }

      // Max iterations reached
      return {
        success: false,
        data: { partialResults: toolsUsed },
        toolsUsed,
        iterations,
        error: {
          type: "max_iterations",
          message: `Exceeded maximum iterations (${ORCHESTRATOR_CONFIG.maxAgentIterations})`,
          recoverable: false,
        },
        metadata: {
          tokensUsed: totalTokens,
          latencyMs: Date.now() - startTime,
          modelUsed: MODEL_CONFIG.agent.model,
        },
      };
    } catch (error: any) {
      return {
        success: false,
        toolsUsed,
        iterations,
        error: {
          type: "timeout",
          message: error.message,
          recoverable: false,
        },
        metadata: {
          tokensUsed: totalTokens,
          latencyMs: Date.now() - startTime,
          modelUsed: MODEL_CONFIG.agent.model,
        },
      };
    }
  }

  private async callAgentLLM(
    history: AgentMessage[],
    selectedModel?: string,
  ): Promise<{ content: string; tokensUsed: number; stopReason?: string }> {
    // Get model info (with registry fallback like userLLM.ts)
    const models = await invoke<any[]>("get_models");
    const modelName = selectedModel || MODEL_CONFIG.agent.model;
    let modelInfo = models.find((m) => m.model_name === modelName);

    // Fallback: If not found in database, check registry
    if (!modelInfo) {
      console.log(
        `Model ${modelName} not found in database, checking registry`,
      );
      const registryModels = modelService.getAvailableModelsWithCapabilities();
      modelInfo = registryModels.find((m) => m.model_name === modelName);

      if (!modelInfo) {
        throw new Error(
          `Model not found in database or registry: ${modelName}`,
        );
      }
    }

    const provider = modelInfo.provider;
    const apiKey = await invoke<string | null>("get_api_key", { provider });
    if (!apiKey) {
      throw new Error(`No API key found for provider: ${provider}`);
    }

    // Route to provider-specific method
    if (provider === "anthropic") {
      return this.callAnthropicAgentWithTools(history, apiKey, modelName);
    } else if (provider === "openai") {
      const systemPrompt = buildAgentSystemPrompt(toolRegistry.getAllDefinitions());
      const messages = [{ role: "system", content: systemPrompt }, ...history];
      return this.callOpenAIAgent(messages, apiKey, modelName);
    } else {
      throw new Error(`Unsupported provider for agent: ${provider}`);
    }
  }

  private async callOpenAIAgent(
    messages: any[],
    apiKey: string,
    modelName: string,
  ): Promise<{ content: string; tokensUsed: number }> {
    const response = await fetch("https://api.openai.com/v1/chat/completions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${apiKey}`,
      },
      body: JSON.stringify({
        model: modelName,
        messages,
        temperature: MODEL_CONFIG.agent.temperature,
        response_format: MODEL_CONFIG.agent.response_format,
        max_tokens: MODEL_CONFIG.agent.max_tokens,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`OpenAI API error: ${response.status} - ${error}`);
    }

    const data = await response.json();
    return {
      content: data.choices[0].message.content,
      tokensUsed: data.usage.total_tokens,
    };
  }

  private async callAnthropicAgentWithTools(
    history: AgentMessage[],
    apiKey: string,
    modelName: string,
  ): Promise<{ content: string; tokensUsed: number; stopReason?: string }> {
    const systemPrompt = buildAnthropicAgentSystemPrompt();

    // Clean messages: Anthropic accepts {role, content}
    // Content can be string or array of blocks (for tool_use/tool_result)
    const conversationMessages = history.map((m) => {
      let content = m.content;
      // If content looks like JSON array, parse it
      if (typeof content === 'string' && content.trim().startsWith('[')) {
        try {
          content = JSON.parse(content);
        } catch (e) {
          // Keep as string if parsing fails
        }
      }
      return {
        role: m.role,
        content,
      };
    });

    // Get tools excluding "respond" - with native tool use, we don't need it
    const tools = toolRegistry.toAnthropicToolSchema().filter(t => t.name !== 'respond');

    const requestBody = {
      model: modelName,
      system: systemPrompt,
      messages: conversationMessages,
      tools: tools,
      temperature: 0.1,
      max_tokens: 2000,
    };

    const response = await fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": apiKey,
        "anthropic-version": "2023-06-01",
        "anthropic-dangerous-direct-browser-access": "true",
      },
      body: JSON.stringify(requestBody),
    });

    if (!response.ok) {
      const error = await response.text();
      console.error("[Agent Anthropic] Error response:", error);
      throw new Error(`Anthropic API error: ${response.status} - ${error}`);
    }

    const data = await response.json();

    // Return full response with stop_reason to determine if more tools are needed
    return {
      content: JSON.stringify(data.content), // content is an array of blocks
      tokensUsed: data.usage.input_tokens + data.usage.output_tokens,
      stopReason: data.stop_reason,
    };
  }

  private parseAgentResponse(
    content: string,
  ):
    | { success: true; tool: string; parameters: any; tools?: ToolCall[] }
    | { success: false; error: string } {
    try {
      const parsed = JSON.parse(content);

      // Single tool call
      if (parsed.tool && parsed.parameters) {
        return {
          success: true,
          tool: parsed.tool,
          parameters: parsed.parameters,
        };
      }

      // Batch tool calls
      if (parsed.tools && Array.isArray(parsed.tools)) {
        return {
          success: true,
          tool: "batch",
          parameters: {},
          tools: parsed.tools,
        };
      }

      return { success: false, error: 'Missing "tool" or "tools" field' };
    } catch (e: any) {
      return { success: false, error: `Invalid JSON: ${e.message}` };
    }
  }

  private getOrCreateState(conversationId: string): AgentConversationState {
    if (!this.conversationStates.has(conversationId)) {
      this.conversationStates.set(conversationId, {
        history: [],
        toolCache: new Map(),
        createdAt: Date.now(),
      });
    }
    return this.conversationStates.get(conversationId)!;
  }

  // Clean up old conversations
  cleanupOldStates(maxAgeMs: number = 3600000): void {
    const now = Date.now();
    for (const [id, state] of this.conversationStates.entries()) {
      if (now - state.createdAt > maxAgeMs) {
        this.conversationStates.delete(id);
      }
    }
  }
}

export const agentService = new AgentService();
