<script lang="ts">
  import { marked } from 'marked';
  import { getContext } from 'svelte';

  const currentSlider = getContext('currentSlider');

  $: settingsMarkdown = `
# Settings

## Model
- **Neural Genesis**: Our fastest model for general use cases.
- **Neural Explorer**: Performance and speed for efficiency.
- **Neural Quantum**: The most powerful model for complex computations.

## Parameters
- **Temperature**: 0.4
- **Top P**: 0.7
- **Top K**: 0.0
${$currentSlider ? `- **Slider**: ${$currentSlider}` : ''}

## Messages
- **Role**: System
- **Content**: You are a...
`;

  $: htmlContent = marked(settingsMarkdown);
</script>

<div class="prose dark:prose-invert max-w-none">
  {@html htmlContent}
</div>
