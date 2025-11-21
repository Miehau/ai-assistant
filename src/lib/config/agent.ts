/**
 * Agent configuration constants
 * Centralized configuration for the AI agent system
 */

export const AGENT_CONFIG = {
  /**
   * Maximum number of iterations the agent will perform before providing final answer
   */
  MAX_ITERATIONS: 4,

  /**
   * Context length for web fetcher tool
   */
  WEB_FETCHER: {
    CONTEXT_LENGTH: 100,
  },
} as const;
