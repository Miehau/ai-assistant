export type ToolResultMode = 'inline' | 'persist' | 'auto';

export interface ToolMetadata {
  name: string;
  description: string;
  args_schema: unknown;
  result_schema: unknown;
  requires_approval: boolean;
  result_mode: ToolResultMode;
}
