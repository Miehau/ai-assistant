<script>
    import { cn } from "$lib/utils.js";
  
    let isFocused = false;
  
    function handleFocus() {
      isFocused = true;
    }
  
    function handleBlur() {
      isFocused = false;
    }
    
  </script>
  
  <div class="flex justify-center items-center h-screen">
    <input
      class={cn(
        "transition-all duration-300 ease-in-out border border-gray-300 rounded-md shadow-sm",
        isFocused ? "w-72 p-4 text-lg" : "w-48 p-2 text-base"
      )}
      on:focus={handleFocus}
      on:blur={handleBlur}
      placeholder="Click to enlarge"
    />
  </div>