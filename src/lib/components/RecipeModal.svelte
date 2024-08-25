<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";

  export let selectedRecipe: Recipe | null = null;
  export let isModalLoading = false;
  export let closeRecipeModal: () => void;

  interface Ingredient {
    name: string;
    amount: string;
    unit: string;
  }

  interface Recipe {
    id: string;
    title: string;
    image: string;
    ingredients: Ingredient[];
    method: string[];
    tags: string[];
  }
</script>

<Dialog.Root open={!!selectedRecipe} onOpenChange={closeRecipeModal}>
  <Dialog.Content class="sm:max-w-[600px] max-h-[90vh] flex flex-col">
    <Dialog.Header>
      <Dialog.Title>{selectedRecipe?.title}</Dialog.Title>
      <Dialog.Description>
        Recipe details
      </Dialog.Description>
    </Dialog.Header>
    {#if isModalLoading}
      <div class="flex justify-center items-center flex-grow">
        <div class="spinner"></div>
      </div>
    {:else}
      <div class="flex-grow overflow-hidden flex flex-col">
        <img src={selectedRecipe?.image} alt={selectedRecipe?.title} class="w-full h-48 object-cover rounded-md mb-4" />
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 flex-grow">
          <div class="flex flex-col">
            <h3 class="font-semibold mb-2">Ingredients:</h3>
            <div class="overflow-y-auto pr-2 flex-grow" style="max-height: 300px;">
              <ul class="list-disc list-inside pb-4">
                {#each selectedRecipe?.ingredients || [] as ingredient}
                  <li class="mb-1">{ingredient.amount} {ingredient.unit} {ingredient.name}</li>
                {/each}
              </ul>
            </div>
          </div>
          <div class="flex flex-col">
            <h3 class="font-semibold mb-2">Method:</h3>
            <div class="overflow-y-auto pr-2 flex-grow" style="max-height: 300px;">
              <ol class="list-decimal list-inside pb-4">
                {#each selectedRecipe?.method || [] as step}
                  <li class="mb-2">{step}</li>
                {/each}
              </ol>
            </div>
          </div>
        </div>
      </div>
    {/if}
    <Dialog.Footer class="sm:mt-4">
      <Button on:click={closeRecipeModal}>Close</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<style>
  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid #f3f3f3;
    border-top: 2px solid #3498db;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
</style>
