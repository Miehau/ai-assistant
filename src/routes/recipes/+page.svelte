<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import * as Card from "$lib/components/ui/card";
  import MainLayout from "$lib/components/MainLayout.svelte";
  import { config } from "$lib/config";

  interface Ingredient {
    name: string;
    unit: string;
  }

  interface Recipe {
    id: number;
    title: string;
    image: string;
    method: string[];
    tags: string[];
  }

  let recipes: Recipe[] = [
    {
      id: 1,
      title: "Spaghetti Carbonara",
      image: "https://example.com/carbonara.jpg",
      method: ["Boil pasta", "Mix eggs and cheese"],
      tags: ["Italian", "Pasta", "Quick"]
    },
    {
      id: 2,
      title: "Chicken Stir Fry",
      image: "https://example.com/stir-fry.jpg",
      method: ["Cut chicken", "Stir fry vegetables"],
      tags: ["Asian", "Chicken", "Healthy"]
    },
  ];

  let isInputFocused = false;
  let inputValue = "";
  let isLoading = false;

  function handleFocus() {
    isInputFocused = true;
  }

  function handleBlur() {
    isInputFocused = false;
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
            id: recipes.length + 1,
            title: newRecipe.title,
            image: imageUrl,
            method: newRecipe.method,
            tags: [] // You might want to generate tags based on method
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
      <Card.Root>
        <Card.Header class="p-0">
          <img src={recipe.image} alt={recipe.title} class="h-48 w-full object-cover" />
        </Card.Header>
        <Card.Content class="p-4">
          <Card.Title class="text-2xl mb-3 justify-end">{recipe.title}</Card.Title>
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
