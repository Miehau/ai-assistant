export interface ToolResult {
  success: boolean;
  result: string;
  metadata?: Record<string, any>;
}

export interface Tool {
  name: string;
  description: string;
  parameters: Record<string, any>;
  execute: (input: any) => Promise<ToolResult>;
} 