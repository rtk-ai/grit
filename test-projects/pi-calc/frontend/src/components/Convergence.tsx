import { Component, createSignal, onMount, onCleanup, For, Show } from "solid-js";
import { streamConvergence } from "../api/client";
import { formatPiValue, formatDuration } from "../utils/format";
import { countCorrectDigits } from "../utils/math";

interface ConvergencePoint {
  iteration: number;
  value: number;
  correctDigits: number;
  error: number;
}

interface ConvergenceChartProps {
  algorithm: string;
  iterations: number;
}

export const ConvergenceChart: Component<ConvergenceChartProps> = (props) => {
  const [points, setPoints] = createSignal<ConvergencePoint[]>([]);
  const [running, setRunning] = createSignal(false);
  let cleanup: (() => void) | null = null;

  const startStream = () => {
    setPoints([]);
    setRunning(true);

    cleanup = streamConvergence(props.algorithm, (data) => {
      setPoints((prev) => [
        ...prev,
        {
          iteration: data.iteration,
          value: data.value,
          correctDigits: data.correct_digits,
          error: data.error,
        },
      ]);
    });
  };

  onMount(() => startStream());
  onCleanup(() => cleanup?.());

  const maxError = () => {
    const errs = points().map((p) => p.error);
    return errs.length > 0 ? Math.max(...errs) : 1;
  };

  const barHeight = (error: number) => {
    if (maxError() === 0) return 0;
    return (error / maxError()) * 200;
  };

  return (
    <div class="convergence-chart">
      <h3>Convergence: {props.algorithm}</h3>
      <Show when={points().length > 0} fallback={<p>Waiting for data...</p>}>
        <div class="chart-container">
          <For each={points()}>
            {(point) => (
              <div class="chart-bar" style={{ height: `${barHeight(point.error)}px` }} title={`Iteration ${point.iteration}: error=${point.error.toExponential(2)}`} />
            )}
          </For>
        </div>
      </Show>
      <button onClick={startStream} disabled={running()}>
        {running() ? "Running..." : "Restart"}
      </button>
    </div>
  );
};

interface ConvergenceTableProps {
  points: ConvergencePoint[];
  maxRows?: number;
}

export const ConvergenceTable: Component<ConvergenceTableProps> = (props) => {
  const rows = () => {
    const max = props.maxRows ?? 50;
    const all = props.points;
    if (all.length <= max) return all;

    const step = Math.ceil(all.length / max);
    return all.filter((_, i) => i % step === 0 || i === all.length - 1);
  };

  return (
    <table class="convergence-table">
      <thead>
        <tr>
          <th>Iteration</th>
          <th>Value</th>
          <th>Correct Digits</th>
          <th>Error</th>
        </tr>
      </thead>
      <tbody>
        <For each={rows()}>
          {(point) => (
            <tr>
              <td>{point.iteration.toLocaleString()}</td>
              <td>{formatPiValue(point.value, 12)}</td>
              <td>{point.correctDigits}</td>
              <td>{point.error.toExponential(4)}</td>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
};

interface ErrorPlotProps {
  points: ConvergencePoint[];
  width?: number;
  height?: number;
}

export const ErrorPlot: Component<ErrorPlotProps> = (props) => {
  const width = () => props.width ?? 600;
  const height = () => props.height ?? 300;

  const pathData = () => {
    const pts = props.points;
    if (pts.length === 0) return "";

    const xScale = width() / Math.max(pts.length - 1, 1);
    const maxLogError = Math.max(...pts.map((p) => (p.error > 0 ? Math.log10(p.error) : -16)));
    const minLogError = Math.min(...pts.map((p) => (p.error > 0 ? Math.log10(p.error) : -16)));
    const yRange = maxLogError - minLogError || 1;

    return pts
      .map((p, i) => {
        const x = i * xScale;
        const logErr = p.error > 0 ? Math.log10(p.error) : -16;
        const y = height() - ((logErr - minLogError) / yRange) * height();
        return `${i === 0 ? "M" : "L"}${x},${y}`;
      })
      .join(" ");
  };

  return (
    <svg class="error-plot" width={width()} height={height()} viewBox={`0 0 ${width()} ${height()}`}>
      <path d={pathData()} fill="none" stroke="#3b82f6" stroke-width="2" />
      <text x={width() / 2} y={height() - 5} text-anchor="middle" font-size="12">
        Iterations
      </text>
      <text x={5} y={15} font-size="12">
        log(error)
      </text>
    </svg>
  );
};

interface AnimatedDigitsProps {
  targetValue: string;
  revealedCount: number;
}

export const AnimatedDigits: Component<AnimatedDigitsProps> = (props) => {
  const digits = () => props.targetValue.split("");

  return (
    <div class="animated-digits">
      <For each={digits()}>
        {(digit, index) => {
          const revealed = () => index() < props.revealedCount;
          const correct = () => countCorrectDigits(parseFloat(props.targetValue.slice(0, index() + 1)));

          return (
            <span
              class={`digit ${revealed() ? "revealed" : "hidden"} ${correct() > index() ? "correct" : ""}`}
            >
              {revealed() ? digit : "?"}
            </span>
          );
        }}
      </For>
    </div>
  );
};
