import type { CustomBackend, CreateCustomBackendInput, UpdateCustomBackendInput } from "$lib/types/customBackend";

/**
 * Service for managing custom backends
 * Uses Svelte 5 runes for reactivity
 */
export class CustomBackendService {
    backends = $state<CustomBackend[]>([]);
    loading = $state<boolean>(false);
    error = $state<string | null>(null);

    /**
     * Load all custom backends from storage
     */
    public async loadBackends(): Promise<CustomBackend[]> {
        // Not yet implemented in server backend
        console.warn('[customBackendService] loadBackends not yet implemented in server backend');
        return [];
    }

    /**
     * Get a specific backend by ID
     */
    public async getBackend(id: string): Promise<CustomBackend | null> {
        console.warn('[customBackendService] getBackend not yet implemented in server backend');
        return null;
    }

    /**
     * Create a new custom backend
     */
    public async createBackend(input: CreateCustomBackendInput): Promise<CustomBackend | null> {
        throw new Error('Not yet implemented in server backend');
    }

    /**
     * Update an existing custom backend
     */
    public async updateBackend(input: UpdateCustomBackendInput): Promise<CustomBackend | null> {
        throw new Error('Not yet implemented in server backend');
    }

    /**
     * Delete a custom backend
     */
    public async deleteBackend(id: string): Promise<boolean> {
        throw new Error('Not yet implemented in server backend');
    }

    /**
     * Get a backend by name
     */
    public getBackendByName(name: string): CustomBackend | undefined {
        return this.backends.find(b => b.name === name);
    }

    /**
     * Get all backends
     */
    public getAllBackends(): CustomBackend[] {
        return [...this.backends];
    }
}

// Export singleton instance
export const customBackendService = new CustomBackendService();
