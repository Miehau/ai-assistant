<script lang="ts">
  import { Chart, Svg, Axis, Tooltip } from 'layerchart';
  import { scaleBand, scaleLinear, scaleOrdinal } from 'd3-scale';
  import { stack, stackOffsetNone, type Series } from 'd3-shape';
  import type { DailyModelUsage } from '$lib/types';

  interface Props {
    data: DailyModelUsage[];
    height?: number;
  }

  let { data, height = 300 }: Props = $props();

  // Define color palette for models
  const colorPalette = [
    '#22d3ee', // cyan
    '#a855f7', // purple
    '#52b788', // primary green
    '#fbbf24', // amber
    '#f472b6', // pink
    '#60a5fa', // blue
    '#34d399', // emerald
    '#f97316', // orange
  ];

  // Get unique models and dates
  const models = $derived([...new Set(data.map(d => d.model_name))]);
  const dates = $derived([...new Set(data.map(d => d.date))].sort());

  interface StackedRecord {
    date: string;
    [key: string]: string | number;
  }

  // Transform data for stacking: { date: string, [model]: cost }[]
  const stackedData = $derived(() => {
    const grouped = new Map<string, StackedRecord>();

    for (const date of dates) {
      grouped.set(date, { date });
    }

    for (const item of data) {
      const record = grouped.get(item.date);
      if (record) {
        record[item.model_name] = item.total_cost;
      }
    }

    return Array.from(grouped.values());
  });

  // Create stacked series
  const series = $derived(() => {
    if (stackedData().length === 0 || models.length === 0) return [] as Series<StackedRecord, string>[];
    const stackGenerator = stack<StackedRecord, string>()
      .keys(models)
      .value((d: StackedRecord, key: string) => (d[key] as number) || 0)
      .offset(stackOffsetNone);
    return stackGenerator(stackedData());
  });

  // Flatten series for bars
  const flattenedData = $derived(() => {
    const result: Array<{ date: string; model: string; y0: number; y1: number; value: number }> = [];
    for (const s of series()) {
      const modelName = s.key;
      for (const point of s) {
        result.push({
          date: point.data.date as string,
          model: modelName,
          y0: point[0],
          y1: point[1],
          value: point[1] - point[0],
        });
      }
    }
    return result;
  });

  const xScale = $derived(
    scaleBand<string>()
      .domain(dates)
      .padding(0.2)
  );

  const yMax = $derived(() => {
    if (flattenedData().length === 0) return 1;
    return Math.max(...flattenedData().map(d => d.y1)) * 1.1;
  });

  const yScale = $derived(
    scaleLinear()
      .domain([0, yMax()])
      .nice()
  );

  const colorScale = $derived(
    scaleOrdinal<string, string>()
      .domain(models)
      .range(colorPalette.slice(0, models.length))
  );

  function formatDate(date: string): string {
    const d = new Date(date);
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  }

  function formatCost(value: number): string {
    return `$${value.toFixed(4)}`;
  }

  function shortenModelName(name: string): string {
    // Shorten common model prefixes
    return name
      .replace('gpt-4-turbo', 'GPT-4T')
      .replace('gpt-4o-mini', 'GPT-4o-m')
      .replace('gpt-4o', 'GPT-4o')
      .replace('gpt-4', 'GPT-4')
      .replace('gpt-3.5-turbo', 'GPT-3.5')
      .replace('claude-3-opus', 'C3-Opus')
      .replace('claude-3-sonnet', 'C3-Sonnet')
      .replace('claude-3-haiku', 'C3-Haiku')
      .replace('claude-3.5-sonnet', 'C3.5-S')
      .replace('claude-sonnet-4', 'C4-Sonnet')
      .replace('claude-opus-4', 'C4-Opus');
  }
</script>

<div class="w-full" style="height: {height}px;">
  {#if flattenedData().length > 0}
    <!-- Legend -->
    <div class="flex flex-wrap gap-3 mb-4 px-2">
      {#each models as model}
        <div class="flex items-center gap-1.5 text-xs">
          <div
            class="w-3 h-3 rounded-sm"
            style="background-color: {colorScale(model)}"
          ></div>
          <span class="text-muted-foreground">{shortenModelName(model)}</span>
        </div>
      {/each}
    </div>

    <Chart
      data={flattenedData()}
      x="date"
      xScale={xScale}
      y="y1"
      yScale={yScale}
      padding={{ left: 56, bottom: 32, right: 8, top: 8 }}
    >
      <Svg>
        <Axis placement="left" class="text-foreground [&_.grid]:stroke-muted/30 [&_.rule]:stroke-muted" grid rule format={(d: number) => formatCost(d)} />
        <Axis placement="bottom" class="text-foreground [&_.rule]:stroke-muted" format={(d: string) => formatDate(d)} />
        {#each flattenedData() as bar}
          <rect
            x={xScale(bar.date)}
            y={yScale(bar.y1)}
            width={xScale.bandwidth()}
            height={Math.max(0, yScale(bar.y0) - yScale(bar.y1))}
            fill={colorScale(bar.model)}
            class="transition-opacity hover:opacity-80"
          />
        {/each}
      </Svg>
      <Tooltip.Root let:data>
        {#if data}
          <Tooltip.Header>{formatDate(data.date)}</Tooltip.Header>
          <Tooltip.List>
            <Tooltip.Item
              label={shortenModelName(data.model)}
              value={formatCost(data.value)}
            />
          </Tooltip.List>
        {/if}
      </Tooltip.Root>
    </Chart>
  {:else}
    <div class="flex items-center justify-center h-full text-muted-foreground">
      No data available
    </div>
  {/if}
</div>
