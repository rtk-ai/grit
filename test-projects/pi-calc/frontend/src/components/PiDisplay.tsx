import { Component, createSignal, createEffect, Show } from "solid-js";
import { fetchPi } from "../api/client";
import { formatPiValue, groupDigits } from "../utils/format";
import { AlgorithmPicker } from "./AlgorithmPicker";

interface DigitHighlighterProps {
  digits: string;
  highlightPosition: number | null;
}

const DigitHighlighter: Component<DigitHighlighterProps> = (props) => {
  const grouped = () => groupDigits(props.digits, 5);

  return (
    <div class="digit-display">
      {grouped()
        .split("")
        .map((char, index) => (
          <span
            class={index === props.highlightPosition ? "highlighted" : "digit"}
            data-position={index}
          >
            {char}
          </span>
        ))}
    </div>
  );
};

interface CopyButtonProps {
  value: string;
}

const CopyButton: Component<CopyButtonProps> = (props) => {
  const [copied, setCopied] = createSignal(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(props.value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button class="copy-btn" onClick={handleCopy}>
      {copied() ? "Copied!" : "Copy"}
    </button>
  );
};

interface PrecisionSliderProps {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
}

const PrecisionSlider: Component<PrecisionSliderProps> = (props) => {
  const min = () => props.min ?? 100;
  const max = () => props.max ?? 1_000_000;

  const logValue = () => Math.log10(props.value);
  const logMin = () => Math.log10(min());
  const logMax = () => Math.log10(max());

  const handleInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    const logVal = parseFloat(target.value);
    const actualValue = Math.round(Math.pow(10, logVal));
    props.onChange(actualValue);
  };

  return (
    <div class="precision-slider">
      <label>
        Iterations: {props.value.toLocaleString()}
      </label>
      <input
        type="range"
        min={logMin()}
        max={logMax()}
        step="0.01"
        value={logValue()}
        onInput={handleInput}
      />
    </div>
  );
};

const PiDisplay: Component = () => {
  const [algorithm, setAlgorithm] = createSignal("leibniz");
  const [iterations, setIterations] = createSignal(10000);
  const [piValue, setPiValue] = createSignal<number | null>(null);
  const [correctDigits, setCorrectDigits] = createSignal(0);
  const [elapsedMs, setElapsedMs] = createSignal(0);
  const [loading, setLoading] = createSignal(false);
  const [highlightPos, setHighlightPos] = createSignal<number | null>(null);

  const calculate = async () => {
    setLoading(true);
    try {
      const result = await fetchPi(algorithm(), iterations());
      setPiValue(result.value);
      setCorrectDigits(result.correctDigits);
      setElapsedMs(result.elapsedMs);
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    algorithm();
    iterations();
    calculate();
  });

  const displayValue = () => {
    if (piValue() === null) return "...";
    return formatPiValue(piValue()!, 15);
  };

  return (
    <div class="pi-display">
      <h2>Calculate Pi</h2>

      <AlgorithmPicker selected={algorithm()} onSelect={setAlgorithm} />
      <PrecisionSlider value={iterations()} onChange={setIterations} />

      <Show when={!loading()} fallback={<div class="loading">Calculating...</div>}>
        <div class="result-container">
          <DigitHighlighter digits={displayValue()} highlightPosition={highlightPos()} />
          <CopyButton value={displayValue()} />

          <div class="stats">
            <span>Correct digits: {correctDigits()}</span>
            <span>Time: {elapsedMs().toFixed(2)}ms</span>
          </div>
        </div>
      </Show>

      <div class="digit-selector">
        <label>Highlight digit:</label>
        <input
          type="number"
          min={0}
          max={15}
          value={highlightPos() ?? ""}
          onInput={(e) => {
            const val = parseInt((e.target as HTMLInputElement).value);
            setHighlightPos(isNaN(val) ? null : val);
          }}
        />
      </div>
    </div>
  );
};

export default PiDisplay;
