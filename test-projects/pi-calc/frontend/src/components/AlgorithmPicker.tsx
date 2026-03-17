import { Component, For } from "solid-js";

interface AlgorithmInfo {
  id: string;
  name: string;
  description: string;
  formula: string;
  convergenceRate: string;
  relativeSpeed: number;
}

const ALGORITHMS: AlgorithmInfo[] = [
  { id: "leibniz", name: "Leibniz", description: "Alternating series based on arctangent", formula: "pi/4 = 1 - 1/3 + 1/5 - 1/7 + ...", convergenceRate: "O(1/n)", relativeSpeed: 3 },
  { id: "monte_carlo", name: "Monte Carlo", description: "Random sampling in unit square", formula: "pi ~ 4 * (points in circle / total points)", convergenceRate: "O(1/sqrt(n))", relativeSpeed: 2 },
  { id: "nilakantha", name: "Nilakantha", description: "Faster converging infinite series", formula: "pi = 3 + 4/(2*3*4) - 4/(4*5*6) + ...", convergenceRate: "O(1/n^3)", relativeSpeed: 5 },
  { id: "chudnovsky", name: "Chudnovsky", description: "Fastest known convergence for pi", formula: "1/pi = 12 * sum((-1)^k * (6k)! * ...)", convergenceRate: "~14 digits/term", relativeSpeed: 10 },
  { id: "wallis", name: "Wallis Product", description: "Infinite product formula", formula: "pi/2 = (2/1)(2/3)(4/3)(4/5)...", convergenceRate: "O(1/n)", relativeSpeed: 3 },
  { id: "ramanujan", name: "Ramanujan", description: "Ramanujan's remarkable formula", formula: "1/pi = (2sqrt(2)/9801) * sum(...)", convergenceRate: "~8 digits/term", relativeSpeed: 9 },
  { id: "bbp", name: "BBP Formula", description: "Allows digit extraction without prior digits", formula: "pi = sum(1/16^k * (4/(8k+1) - 2/(8k+4) - ...))", convergenceRate: "O(16^-n)", relativeSpeed: 8 },
  { id: "gauss_legendre", name: "Gauss-Legendre", description: "Quadratic convergence via AGM", formula: "pi ~ (a + b)^2 / (4t)", convergenceRate: "O(2^n digits)", relativeSpeed: 10 },
];

interface SpeedIndicatorProps {
  speed: number;
  maxSpeed?: number;
}

const SpeedIndicator: Component<SpeedIndicatorProps> = (props) => {
  const max = () => props.maxSpeed ?? 10;
  const percentage = () => (props.speed / max()) * 100;

  return (
    <div class="speed-indicator">
      <div class="speed-bar" style={{ width: `${percentage()}%` }} />
      <span class="speed-label">{props.speed}/{max()}</span>
    </div>
  );
};

interface AlgorithmDescriptionProps {
  formula: string;
  convergenceRate: string;
}

const AlgorithmDescription: Component<AlgorithmDescriptionProps> = (props) => {
  return (
    <div class="algorithm-description">
      <code class="formula">{props.formula}</code>
      <span class="convergence">Convergence: {props.convergenceRate}</span>
    </div>
  );
};

interface AlgorithmCardProps {
  algorithm: AlgorithmInfo;
  selected: boolean;
  onSelect: (id: string) => void;
}

const AlgorithmCard: Component<AlgorithmCardProps> = (props) => {
  return (
    <div
      class={`algorithm-card ${props.selected ? "selected" : ""}`}
      onClick={() => props.onSelect(props.algorithm.id)}
      role="button"
      tabIndex={0}
    >
      <h3>{props.algorithm.name}</h3>
      <p>{props.algorithm.description}</p>
      <AlgorithmDescription
        formula={props.algorithm.formula}
        convergenceRate={props.algorithm.convergenceRate}
      />
      <SpeedIndicator speed={props.algorithm.relativeSpeed} />
    </div>
  );
};

interface AlgorithmPickerProps {
  selected: string;
  onSelect: (id: string) => void;
}

export const AlgorithmPicker: Component<AlgorithmPickerProps> = (props) => {
  return (
    <div class="algorithm-picker">
      <h3>Select Algorithm</h3>
      <div class="algorithm-grid">
        <For each={ALGORITHMS}>
          {(algo) => (
            <AlgorithmCard
              algorithm={algo}
              selected={props.selected === algo.id}
              onSelect={props.onSelect}
            />
          )}
        </For>
      </div>
    </div>
  );
};
