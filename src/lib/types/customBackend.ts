export interface CustomBackend {
    id: string;
    name: string;
    url: string;
    api_key?: string;
    created_at: number;
}

export interface CreateCustomBackendInput {
    name: string;
    url: string;
    api_key?: string;
}

export interface UpdateCustomBackendInput {
    id: string;
    name?: string;
    url?: string;
    api_key?: string;
}
