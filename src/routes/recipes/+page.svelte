<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as Card from "$lib/components/ui/card";
  import * as Dialog from "$lib/components/ui/dialog";
  import MainLayout from "$lib/components/MainLayout.svelte";
  import { config } from "$lib/config";

  interface Ingredient {
    name: string;
    unit: string;
  }

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

  let recipes: Recipe[] = [
    {
      id: "1",
      title: "Spaghetti Carbonara",
      image: "https://example.com/carbonara.jpg",
      ingredients: [
        { name: "Spaghetti", amount: "400", unit: "g" },
        { name: "Pancetta", amount: "150", unit: "g" },
        { name: "Eggs", amount: "3", unit: "large" },
        { name: "Parmesan cheese", amount: "50", unit: "g" },
      ],
      method: ["Boil pasta", "Fry pancetta", "Mix eggs and cheese", "Combine all ingredients"],
      tags: ["Italian", "Pasta", "Quick"]
    },
    {
      id: "2",
      title: "Chicken Stir Fry",
      image: "https://example.com/stir-fry.jpg",
      ingredients: [
        { name: "Chicken breast", amount: "500", unit: "g" },
        { name: "Mixed vegetables", amount: "400", unit: "g" },
        { name: "Soy sauce", amount: "2", unit: "tbsp" },
        { name: "Vegetable oil", amount: "1", unit: "tbsp" },
      ],
      method: ["Cut chicken", "Prepare vegetables", "Heat oil in wok", "Stir fry chicken and vegetables", "Add soy sauce"],
      tags: ["Asian", "Chicken", "Healthy"]
    },
  ];

  let isInputFocused = false;
  let inputValue = "";
  let isLoading = false;
  let selectedRecipe: Recipe | null = null;

  function handleFocus() {
    isInputFocused = true;
  }

  function handleBlur() {
    isInputFocused = false;
  }

  function openRecipeModal(recipe: Recipe) {
    selectedRecipe = recipe;
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
            tags: newRecipe.tags || []
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
      <div class="grid gap-6 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {#each recipes as recipe}
          <Card.Root on:click={() => openRecipeModal(recipe)} class="cursor-pointer hover:shadow-lg transition-shadow duration-300">
            <Card.Header class="p-0">
              <img src={recipe.image} alt={recipe.title} class="h-48 w-full object-cover" />
            </Card.Header>
            <Card.Content class="p-4">
              <Card.Title class="text-2xl mb-3 text-right">{recipe.title}</Card.Title>
              <div class="mt-2 flex flex-wrap justify-end gap-2">
                {#each recipe.tags as tag}
                  <span class="rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold text-primary-foreground">
                    {tag}
                  </span>
                {/each}
              </div>
            </Card.Content>
          </Card.Root>
        {/each}
      </div>
    </div>
  </div>

  <Dialog.Root open={!!selectedRecipe} onOpenChange={closeRecipeModal}>
    <Dialog.Content class="sm:max-w-[600px]">
      <Dialog.Header>
        <Dialog.Title>{selectedRecipe?.title}</Dialog.Title>
        <Dialog.Description>
          Recipe details
        </Dialog.Description>
      </Dialog.Header>
      <div class="grid gap-4 py-4">
        <img src={selectedRecipe?.image} alt={selectedRecipe?.title} class="w-full h-48 object-cover rounded-md" />
        <div class="grid grid-cols-2 gap-4">
          <div>
            <h3 class="font-semibold mb-2">Ingredients:</h3>
            <ul class="list-disc list-inside">
              {#each selectedRecipe?.ingredients || [] as ingredient}
                <li>{ingredient.amount} {ingredient.unit} {ingredient.name}</li>
              {/each}
            </ul>
          </div>
          <div>
            <h3 class="font-semibold mb-2">Method:</h3>
            <ol class="list-decimal list-inside">
              {#each selectedRecipe?.method || [] as step}
                <li>{step}</li>
              {/each}
            </ol>
          </div>
        </div>
        <div>
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
      <Dialog.Footer>
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
