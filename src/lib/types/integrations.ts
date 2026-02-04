export interface IntegrationMetadata {
    id: string;
    name: string;
    provider: string;
    description: string;
    auth_type: string;
    category: string;
    capabilities: string[];
}

export interface GoogleCalendarListItem {
    id: string;
    summary: string;
    primary: boolean;
    time_zone?: string;
    access_role?: string;
}
