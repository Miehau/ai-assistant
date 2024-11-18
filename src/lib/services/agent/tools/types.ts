export interface ToolResult {
  success: boolean;
  result: string;
  metadata?: Record<string, any>;
}

export interface Tool {
  name: string;
  description: string;
  execute: (input: any) => Promise<ToolResult>;
} 