<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import type { UsageStatistics } from '$lib/types';
  import { Button } from '$lib/components/ui/button';
  import { Download, TrendingUp, DollarSign, Zap } from 'lucide-svelte';
  import MainLayout from '$lib/components/MainLayout.svelte';
  import UsageAreaChart from '$lib/components/charts/UsageAreaChart.svelte';
  import UsageLineChart from '$lib/components/charts/UsageLineChart.svelte';
  import ModelStackedBarChart from '$lib/components/charts/ModelStackedBarChart.svelte';

  let statistics = $state<UsageStatistics | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let dateRange = $state<'7d' | '30d' | 'all'>('30d');

  async function loadStatistics() {
    loading = true;
    error = null;

    try {
      let startDate: number | undefined;
      const endDate = Math.floor(Date.now() / 1000); // Current time in seconds

      if (dateRange === '7d') {
        startDate = Math.floor((Date.now() - 7 * 24 * 60 * 60 * 1000) / 1000);
      } else if (dateRange === '30d') {
        startDate = Math.floor((Date.now() - 30 * 24 * 60 * 60 * 1000) / 1000);
      }

      const stats = await invoke<UsageStatistics>('get_usage_statistics', {
        startDate,
        endDate
      });

      statistics = stats;
    } catch (e) {
      console.error('Failed to load usage statistics:', e);
      error = e instanceof Error ? e.message : 'Failed to load statistics';
    } finally {
      loading = false;
    }
  }

  function exportData(format: 'json' | 'csv') {
    if (!statistics) return;

    if (format === 'json') {
      const dataStr = JSON.stringify(statistics, null, 2);
      const blob = new Blob([dataStr], { type: 'application/json' });
      downloadBlob(blob, 'usage-statistics.json');
    } else {
      // CSV export
      const rows: string[] = [];
      rows.push('Model,Messages,Tokens,Cost');
      statistics.by_model.forEach(model => {
        rows.push(`${model.model_name},${model.message_count},${model.total_tokens},${model.total_cost.toFixed(4)}`);
      });
      const csvContent = rows.join('\n');
      const blob = new Blob([csvContent], { type: 'text/csv' });
      downloadBlob(blob, 'usage-statistics.csv');
    }
  }

  function downloadBlob(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  function getMaxCost(): number {
    if (!statistics?.by_model.length) return 1;
    return Math.max(...statistics.by_model.map(m => m.total_cost));
  }

  function getMaxTokens(): number {
    if (!statistics?.by_model.length) return 1;
    return Math.max(...statistics.by_model.map(m => m.total_tokens));
  }

  // Transform data for charts
  const costChartData = $derived(
    statistics?.by_date
      .map(d => ({
        date: new Date(d.date),
        cost: d.total_cost,
      }))
      .sort((a, b) => a.date.getTime() - b.date.getTime()) ?? []
  );

  const tokenChartData = $derived(
    statistics?.by_date
      .map(d => ({
        date: new Date(d.date),
        tokens: d.total_tokens,
      }))
      .sort((a, b) => a.date.getTime() - b.date.getTime()) ?? []
  );

  onMount(() => {
    loadStatistics();
  });

  $effect(() => {
    loadStatistics();
  });
</script>

<svelte:head>
  <title>Usage Statistics</title>
</svelte:head>

<MainLayout>
  <div class="container mx-auto p-6 max-w-7xl">
    <!-- Header -->
    <div class="mb-8">
      <h1 class="text-3xl font-bold mb-2">Usage Statistics</h1>
      <p class="text-muted-foreground">Track your AI model usage, costs, and token consumption</p>
    </div>

    <!-- Time Range Selector -->
    <div class="flex gap-2 mb-6">
      <Button
        variant={dateRange === '7d' ? 'default' : 'outline'}
        size="sm"
        onclick={() => dateRange = '7d'}
        class={dateRange === '7d' ? 'gradient-primary' : ''}
      >
        Last 7 Days
      </Button>
      <Button
        variant={dateRange === '30d' ? 'default' : 'outline'}
        size="sm"
        onclick={() => dateRange = '30d'}
        class={dateRange === '30d' ? 'gradient-primary' : ''}
      >
        Last 30 Days
      </Button>
      <Button
        variant={dateRange === 'all' ? 'default' : 'outline'}
        size="sm"
        onclick={() => dateRange = 'all'}
        class={dateRange === 'all' ? 'gradient-primary' : ''}
      >
        All Time
      </Button>

      <div class="ml-auto flex gap-2">
        <Button variant="outline" size="sm" onclick={() => exportData('json')}>
          <Download class="size-4 mr-2" />
          Export JSON
        </Button>
        <Button variant="outline" size="sm" onclick={() => exportData('csv')}>
          <Download class="size-4 mr-2" />
          Export CSV
        </Button>
      </div>
    </div>

    {#if loading}
      <div class="flex items-center justify-center py-12">
        <div class="loading-spinner"></div>
        <p class="ml-4 text-muted-foreground">Loading statistics...</p>
      </div>
    {:else if error}
      <div class="glass-panel p-6 border-destructive">
        <p class="text-destructive">Error: {error}</p>
        <Button onclick={loadStatistics} class="mt-4" variant="outline">Retry</Button>
      </div>
    {:else if statistics}
      <!-- Summary Cards -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-8">
        <!-- Total Cost Card -->
        <div class="glass-panel p-6">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-muted-foreground">Total Cost</h3>
            <DollarSign class="size-4 text-primary" />
          </div>
          <p class="text-3xl font-bold">${statistics.total_cost.toFixed(4)}</p>
          <p class="text-xs text-muted-foreground mt-2">{statistics.total_messages} messages</p>
        </div>

        <!-- Total Tokens Card -->
        <div class="glass-panel p-6">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-muted-foreground">Total Tokens</h3>
            <Zap class="size-4 text-amber-500" />
          </div>
          <p class="text-3xl font-bold">{statistics.total_tokens.toLocaleString()}</p>
          <p class="text-xs text-muted-foreground mt-2">
            {Math.round(statistics.total_tokens / Math.max(statistics.total_messages, 1))} avg/message
          </p>
        </div>

        <!-- Avg Cost Per Message Card -->
        <div class="glass-panel p-6">
          <div class="flex items-center justify-between mb-2">
            <h3 class="text-sm font-medium text-muted-foreground">Avg Cost/Message</h3>
            <TrendingUp class="size-4 text-cyan-500" />
          </div>
          <p class="text-3xl font-bold">
            ${(statistics.total_cost / Math.max(statistics.total_messages, 1)).toFixed(4)}
          </p>
          <p class="text-xs text-muted-foreground mt-2">Per interaction</p>
        </div>
      </div>

      <!-- Cost by Model -->
      <div class="glass-panel p-6 mb-8">
        <h2 class="text-xl font-semibold mb-4">Cost by Model</h2>
        {#if statistics.by_model.length > 0}
          <div class="space-y-4">
            {#each statistics.by_model as model}
              {@const costPercentage = (model.total_cost / getMaxCost()) * 100}
              {@const tokenPercentage = (model.total_tokens / getMaxTokens()) * 100}

              <div>
                <div class="flex justify-between items-center mb-2">
                  <div>
                    <p class="font-medium">{model.model_name}</p>
                    <p class="text-xs text-muted-foreground">
                      {model.message_count} messages · {model.total_tokens.toLocaleString()} tokens
                    </p>
                  </div>
                  <p class="font-bold text-primary">${model.total_cost.toFixed(4)}</p>
                </div>

                <!-- Cost bar -->
                <div class="h-2 bg-muted rounded-full overflow-hidden mb-1">
                  <div
                    class="h-full gradient-cyan transition-all duration-300"
                    style="width: {costPercentage}%"
                  ></div>
                </div>

                <!-- Token bar -->
                <div class="h-2 bg-muted rounded-full overflow-hidden">
                  <div
                    class="h-full gradient-amber transition-all duration-300"
                    style="width: {tokenPercentage}%"
                  ></div>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-muted-foreground text-center py-8">No usage data yet</p>
        {/if}
      </div>

      <!-- Cost Over Time Chart -->
      {#if costChartData.length > 0}
        <div class="glass-panel p-6 mb-8">
          <h2 class="text-xl font-semibold mb-4">Cost Over Time</h2>
          <UsageAreaChart data={costChartData} height={250} />
        </div>
      {/if}

      <!-- Token Usage Over Time Chart -->
      {#if tokenChartData.length > 0}
        <div class="glass-panel p-6 mb-8">
          <h2 class="text-xl font-semibold mb-4">Token Usage Over Time</h2>
          <UsageLineChart data={tokenChartData} height={250} />
        </div>
      {/if}

      <!-- Cost by Model Over Time (Stacked Bar Chart) -->
      {#if statistics.by_model_date && statistics.by_model_date.length > 0}
        <div class="glass-panel p-6 mb-8">
          <h2 class="text-xl font-semibold mb-4">Cost by Model Over Time</h2>
          <ModelStackedBarChart data={statistics.by_model_date} height={350} />
        </div>
      {/if}

      <!-- Daily Usage Table (collapsed by default) -->
      {#if statistics.by_date.length > 0}
        <details class="glass-panel p-6">
          <summary class="text-xl font-semibold mb-4 cursor-pointer hover:text-primary transition-colors">
            Daily Usage Details
          </summary>
          <div class="space-y-3 mt-4">
            {#each statistics.by_date.slice(0, 14) as daily}
              {@const maxDailyCost = Math.max(...statistics.by_date.map(d => d.total_cost))}
              {@const costPercentage = (daily.total_cost / maxDailyCost) * 100}

              <div>
                <div class="flex justify-between items-center mb-1">
                  <p class="text-sm font-medium">{daily.date}</p>
                  <div class="text-right">
                    <p class="text-sm font-bold">${daily.total_cost.toFixed(4)}</p>
                    <p class="text-xs text-muted-foreground">
                      {daily.message_count} msgs · {daily.total_tokens.toLocaleString()} tokens
                    </p>
                  </div>
                </div>
                <div class="h-2 bg-muted rounded-full overflow-hidden">
                  <div
                    class="h-full gradient-primary transition-all duration-300 glow-green"
                    style="width: {costPercentage}%"
                  ></div>
                </div>
              </div>
            {/each}
          </div>
        </details>
      {/if}
    {:else}
      <div class="glass-panel p-12 text-center">
        <p class="text-muted-foreground mb-4">No usage data available</p>
        <p class="text-sm text-muted-foreground">Start chatting to see your usage statistics here!</p>
      </div>
    {/if}
  </div>
</MainLayout>

<style>
  .loading-spinner {
    width: 24px;
    height: 24px;
    border: 3px solid hsl(var(--muted));
    border-top-color: hsl(var(--primary));
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
