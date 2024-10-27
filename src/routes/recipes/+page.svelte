<script lang="ts">
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Card, CardHeader, CardContent, CardTitle } from "$lib/components/ui/card";
  import MainLayout from "$lib/components/MainLayout.svelte";
  import RecipeModal from "$lib/components/RecipeModal.svelte";
  import AddRecipeModal from "$lib/components/AddRecipeModal.svelte";
  import { config } from "$lib/config";
  import { getAllRecipes, createRecipe, updateRecipe, deleteRecipe } from "$lib/services/api";
  import { localDB } from "$lib/db/pouchdb";

  interface Ingredient {
    id: string;
    name: string;
    amount: number;
  }

  interface MethodStep {
    id: string;
    stepNumber: number;
    description: string;
  }

  interface Recipe {
    _id: string;
    title: string;
    image: string;
    source?: string;
    ingredients: Ingredient[];
    methodSteps: MethodStep[];
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
  let file: File | null = null;
  let isAddRecipeModalOpen = false;

  onMount(async () => {
    await fetchAllRecipes();
    
    if (localDB) {
      localDB.changes({
        since: 'now',
        live: true
      }).on('change', function(change) {
        console.log('Change detected:', change);
        fetchAllRecipes();
      });
    }
  });

  async function fetchAllRecipes() {
    try {
      isInitialLoading = true;
      recipes = await getAllRecipes();
    } catch (err) {
      console.error('Error fetching recipes:', err);
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

  async function openRecipeModal(recipe: Recipe) {
    selectedRecipe = recipe;
    isAddRecipeModalOpen = true;
  }

  function closeRecipeModal() {
    selectedRecipe = null;
  }

  async function handleSubmit(formData: {
    title: string,
    image: string | null,
    ingredientsText: string,
    methodStepsText: string,
    tags: string
  }) {
    const ingredients = formData.ingredientsText.split('\n').map((line, index) => {
      const [name, amount] = line.split(',').map(s => s.trim());
      return { name, amount: parseFloat(amount) || 0 };
    });

    const methodSteps = formData.methodStepsText.split('\n\n').map((step, index) => ({
      description: step.trim(),
      stepNumber: index + 1,
    }));

    const recipeData = {
      _id: selectedRecipe?._id || 'temp_id',
      title: formData.title,
      image: formData.image || '',
      ingredients,
      methodSteps,
      tags: formData.tags.split(',').map(t => t.trim())
    };

    if (selectedRecipe) {
      await handleUpdateRecipe(recipeData as Recipe); // Add type assertion
    } else {
      await handleAddRecipe(recipeData as Recipe); // Add type assertion
    }
    closeAddRecipeModal();
  }

  async function handleFileChange(event: Event) {
    const target = event.target as HTMLInputElement;
    if (target.files) {
      file = target.files[0];
      await submitRecipe(); // Automatically submit when a file is selected
    }
  }

  async function submitRecipe() {
    if (file || inputValue.trim()) {
      isLoading = true;
      try {
        let recipeData: FormData | object;
        if (file) {
          recipeData = new FormData();
          (recipeData as FormData).append('image', file);
          (recipeData as FormData).append('title', inputValue || file.name); // Use file name if no title is provided
        } else if (inputValue.startsWith('http')) {
          recipeData = { source: inputValue };
        } else {
          recipeData = { title: inputValue };
        }

        const newRecipe = await createRecipe(recipeData);
        recipes = [...recipes, newRecipe];
        console.log('Recipe submitted successfully');
      } catch (err) {
        console.error('Error submitting recipe:', err);
        error = 'Failed to submit recipe. Please try again.';
      } finally {
        isLoading = false;
        inputValue = "";
        file = null;
      }
    }
  }

  async function handleUpdateRecipe(updatedRecipe: Recipe) {
    try {
      const updated = await updateRecipe(updatedRecipe._id, updatedRecipe); // Change 'id' to '_id'
      recipes = recipes.map(r => r._id === updated._id ? updated : r); // Change 'id' to '_id'
      closeRecipeModal();
    } catch (err) {
      console.error('Error updating recipe:', err);
      error = 'Failed to update recipe. Please try again.';
    }
  }

  async function handleDeleteRecipe(id: string) {
    try {
      await deleteRecipe(id);
      recipes = recipes.filter(r => r._id !== id); // Change 'id' to '_id'
      closeRecipeModal();
    } catch (err) {
      console.error('Error deleting recipe:', err);
      error = 'Failed to delete recipe. Please try again.';
    }
  }

  function openAddRecipeModal() {
    isAddRecipeModalOpen = true;
  }

  function closeAddRecipeModal() {
    isAddRecipeModalOpen = false;
    selectedRecipe = null;
  }

  async function handleAddRecipe(newRecipe: Recipe) {
    try {
      console.log('New recipe:', newRecipe);
      const createdRecipe = await createRecipe(newRecipe);
      recipes = [...recipes, createdRecipe];
      closeAddRecipeModal();
    } catch (err) {
      console.error('Error adding recipe:', err);
      error = 'Failed to add recipe. Please try again.';
    }
  }

  async function handleInputSubmit(event: KeyboardEvent) {
    if (event.key === 'Enter' && inputValue.trim()) {
      event.preventDefault();
      await submitRecipe();
    }
  }
</script>

<MainLayout>
  <div class="container mx-auto py-8">
    <form on:submit|preventDefault={submitRecipe} class="mb-8 flex justify-center relative input-wrapper" class:focused={isInputFocused}>
      <div class="relative w-full max-w-xl">
        <div class:pr-12={isLoading} class="w-full flex">
          <Input
            type="text"
            placeholder="Add a new recipe or paste a URL..."
            class="w-full rounded-l-full transition-all duration-300"
            on:focus={handleFocus}
            on:blur={handleBlur}
            on:keydown={handleInputSubmit}
            bind:value={inputValue}
            disabled={isLoading}
          />
          <input type="file" id="recipeImage" on:change={handleFileChange} class="hidden" accept="image/*" />
          <label for="recipeImage" class="cursor-pointer bg-primary text-primary-foreground px-4 rounded-r-full flex items-center">
            📷
          </label>
        </div>
        {#if isLoading}
          <div class="spinner absolute right-4 top-1/2 transform -translate-y-1/2"></div>
        {/if}
      </div>
    </form>

    <div class="mb-4 flex justify-end">
      <Button on:click={openAddRecipeModal}>Add New Recipe</Button>
    </div>

    {#if error}
      <p class="text-center text-red-500 mb-4">{error}</p>
    {/if}

    <div class="content" class:blurred={isInputFocused}>
      {#if isInitialLoading}
        <div class="flex justify-center items-center h-64">
          <div class="spinner"></div>
        </div>
      {:else if !recipes || recipes.length === 0}
        <p class="text-center text-gray-500">No recipes available. Add a new recipe to get started!</p>
      {:else}
        <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {#each recipes as recipe (recipe._id)}
            <Card on:click={() => openRecipeModal(recipe)} class="cursor-pointer hover:shadow-lg transition-shadow duration-300">
              <CardHeader class="p-0">
                <img src={`${recipe.image}`} alt={recipe.title} class="h-48 w-full object-cover" />
              </CardHeader>
              <CardContent class="p-4">
                <CardTitle class="text-2xl mb-3 text-right">{recipe.title}</CardTitle>
                <div class="mt-2 flex flex-wrap justify-end gap-2">
                  {#each recipe.tags as tag}
                    <span class="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold text-primary-foreground">
                      {tag}
                    </span>
                  {/each}
                </div>
              </CardContent>
            </Card>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <AddRecipeModal
    isOpen={isAddRecipeModalOpen}
    onClose={closeAddRecipeModal}
    onSubmit={handleSubmit}
    recipe={selectedRecipe}
  />
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
