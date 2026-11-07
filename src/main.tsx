import { render } from "@solidjs/web";
import App from "./App";
import "./styles/tokens.css";
import "./styles/app.css";

const root = document.getElementById("root");
if (!root) throw new Error("Studio root element #root is missing from index.html");

render(() => <App />, root);
