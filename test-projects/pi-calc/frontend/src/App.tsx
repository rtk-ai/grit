import { Component, createSignal, onMount } from "solid-js";
import { Route, Routes } from "@solidjs/router";
import PiDisplay from "./components/PiDisplay";
import { ComparisonView } from "./components/Comparison";
import { ConvergenceChart } from "./components/Convergence";
import { checkHealth } from "./api/client";

const Header: Component = () => {
  const [activeTab, setActiveTab] = createSignal("calculator");

  return (
    <header>
      <nav>
        <h1>Pi Calculator</h1>
        <ul>
          <li class={activeTab() === "calculator" ? "active" : ""}>
            <a href="/" onClick={() => setActiveTab("calculator")}>
              Calculator
            </a>
          </li>
          <li class={activeTab() === "compare" ? "active" : ""}>
            <a href="/compare" onClick={() => setActiveTab("compare")}>
              Compare
            </a>
          </li>
          <li class={activeTab() === "convergence" ? "active" : ""}>
            <a href="/convergence" onClick={() => setActiveTab("convergence")}>
              Convergence
            </a>
          </li>
        </ul>
      </nav>
    </header>
  );
};

const Footer: Component = () => {
  const [healthy, setHealthy] = createSignal(false);
  const [calculationCount, setCalculationCount] = createSignal(0);

  onMount(async () => {
    const status = await checkHealth();
    setHealthy(status.ok);
    setCalculationCount(status.cacheSize);
  });

  return (
    <footer>
      <div class="footer-content">
        <span>Pi Calculator v0.1.0</span>
        <span class={healthy() ? "status-ok" : "status-error"}>
          API: {healthy() ? "Connected" : "Disconnected"}
        </span>
        <span>Calculations: {calculationCount()}</span>
      </div>
    </footer>
  );
};

const App: Component = () => {
  return (
    <div class="app">
      <Header />
      <main>
        <Routes>
          <Route path="/" component={PiDisplay} />
          <Route path="/compare" component={ComparisonView} />
          <Route path="/convergence" component={() => <ConvergenceChart algorithm="leibniz" iterations={1000} />} />
        </Routes>
      </main>
      <Footer />
    </div>
  );
};

export default App;
