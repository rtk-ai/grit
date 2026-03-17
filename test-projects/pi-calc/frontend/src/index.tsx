import { render } from "solid-js/web";
import { Router } from "@solidjs/router";
import App from "./App";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Root element not found. Ensure index.html has a div#root.");
}

render(
  () => (
    <Router>
      <App />
    </Router>
  ),
  root
);
