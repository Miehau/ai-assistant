<script lang="ts">
  import { Chart, Svg, Spline, Axis, Highlight, Tooltip, Points } from 'layerchart';
  import { scaleUtc, scaleLinear } from 'd3-scale';
  import { curveMonotoneX } from 'd3-shape';

  interface ChartData {
    date: Date;
    tokens: number;
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
      .domain([0, Math.max(...data.map(d => d.tokens)) * 1.1])
      .nice()
  );

  function formatDate(date: Date): string {
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  }

  function formatTokens(value: number): string {
    if (value >= 1000000) return `${(value / 1000000).toFixed(1)}M`;
    if (value >= 1000) return `${(value / 1000).toFixed(1)}K`;
    return value.toString();
  }
</script>

<div class="w-full" style="height: {height}px;">
  {#if data.length > 0}
    <Chart
      {data}
      x="date"
      xScale={xScale}
      y="tokens"
      yScale={yScale}
      padding={{ left: 48, bottom: 24, right: 8, top: 8 }}
    >
      <Svg>
        <Axis placement="left" class="text-foreground [&_.grid]:stroke-muted/30 [&_.rule]:stroke-muted" grid rule format={(d: number) => formatTokens(d)} />
        <Axis placement="bottom" class="text-foreground [&_.rule]:stroke-muted" format={(d: Date) => formatDate(d)} />
        <Spline class="stroke-2 stroke-amber-500" curve={curveMonotoneX} />
        <Points class="fill-amber-500" r={3} />
        <Highlight points={{ class: 'fill-amber-500' }} lines />
      </Svg>
      <Tooltip.Root let:data>
        {#if data}
          <Tooltip.Header>{formatDate(data.date)}</Tooltip.Header>
          <Tooltip.List>
            <Tooltip.Item label="Tokens" value={data.tokens.toLocaleString()} />
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
