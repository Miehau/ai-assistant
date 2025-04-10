import type { ModelCapabilities, ModelSpecs } from "$lib/models/registry/types";

export interface Model {
    provider: string;
    model_name: string;
    name?: string; // Human-readable name
    enabled: boolean;
    url?: string;
    deployment_name?: string;
    capabilities?: ModelCapabilities;
    specs?: ModelSpecs;
}
