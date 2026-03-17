import { Component, createSignal, createResource, For, Show } from "solid-js";
import { fetchComparison } from "../api/client";
import { formatDuration, formatNumber, calculateAccuracy } from "../utils/format";
import { knownPiDigits } from "../utils/math";

interface ComparisonResult {
  algorithm: string;
  value: number;
  correctDigits: number;
  error: number;
  elapsedMs: number;
}

interface ComparisonBarProps {
  label: string;
  value: number;
  maxValue: number;
  color?: string;
}

const ComparisonBar: Component<ComparisonBarProps> = (props) => {
  const percentage = () => {
    if (props.maxValue === 0) return 0;
    return (props.value / props.maxValue) * 100;
  };

  const barColor = () => props.color ?? "#3b82f6";

  return (
    <div class="comparison-bar">
      <span class="bar-label">{props.label}</span>
      <div class="bar-track">
        <div
          class="bar-fill"
          style={{
            width: `${percentage()}%`,
            "background-color": barColor(),
          }}
        />
      </div>
      <span class="bar-value">{props.value.toFixed(2)}</span>
    </div>
  );
};

interface TimingTableProps {
  results: ComparisonResult[];
}

const TimingTable: Component<TimingTableProps> = (props) => {
  const sorted = () => [...props.results].sort((a, b) => a.elapsedMs - b.elapsedMs);

  return (
    <table class="timing-table">
      <thead>
        <tr>
          <th>Rank</th>
          <th>Algorithm</th>
          <th>Time</th>
          <th>Correct Digits</th>
          <th>Accuracy</th>
        </tr>
      </thead>
      <tbody>
        <For each={sorted()}>
          {(result, index) => {
            const accuracy = () => calculateAccuracy(result.value, parseFloat(knownPiDigits().slice(0, 16)));
            return (
              <tr>
                <td>{index() + 1}</td>
                <td>{result.algorithm}</td>
                <td>{formatDuration(result.elapsedMs)}</td>
                <td>{result.correctDigits}</td>
                <td>{accuracy().toFixed(10)}%</td>
              </tr>
            );
          }}
        </For>
      </tbody>
    </table>
  );
};

interface AccuracyBadgeProps {
  correctDigits: number;
}

const AccuracyBadge: Component<AccuracyBadgeProps> = (props) => {
  const level = () => {
    if (props.correctDigits >= 14) return "excellent";
    if (props.correctDigits >= 10) return "great";
    if (props.correctDigits >= 6) return "good";
    if (props.correctDigits >= 3) return "fair";
    return "poor";
  };

  const color = () => {
    switch (level()) {
      case "excellent": return "#22c55e";
      case "great": return "#84cc16";
      case "good": return "#eab308";
      case "fair": return "#f97316";
      default: return "#ef4444";
    }
  };

  return (
    <span
      class={`accuracy-badge badge-${level()}`}
      style={{ "background-color": color() }}
    >
      {props.correctDigits} digits ({level()})
    </span>
  );
};

export const ComparisonView: Component = () => {
  const [iterations, setIterations] = createSignal(10000);

  const [results] = createResource(iterations, async (iters) => {
    const data = await fetchComparison(iters);
    return data as ComparisonResult[];
  });

  const maxTime = () => {
    const r = results();
    if (!r || r.length === 0) return 1;
    return Math.max(...r.map((x) => x.elapsedMs));
  };

  const maxDigits = () => {
    const r = results();
    if (!r || r.length === 0) return 1;
    return Math.max(...r.map((x) => x.correctDigits));
  };

  return (
    <div class="comparison-view">
      <h2>Algorithm Comparison</h2>

      <div class="iteration-selector">
        <label>Iterations: {formatNumber(iterations())}</label>
        <input
          type="range"
          min={2}
          max={6}
          step={0.1}
          value={Math.log10(iterations())}
          onInput={(e) => setIterations(Math.round(Math.pow(10, parseFloat((e.target as HTMLInputElement).value))))}
        />
      </div>

      <Show when={results()} fallback={<p>Loading comparison...</p>}>
        <section>
          <h3>Speed (ms)</h3>
          <For each={results()!}>
            {(result) => (
              <ComparisonBar
                label={result.algorithm}
                value={result.elapsedMs}
                maxValue={maxTime()}
                color="#3b82f6"
              />
            )}
          </For>
        </section>

        <section>
          <h3>Accuracy (correct digits)</h3>
          <For each={results()!}>
            {(result) => (
              <div class="accuracy-row">
                <span>{result.algorithm}</span>
                <AccuracyBadge correctDigits={result.correctDigits} />
              </div>
            )}
          </For>
        </section>

        <section>
          <h3>Detailed Timing</h3>
          <TimingTable results={results()!} />
        </section>
      </Show>
    </div>
  );
};
