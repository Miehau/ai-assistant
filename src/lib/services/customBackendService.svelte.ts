import { invoke } from "@tauri-apps/api/tauri";
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
        this.loading = true;
        this.error = null;

        try {
            const backends = await invoke<CustomBackend[]>("get_custom_backends");
            this.backends = backends;
            console.log('[CustomBackendService] Loaded backends:', backends.length);
            return backends;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            console.error('[CustomBackendService] Error loading backends:', error);
            return [];
        } finally {
            this.loading = false;
        }
    }

    /**
     * Get a specific backend by ID
     */
    public async getBackend(id: string): Promise<CustomBackend | null> {
        try {
            const backend = await invoke<CustomBackend | null>("get_custom_backend", { id });
            return backend;
        } catch (error) {
            console.error(`[CustomBackendService] Error getting backend ${id}:`, error);
            return null;
        }
    }

    /**
     * Create a new custom backend
     */
    public async createBackend(input: CreateCustomBackendInput): Promise<CustomBackend | null> {
        this.loading = true;
        this.error = null;

        try {
            const backend = await invoke<CustomBackend>("create_custom_backend", { input });
            this.backends = [...this.backends, backend];
            console.log('[CustomBackendService] Created backend:', backend.name);
            return backend;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            console.error('[CustomBackendService] Error creating backend:', error);
            return null;
        } finally {
            this.loading = false;
        }
    }

    /**
     * Update an existing custom backend
     */
    public async updateBackend(input: UpdateCustomBackendInput): Promise<CustomBackend | null> {
        this.loading = true;
        this.error = null;

        try {
            const backend = await invoke<CustomBackend | null>("update_custom_backend", { input });
            if (backend) {
                this.backends = this.backends.map(b => b.id === backend.id ? backend : b);
                console.log('[CustomBackendService] Updated backend:', backend.name);
            }
            return backend;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            console.error('[CustomBackendService] Error updating backend:', error);
            return null;
        } finally {
            this.loading = false;
        }
    }

    /**
     * Delete a custom backend
     */
    public async deleteBackend(id: string): Promise<boolean> {
        this.loading = true;
        this.error = null;

        try {
            const success = await invoke<boolean>("delete_custom_backend", { id });
            if (success) {
                this.backends = this.backends.filter(b => b.id !== id);
                console.log('[CustomBackendService] Deleted backend:', id);
            }
            return success;
        } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            this.error = message;
            console.error('[CustomBackendService] Error deleting backend:', error);
            return false;
        } finally {
            this.loading = false;
        }
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
