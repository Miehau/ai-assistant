import type { DateTime } from "luxon";

export interface SystemPrompt {
    id: string;
    name: string;
    content: string;
    created_at: string;  // ISO string from backend
    updated_at: string;  // ISO string from backend
}
