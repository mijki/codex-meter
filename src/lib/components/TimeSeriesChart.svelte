<script lang="ts" module>
  export interface ChartSeries {
    label: string;
    values: Array<number | null>;
    color: string;
    accuracy: 'reported_exact' | 'derived_exact' | 'estimated';
    dashed?: boolean;
  }

  export interface ChartMarker {
    at: string;
    label: string;
    color: string;
  }
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';
  import type { AlignedData, Options, Series } from 'uplot';

  let {
    title,
    description,
    timestamps,
    series,
    markers = [],
    thresholds = [],
    unit = '%',
  }: {
    title: string;
    description: string;
    timestamps: string[];
    series: ChartSeries[];
    markers?: ChartMarker[];
    thresholds?: number[];
    unit?: string;
  } = $props();

  let host = $state<HTMLDivElement>();
  let empty = $derived(
    timestamps.length < 2 || series.every((item) => item.values.every((v) => v === null)),
  );

  let isTokenSeries = $derived(unit.trim() === 'tokens');
  const formatValue = (value: number | null): string => {
    if (value === null || !Number.isFinite(value)) return '—';
    if (isTokenSeries) {
      return `${new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 2 }).format(value)} tokens`;
    }
    return `${value.toFixed(2)}${unit}`;
  };

  onMount(() => {
    if (empty || !host || host.clientWidth === 0) return;
    const container = host;

    const thresholdSeries: ChartSeries[] = thresholds.map((value) => ({
      label: `${value}% threshold`,
      values: timestamps.map(() => value),
      color: value >= 90 ? '#d35f5f' : value >= 75 ? '#d69a45' : '#657b73',
      accuracy: 'derived_exact',
      dashed: true,
    }));
    const visibleSeries = [...series, ...thresholdSeries];
    const timeSpanSeconds =
      (new Date(timestamps.at(-1) ?? 0).getTime() - new Date(timestamps[0] ?? 0).getTime()) / 1_000;
    const data: AlignedData = [
      timestamps.map((timestamp) => new Date(timestamp).getTime() / 1_000),
      ...visibleSeries.map((item) => item.values),
    ];
    const plotSeries: Series[] = [
      { label: 'Local time' },
      ...visibleSeries.map((item) => ({
        label: item.label,
        stroke: item.color,
        width: item.dashed ? 1 : 2,
        ...(item.dashed ? { dash: [6, 5] } : {}),
        points: {
          show: item.accuracy === 'reported_exact',
          size: item.accuracy === 'reported_exact' ? 5 : 3,
        },
        spanGaps: false,
        value: (_plot: uPlot, raw: number | null) => formatValue(raw),
      })),
    ];
    const markerPlugin = {
      hooks: {
        draw: [
          (plot: uPlot) => {
            const context = plot.ctx;
            context.save();
            context.font = '10px Segoe UI';
            context.textBaseline = 'top';
            for (const marker of markers) {
              const timestamp = new Date(marker.at).getTime() / 1_000;
              if (!Number.isFinite(timestamp)) continue;
              const x = Math.round(plot.valToPos(timestamp, 'x', true));
              context.strokeStyle = marker.color;
              context.fillStyle = marker.color;
              context.setLineDash([4, 4]);
              context.beginPath();
              context.moveTo(x, plot.bbox.top);
              context.lineTo(x, plot.bbox.top + plot.bbox.height);
              context.stroke();
              context.fillText(marker.label, x + 5, plot.bbox.top + 5);
            }
            context.restore();
          },
        ],
      },
    };
    const options: Options = {
      width: Math.max(480, container.clientWidth),
      height: 310,
      title,
      class: 'codex-meter-uplot',
      series: plotSeries,
      plugins: [markerPlugin],
      legend: { show: true, live: true },
      cursor: { show: true, x: true, y: true, drag: { x: true, y: false } },
      axes: [
        {
          stroke: '#829089',
          grid: { stroke: '#26312d', width: 1 },
          values: (_plot, values) =>
            values.map((value) =>
              timeSpanSeconds > 172_800
                ? new Date(value * 1_000).toLocaleDateString([], {
                    month: 'short',
                    day: 'numeric',
                  })
                : new Date(value * 1_000).toLocaleTimeString([], {
                    hour: '2-digit',
                    minute: '2-digit',
                  }),
            ),
        },
        {
          stroke: '#829089',
          grid: { stroke: '#26312d', width: 1 },
          values: (_plot, values) =>
            values.map((value) =>
              isTokenSeries
                ? new Intl.NumberFormat(undefined, {
                    notation: 'compact',
                    maximumFractionDigits: 1,
                  }).format(value)
                : `${value.toFixed(0)}${unit}`,
            ),
        },
      ],
      scales: {
        x: { time: true },
        y: { auto: true },
      },
    };
    const chart = new uPlot(options, data, container);
    const observer = new ResizeObserver(() => {
      if (container.clientWidth > 0) {
        chart.setSize({ width: container.clientWidth, height: 310 });
      }
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
      chart.destroy();
    };
  });
</script>

<figure class="time-series-chart" aria-label={title}>
  <figcaption>
    <strong>{title}</strong>
    <span>{description}</span>
  </figcaption>
  {#if empty}
    <div class="chart-empty" role="status">
      <strong>Chart unavailable</strong>
      <span>At least two compatible observations are required. No synthetic line is shown.</span>
    </div>
  {:else}
    <div class="chart-host" bind:this={host}></div>
    <details class="chart-summary">
      <summary>Accessible chart data</summary>
      <div class="chart-data-table" role="table" aria-label={`${title} data`}>
        {#each timestamps as timestamp, index (timestamp)}
          <div role="row">
            <time datetime={timestamp}>{new Date(timestamp).toLocaleString()}</time>
            {#each series as item (item.label)}
              <span>{item.label}: {formatValue(item.values[index] ?? null)}</span>
            {/each}
          </div>
        {/each}
      </div>
    </details>
  {/if}
</figure>
