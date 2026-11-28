/**
 * Safe KaTeX math rendering for Boomergames chat messages.
 *
 * Supports `$...$`, `$$...$$`, `\(...\)`, and `\[...\]` delimiters, plus casual
 * TeX fraction shorthand (e.g. `$5 over 2$` -> `5 \over 2`), while keeping plain
 * text strictly DOM-escaped (`createTextNode`) and enforcing `trust: false` and
 * bounded macro expansion (`maxExpand: 100`) to prevent XSS or macro DoS.
 */

interface KatexGlobal {
  renderToString(
    tex: string,
    options?: {
      readonly displayMode?: boolean;
      readonly throwOnError?: boolean;
      readonly strict?: boolean;
      readonly trust?: boolean;
      readonly maxExpand?: number;
      readonly maxSize?: number;
    },
  ): string;
}

interface KatexWindow extends Window {
  katex?: KatexGlobal;
}

interface ChatToken {
  readonly kind: "text" | "math";
  readonly content: string;
  readonly displayMode: boolean;
}

const pendingNodes: Array<{ readonly target: HTMLElement; readonly text: string }> = [];
let loadListenerAttached = false;

function getKatex(): KatexGlobal | null {
  return (window as KatexWindow).katex ?? null;
}

function scheduleRerenderOnLoad(target: HTMLElement, text: string): void {
  pendingNodes.push({ target, text });
  if (loadListenerAttached) {
    return;
  }
  loadListenerAttached = true;
  window.addEventListener(
    "load",
    () => {
      if (getKatex() === null) {
        pendingNodes.length = 0;
        return;
      }
      const batch = pendingNodes.splice(0, pendingNodes.length);
      for (const entry of batch) {
        if (entry.target.isConnected) {
          entry.target.textContent = "";
          appendChatTextWithKatex(entry.target, entry.text);
        }
      }
    },
    { once: true },
  );
}

/**
 * Normalizes casual `over` fraction shorthand inside math delimiters (e.g. `$5 over 2$` -> `5 \over 2`)
 * without touching already-escaped `\over`, `\overline`, `\overset`, or words like `cover`.
 */
function normalizeMathExpr(expr: string): string {
  return expr.replace(/(^|[^\\a-zA-Z])over(?![a-zA-Z])/g, "$1\\over");
}

function isDigit(ch: string | undefined): boolean {
  return ch !== undefined && ch >= "0" && ch <= "9";
}

function tokenizeChatMath(input: string): ChatToken[] {
  const tokens: ChatToken[] = [];
  let textBuf = "";
  let i = 0;

  const flushText = (): void => {
    if (textBuf !== "") {
      tokens.push({ kind: "text", content: textBuf, displayMode: false });
      textBuf = "";
    }
  };

  while (i < input.length) {
    // Display math: $$ ... $$
    if (input.startsWith("$$", i)) {
      const close = input.indexOf("$$", i + 2);
      if (close > i + 2) {
        const inner = input.slice(i + 2, close).trim();
        if (inner !== "") {
          flushText();
          tokens.push({ kind: "math", content: inner, displayMode: true });
          i = close + 2;
          continue;
        }
      }
    }

    // Display math: \[ ... \]
    if (input.startsWith("\\[", i)) {
      const close = input.indexOf("\\]", i + 2);
      if (close > i + 2) {
        const inner = input.slice(i + 2, close).trim();
        if (inner !== "") {
          flushText();
          tokens.push({ kind: "math", content: inner, displayMode: true });
          i = close + 2;
          continue;
        }
      }
    }

    // Inline math: \( ... \)
    if (input.startsWith("\\(", i)) {
      const close = input.indexOf("\\)", i + 2);
      if (close > i + 2) {
        const inner = input.slice(i + 2, close).trim();
        if (inner !== "") {
          flushText();
          tokens.push({ kind: "math", content: inner, displayMode: false });
          i = close + 2;
          continue;
        }
      }
    }

    // Inline math: $ ... $ (closing `$` must not be immediately followed by a digit,
    // avoiding false positives on currency ranges like "$5 and $10").
    if (input[i] === "$" && input[i + 1] !== "$") {
      let close = input.indexOf("$", i + 1);
      while (close !== -1 && (input[close - 1] === "\\" || isDigit(input[close + 1]))) {
        close = input.indexOf("$", close + 1);
      }
      if (close > i + 1) {
        const inner = input.slice(i + 1, close).trim();
        if (inner !== "") {
          flushText();
          tokens.push({ kind: "math", content: inner, displayMode: false });
          i = close + 1;
          continue;
        }
      }
    }

    textBuf += input[i] ?? "";
    i += 1;
  }

  flushText();
  return tokens;
}

/**
 * Appends `text` into `container`, rendering any TeX/KaTeX expressions via KaTeX CDN
 * while safely escaping all surrounding plain text via `document.createTextNode`.
 */
export function appendChatTextWithKatex(container: HTMLElement, text: string): void {
  const tokens = tokenizeChatMath(text);
  const hasMath = tokens.some((t) => t.kind === "math");
  if (!hasMath) {
    container.append(document.createTextNode(text));
    return;
  }

  const katex = getKatex();
  if (katex === null) {
    const span = document.createElement("span");
    span.textContent = text;
    container.append(span);
    scheduleRerenderOnLoad(span, text);
    return;
  }

  for (const tok of tokens) {
    if (tok.kind === "text") {
      container.append(document.createTextNode(tok.content));
      continue;
    }
    const expr = normalizeMathExpr(tok.content);
    try {
      const html = katex.renderToString(expr, {
        displayMode: tok.displayMode,
        throwOnError: false,
        strict: false,
        trust: false,
        maxExpand: 100,
        maxSize: 10,
      });
      const mathSpan = document.createElement("span");
      mathSpan.className = tok.displayMode ? "chat-math-display" : "chat-math-inline";
      mathSpan.innerHTML = html;
      container.append(mathSpan);
    } catch {
      container.append(document.createTextNode(tok.content));
    }
  }
}
