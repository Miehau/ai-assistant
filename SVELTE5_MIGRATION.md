# Migracja do Svelte 5.42.2

## Wykonane zmiany

### Zaktualizowane zależności:
- **svelte**: 4.2.7 → 5.42.2
- **@sveltejs/kit**: latest → 2.48.0
- **@sveltejs/vite-plugin-svelte**: 3.0.0 → 6.2.1
- **svelte-check**: 3.6.0 → 4.3.3
- **bits-ui**: 0.21.16 → 2.14.1
- **formsnap**: 1.0.1 → 2.0.1
- **vite**: 5.0.3 → 6.4.1

### Zmiany w konfiguracji:

1. **vite.config.js**:
   - Dodano `optimizeDeps.exclude: ['bits-ui']` - wyłączenie optymalizacji bits-ui przez Vite
   - Dodano `ssr.noExternal: ['bits-ui']` - wymuszenie bundlowania bits-ui w SSR

2. **svelte.config.js**:
   - Zaktualizowano `vitePreprocess` z dodatkowymi opcjami
   - Skonfigurowano `adapter-static` z `fallback: 'index.html'` dla SPA

3. **src/routes/+layout.ts**:
   - Zmieniono `prerender: true` → `prerender: false`
   - Zachowano `ssr: false` dla aplikacji Tauri

4. **src/lib/components/ui/button/index.ts**:
   - Zaktualizowano typy z `ButtonPrimitive.Props` → `ButtonPrimitive.RootProps`
   - Zmieniono `ButtonPrimitive.Events` → `Record<string, never>` (bits-ui 2.x nie eksportuje Events)

## Status

### ✅ Działa:
- Serwer deweloperski (`npm run dev:web`)
- Hot Module Replacement (HMR)
- Wszystkie komponenty UI (bits-ui 2.x)
- TypeScript
- Routing

### ⚠️ Znane problemy:

**Production build (SSR) - Parser Error**:
```
[vite-plugin-svelte:compile] Unexpected token at position 1:25
```

**Przyczyna**: Bug w Svelte 5.42.2 kompilatora podczas budowania SSR bundle. Kompilator wydaje się usuwać nowe linie z kodu podczas parsowania, co powoduje błędy składni.

**Workaround**: 
- Aplikacja jest Tauri (desktop app), więc SSR nie jest wymagany
- Dev mode działa poprawnie
- Możliwe rozwiązanie: czekać na fix w Svelte lub użyć starszej wersji Svelte 5.x

## Wymagania

- **Node.js**: v22.x (wymagane dla @sveltejs/vite-plugin-svelte 6.x z powodu `styleText` w `node:util`)
- **Bun**: 1.3.0+ (dla dev dependencies)

## Uruchomienie

```bash
# Używając Node 22 (przez nvm)
nvm use 22
npm run dev:web

# Alternatywnie z Bun (używa Bun runtime)
bun run dev:web
```

## Tryb kompatybilności Svelte 5

Svelte 5 działa w trybie kompatybilności wstecznej:
- Komponenty mogą używać starej składni (`export let`, `$$props`, `$$restProps`)
- Komponenty z bits-ui używają nowej składni (runes: `$props()`, `$state()`)
- Oba tryby współistnieją bez problemów

## Następne kroki

1. Monitorować Svelte 5.x releases dla fix'a SSR parser bug
2. Opcjonalnie: zmigrować komponenty do runes syntax
3. Rozważyć aktualizację innych UI dependencies do Svelte 5 compatible versions
