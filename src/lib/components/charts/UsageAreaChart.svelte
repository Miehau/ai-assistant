<script lang="ts">
  import { Chart, Svg, Area, Axis, Highlight, Tooltip } from 'layerchart';
  import { scaleUtc, scaleLinear } from 'd3-scale';
  import { curveMonotoneX } from 'd3-shape';

  interface ChartData {
    date: Date;
    cost: number;
  }

  interface Props {
    data: ChartData[];
    height?: number;
  }

  let { data, height = 200 }: Props = $props();

  const xScale = $derived(
    scaleUtc()
      .domain([
        Math.min(...data.map(d => d.date.getTime())),
        Math.max(...data.map(d => d.date.getTime()))
      ])
  );

  const yScale = $derived(
    scaleLinear()
      .domain([0, Math.max(...data.map(d => d.cost)) * 1.1])
      .nice()
  );

  function formatDate(date: Date): string {
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  }

  function formatCost(value: number): string {
    return `$${value.toFixed(4)}`;
  }
</script>

<div class="w-full" style="height: {height}px;">
  {#if data.length > 0}
    <Chart
      {data}
      x="date"
      xScale={xScale}
      y="cost"
      yScale={yScale}
      padding={{ left: 48, bottom: 24, right: 8, top: 8 }}
    >
      <Svg>
        <Axis placement="left" class="text-foreground [&_.grid]:stroke-muted/30 [&_.rule]:stroke-muted" grid rule format={(d: number) => formatCost(d)} />
        <Axis placement="bottom" class="text-foreground [&_.rule]:stroke-muted" format={(d: Date) => formatDate(d)} />
        <defs>
          <linearGradient id="area-gradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" class="[stop-color:hsl(var(--primary))]" stop-opacity="0.5" />
            <stop offset="100%" class="[stop-color:hsl(var(--primary))]" stop-opacity="0.05" />
          </linearGradient>
        </defs>
        <Area line={{ class: 'stroke-2 stroke-primary' }} fill="url(#area-gradient)" curve={curveMonotoneX} />
        <Highlight points={{ class: 'fill-primary' }} lines />
      </Svg>
      <Tooltip.Root let:data>
        {#if data}
          <Tooltip.Header>{formatDate(data.date)}</Tooltip.Header>
          <Tooltip.List>
            <Tooltip.Item label="Cost" value={formatCost(data.cost)} />
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
