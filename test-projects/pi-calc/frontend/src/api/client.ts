const API_BASE = "http://localhost:3001/api";

interface PiResult {
  value: number;
  formatted: string;
  algorithm: string;
  iterations: number;
  correctDigits: number;
  error: number;
  elapsedMs: number;
}

interface ComparisonResult {
  algorithm: string;
  value: number;
  correctDigits: number;
  error: number;
  elapsedMs: number;
}

interface DigitResult {
  position: number;
  digit: string;
  errorBound: number;
}

interface ConvergenceData {
  iteration: number;
  value: number;
  correct_digits: number;
  error: number;
}

interface HistoryEntry {
  totalCalculations: number;
  cacheHits: number;
  algorithmsUsed: string[];
  lastCalculation: string | null;
}

interface HealthStatus {
  ok: boolean;
  cacheSize: number;
}

export async function fetchPi(algo: string, iterations: number): Promise<PiResult> {
  const params = new URLSearchParams({
    iterations: iterations.toString(),
    digits: "15",
  });

  const response = await fetch(`${API_BASE}/pi/${algo}?${params}`);
  if (!response.ok) {
    throw new Error(`Failed to fetch pi: ${response.statusText}`);
  }

  const data = await response.json();
  return {
    value: data.value,
    formatted: data.formatted,
    algorithm: data.algorithm,
    iterations: data.iterations,
    correctDigits: data.correct_digits,
    error: data.error,
    elapsedMs: data.elapsed_ms,
  };
}

export async function fetchComparison(iterations: number): Promise<ComparisonResult[]> {
  const params = new URLSearchParams({ iterations: iterations.toString() });
  const response = await fetch(`${API_BASE}/compare?${params}`);

  if (!response.ok) {
    throw new Error(`Failed to fetch comparison: ${response.statusText}`);
  }

  const data = await response.json();
  return data.map((entry: any) => ({
    algorithm: entry.algorithm,
    value: entry.value,
    correctDigits: entry.correct_digits,
    error: entry.error,
    elapsedMs: entry.elapsed_ms,
  }));
}

export async function fetchDigit(position: number): Promise<DigitResult> {
  const response = await fetch(`${API_BASE}/digit/${position}`);

  if (!response.ok) {
    throw new Error(`Failed to fetch digit: ${response.statusText}`);
  }

  const data = await response.json();
  return {
    position: data.position,
    digit: data.digit,
    errorBound: data.error_bound,
  };
}

export function streamConvergence(
  algo: string,
  onData: (data: ConvergenceData) => void
): () => void {
  const eventSource = new EventSource(`${API_BASE}/convergence/${algo}`);

  eventSource.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data) as ConvergenceData;
      onData(data);
    } catch (err) {
      console.error("Failed to parse convergence data:", err);
    }
  };

  eventSource.onerror = () => {
    eventSource.close();
  };

  return () => eventSource.close();
}

export async function fetchHistory(): Promise<HistoryEntry> {
  const response = await fetch(`${API_BASE}/history`);

  if (!response.ok) {
    throw new Error(`Failed to fetch history: ${response.statusText}`);
  }

  const data = await response.json();
  return {
    totalCalculations: data.total_calculations,
    cacheHits: data.cache_hits,
    algorithmsUsed: data.algorithms_used,
    lastCalculation: data.last_calculation,
  };
}

export async function checkHealth(): Promise<HealthStatus> {
  try {
    const response = await fetch(`${API_BASE}/health`);
    if (!response.ok) return { ok: false, cacheSize: 0 };

    const data = await response.json();
    return {
      ok: data.status === "healthy",
      cacheSize: data.cache_size,
    };
  } catch {
    return { ok: false, cacheSize: 0 };
  }
}
