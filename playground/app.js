// Preact+htmはCDN経由ではなくvendor/以下にバンドルを同梱している
// （GitHub Pages公開後に外部CDNの可用性へ依存しないようにするため）。
import { html, render, useState, useCallback } from "./vendor/htm-preact-standalone.module.js";
import init, { run_umor } from "./pkg/umor.js";

const SAMPLE = "1　2　加　表示する。";

function App({ ready, error }) {
  const [source, setSource] = useState(SAMPLE);
  const [output, setOutput] = useState("");
  const [running, setRunning] = useState(false);

  const runCode = useCallback(() => {
    setRunning(true);
    try {
      setOutput(run_umor(source));
    } finally {
      setRunning(false);
    }
  }, [source]);

  if (error) {
    return html`<h1>Umor Playground</h1>
      <pre>WASMモジュールの読み込みに失敗しました: ${error}</pre>`;
  }

  return html`
    <h1>Umor Playground</h1>
    <p class="hint">
      Umorコードを入力して「実行」を押してください（単発実行のみ。無限ループを含む
      コードはタブがハングする可能性があります）。
    </p>
    <textarea
      value=${source}
      onInput=${(e) => setSource(e.target.value)}
      spellcheck="false"
    ></textarea>
    <div>
      <button disabled=${!ready || running} onClick=${runCode}>
        ${running ? "実行中..." : "実行"}
      </button>
    </div>
    <pre>${output}</pre>
  `;
}

const root = document.getElementById("app");
render(html`<${App} ready=${false} />`, root);

init()
  .then(() => render(html`<${App} ready=${true} />`, root))
  .catch((e) => render(html`<${App} ready=${false} error=${String(e)} />`, root));
