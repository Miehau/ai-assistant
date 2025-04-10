# RFC: Model Registry and Auto-Configuration Feature

## Overview
This document outlines the design and implementation plan for a centralized model registry system that will automatically configure and manage AI models across different providers in the application.

## Background
Currently, models are manually added to the application, requiring explicit configuration for each model. This approach is not scalable and requires frequent updates as providers add new models.

## Goals
- Provide a centralized registry of all supported AI models
- Enable automatic model availability based on API key presence
- Simplify model selection and configuration process
- Ensure extensibility for future providers and model types

## Model Schema
```typescript
interface ProviderConfig {
  id: string;                  // e.g., "openai", "anthropic", "google"
  name: string;                // Display name (e.g., "OpenAI")
  authType: "api_key" | "oauth";// Authentication method
  baseUrl: string;            // API endpoint with placeholders
  defaultHeaders: Record<string, string>; // Headers with placeholders for API keys
}

interface ModelConfig {
  name: string;               // Human-readable name (e.g., "GPT-4 Turbo")
  provider: string;           // Reference to provider ID
  capabilities: {
    text: boolean;            // Text generation
    vision: boolean;          // Image understanding
    audio: boolean;           // Audio processing/generation
    embedding: boolean;       // Vector embeddings
    function_calling: boolean;// Function calling support
  };
  specs: {
    contextWindow: number;    // Max context size in tokens
    maxOutputTokens?: number; // Max output tokens if different from context
    tokenLimit?: number;      // Rate limit per minute if applicable
    tokenization: {
      specialTokens?: {      // Special tokens that affect counting
        system?: string[];   // System message tokens
        user?: string[];    // User message tokens
        assistant?: string[]; // Assistant message tokens
        function?: string[]; // Function-related tokens
      };
      encoding?: string;    // Encoding method (e.g., 'cl100k_base' for GPT-4)
      averageTokensPerChar?: number; // For rough estimation
    };
  };
  defaultParameters: {        // Default model parameters
    temperature?: number;
    top_p?: number;
    frequency_penalty?: number;
    presence_penalty?: number;
    [key: string]: any;      // Other model-specific parameters
  };
  metadata?: {
    releaseDate?: string;    // When the model was released
    version?: string;        // Model version if applicable
    deprecated?: boolean;    // Whether model is deprecated
    tags?: string[];        // Additional categorization
  };
}
```

## Implementation Plan

### Phase 1: Registry Setup
1. ✅ Create model registry structure:
   - ✅ Define provider configuration schema
   - ✅ Create provider-specific model definitions
   - ✅ Set up type definitions for models and capabilities
   - ✅ Add human-readable names for models

2. ✅ Extend existing Model interface with:
   - ✅ Capability flags (text, vision, audio, etc.)
   - ✅ Model specifications (context window, token limits)
   - ✅ Default parameters
   - ✅ Metadata (version, deprecation status)
   - ✅ Human-readable display names

3. ✅ Create initial provider configurations for:
   - ✅ OpenAI models
   - ✅ Anthropic models
   - ✅ Azure deployments
   - ✅ Custom providers

### Phase 2: Model Management Service
1. ✅ Develop ModelRegistryService to:
   - ✅ Load and validate provider configurations
   - ✅ Manage model availability based on API keys
   - ✅ Filter models by capabilities
   - ✅ Handle provider-specific model validation

2. ✅ Integrate with existing systems:
   - ✅ Update model management in Models.svelte
   - ✅ Enhance API key validation
   - ✅ Add capability-based model filtering

3. ✅ Improve ApiKeyManager functionality:
   - ✅ Auto-detect available models on key addition
   - ✅ Validate provider-specific authentication
   - ✅ Track provider connection status

### Phase 3: UI Enhancements
1. ✅ Update model selector in ChatControls:
   - ✅ Add capability indicators (icons for vision, audio, reasoning)
   - ✅ Group models by provider
   - ✅ Show model specifications in tooltips
   - ✅ Display human-readable model names

2. ✅ Enhance model information display:
   - ✅ Add context window information
   - ✅ Show available capabilities
   - ✅ Align capability icons to the right
   - ✅ Indicate model status with icons

3. ✅ Improve model selection UX:
   - ✅ Filter models by capability
   - ✅ Sort by provider and model name
   - ✅ Use technical model IDs for API calls while showing friendly names

## Security Considerations
- API keys remain client-side
- Model availability checks performed locally
- No external requests until model is actually used

## Implementation Details

### Feature-Based Structure
The model registry has been implemented following a feature-based structure to improve maintainability:

```
src/lib/models/
├── index.ts                  # Exports services and registry
├── apiKeyService.ts          # API key management
├── modelService.ts           # Bridge between registry and app
└── registry/
    ├── index.ts              # Exports types and singleton
    ├── models.json           # Model definitions with capabilities
    ├── modelRegistryService.ts # Service to manage models
    ├── providers.json        # Provider configurations
    └── types.ts              # Type definitions
```

### Key Components

1. **ModelRegistryService**: Central service that manages models and providers
   - Loads and validates provider configurations
   - Manages model availability based on API keys
   - Provides filtering by capabilities and providers

2. **ApiKeyService**: Handles API key management
   - Loads, saves, and validates API keys
   - Updates model availability based on keys
   - Integrates with existing API key storage

3. **UI Enhancements**:
   - Categorized model dropdown by provider
   - Added capability icons with tooltips (reasoning, vision, audio)
   - Displayed human-readable names while using technical IDs for API calls
   - Aligned capability icons to the right for better readability
   - Updated Anthropic model IDs to match their API requirements

## Future Enhancements
- Model performance metrics tracking
- Cost estimation based on usage
- Automatic model fallback
- Custom model configurations
- Provider-specific optimizations

## Success Metrics
- Reduced time to add new models
- Improved user experience in model selection
- Increased usage of appropriate models for specific tasks
- Reduced configuration errors
