<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Card, CardHeader, CardContent, CardTitle } from "$lib/components/ui/card";
  import * as Dialog from "$lib/components/ui/dialog";
  import MainLayout from "$lib/components/MainLayout.svelte";
  import { config } from "$lib/config";

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

  let recipes: Recipe[] = [];
  let isInputFocused = false;
  let inputValue = "";
  let isLoading = false;
  let selectedRecipe: Recipe | null = null;
  let isModalLoading = false;
  let isInitialLoading = true;
  let error: string | null = null;

  onMount(async () => {
    await fetchAllRecipes();
  });

  async function fetchAllRecipes() {
    try {
      const response = await fetch(`${config.apiUrl}/api/recipes/`);
      if (!response.ok) {
        throw new Error('Failed to fetch recipes');
      }
      const fetchedRecipes = await response.json();
      console.log(fetchedRecipes);
      if (Array.isArray(fetchedRecipes)) {
        recipes = fetchedRecipes;
      } else {
        throw new Error('Fetched data is not an array');
      }
    } catch (error) {
      console.error('Error fetching recipes:', error);
      error = 'Failed to load recipes. Please try again later.';
    } finally {
      isInitialLoading = false;
    }
  }

  function handleFocus() {
    isInputFocused = true;
  }

  function handleBlur() {
    isInputFocused = false;
  }

  async function fetchRecipeDetails(recipeId: string): Promise<Recipe> {
    const response = await fetch(`${config.apiUrl}/api/recipes/${recipeId}`);
    if (!response.ok) {
      throw new Error('Failed to fetch recipe details');
    }
    return await response.json();
  }

  async function openRecipeModal(recipe: Recipe) {
    isModalLoading = true;
    try {
      const fullRecipe = await fetchRecipeDetails(recipe.id);
      selectedRecipe = fullRecipe;
    } catch (error) {
      console.error('Error fetching recipe details:', error);
      // Fallback to using the basic recipe info if fetch fails
      selectedRecipe = recipe;
    } finally {
      isModalLoading = false;
    }
  }

  function closeRecipeModal() {
    selectedRecipe = null;
  }

  async function handleSubmit(event: Event) {
    event.preventDefault();
    if (inputValue.trim()) {
      isLoading = true;
      try {
        const response = await fetch(`${config.apiUrl}/api/recipes`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ recipe: inputValue }),
        });

        if (response.ok) {
          const newRecipe = await response.json();
          const imageUrl = URL.createObjectURL(new Blob([newRecipe.image], { type: 'image/jpeg' }));
          
          recipes = [...recipes, {
            id: newRecipe.id,
            title: newRecipe.title,
            image: imageUrl,
            method: newRecipe.method,
            tags: newRecipe.tags || [],
            ingredients: newRecipe.ingredients || []
          }];

          console.log('Recipe submitted successfully');
        } else {
          console.error('Failed to submit recipe');
        }
      } catch (error) {
        console.error('Error submitting recipe:', error);
      } finally {
        isLoading = false;
        inputValue = ""; // Clear the input after processing (success or failure)
      }
    }
  }
</script>

<MainLayout>
  <div class="container mx-auto py-8">
    <form on:submit={handleSubmit} class="mb-8 flex justify-center relative input-wrapper" class:focused={isInputFocused}>
      <div class="relative w-full max-w-xl">
        <div class:pr-12={isLoading} class="w-full">
          <Input
            type="text"
            placeholder="Add a new recipe..."
            class="w-full rounded-full transition-all duration-300 pr-10"
            on:focus={handleFocus}
            on:blur={handleBlur}
            bind:value={inputValue}
            disabled={isLoading}
          />
        </div>
        {#if isLoading}
          <div class="spinner absolute right-4 top-1/2 transform -translate-y-1/2"></div>
        {/if}
      </div>
    </form>

    <div class="content" class:blurred={isInputFocused}>
      {#if isInitialLoading}
        <div class="flex justify-center items-center h-64">
          <div class="spinner"></div>
        </div>
      {:else if error}
        <p class="text-center text-red-500">{error}</p>
      {:else if !recipes || recipes.length === 0}
        <p class="text-center text-gray-500">No recipes available. Add a new recipe to get started!</p>
      {:else}
        <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {#each recipes as recipe (recipe.id)}
            <Card on:click={() => openRecipeModal(recipe)} class="cursor-pointer hover:shadow-lg transition-shadow duration-300">
              <CardHeader class="p-0">
                <img src={recipe.image} alt={recipe.title} class="h-48 w-full object-cover" />
              </CardHeader>
              <CardContent class="p-4">
                <CardTitle class="text-2xl mb-3 text-right">{recipe.title}</CardTitle>
                <div class="mt-2 flex flex-wrap justify-end gap-2">
                  {#if recipe.tags && recipe.tags.length > 0}
                    {#each recipe.tags as tag}
                      <span class="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold text-primary-foreground">
                        {tag}
                      </span>
                    {/each}
                  {/if}
                </div>
              </CardContent>
            </Card>
          {/each}
        </div>
      {/if}
    </div>
  </div>

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
          <div class="grid grid-cols-1 md:grid-cols-2 gap-4 flex-grow overflow-hidden">
            <div class="overflow-hidden flex flex-col">
              <h3 class="font-semibold mb-2">Ingredients:</h3>
              <div class="overflow-y-auto flex-grow pr-2">
                <ul class="list-disc list-inside">
                  {#each selectedRecipe?.ingredients || [] as ingredient}
                    <li class="mb-1">{ingredient.amount} {ingredient.unit} {ingredient.name}</li>
                  {/each}
                </ul>
              </div>
            </div>
            <div class="overflow-hidden flex flex-col">
              <h3 class="font-semibold mb-2">Method:</h3>
              <div class="overflow-y-auto flex-grow pr-2">
                <ol class="list-decimal list-inside">
                  {#each selectedRecipe?.method || [] as step}
                    <li class="mb-2">{step}</li>
                  {/each}
                </ol>
              </div>
            </div>
          </div>
          <div class="mt-4">
            <h3 class="font-semibold mb-2">Tags:</h3>
            <div class="flex flex-wrap gap-2">
              {#each selectedRecipe?.tags || [] as tag}
                <span class="rounded-full bg-primary px-2 py-0.5 text-xs font-semibold text-primary-foreground">
                  {tag}
                </span>
              {/each}
            </div>
          </div>
        </div>
      {/if}
      <Dialog.Footer class="mt-4">
        <Button on:click={closeRecipeModal}>Close</Button>
      </Dialog.Footer>
    </Dialog.Content>
  </Dialog.Root>
</MainLayout>

<style>
  .input-wrapper {
    z-index: 10;
    transition: all 0.3s ease-in-out;
  }

  .input-wrapper.focused :global(input) {
    transform: scale(1.2);
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  }

  .content {
    transition: filter 0.3s ease-in-out;
  }

  .content.blurred {
    filter: blur(5px);
  }

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
