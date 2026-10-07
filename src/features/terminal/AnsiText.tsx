import type { CSSProperties, ReactNode } from "react";

/* eslint-disable no-control-regex */

interface AnsiTextProps {
  text: string;
}

const BASIC_COLORS = [
  "#202825",
  "#d2533d",
  "#176b58",
  "#b77818",
  "#3572a5",
  "#9467bd",
  "#168c91",
  "#e4e8e2",
  "#68736e",
  "#f06a54",
  "#2b9477",
  "#e3ad41",
  "#62a0d1",
  "#b58bd5",
  "#38aeb1",
  "#ffffff",
];

const SGR = /\u001b\[([0-9;]*)m/g;
const OTHER_ESCAPE_SEQUENCES =
  /\u001b(?:\[[0-?]*[ -/]*[@-~]|\][^\u0007]*(?:\u0007|\u001b\\)|P[^\u001b]*(?:\u001b\\)|(?!\[|\])[@-~])/g;
const INCOMPLETE_ESCAPE_SEQUENCE = /\u001b(?:\[[0-?]*[ -/]*|\][\s\S]*|P[\s\S]*)$/g;

function xtermColor(index: number): string | undefined {
  if (!Number.isInteger(index) || index < 0 || index > 255) return undefined;
  if (index < BASIC_COLORS.length) return BASIC_COLORS[index];
  if (index < 232) {
    const component = (value: number) => (value === 0 ? 0 : 55 + value * 40);
    const cube = index - 16;
    const red = Math.floor(cube / 36);
    const green = Math.floor((cube % 36) / 6);
    const blue = cube % 6;
    return `rgb(${component(red)}, ${component(green)}, ${component(blue)})`;
  }
  const gray = 8 + (index - 232) * 10;
  return `rgb(${gray}, ${gray}, ${gray})`;
}

function rgbColor(red: number, green: number, blue: number): string | undefined {
  if (![red, green, blue].every((value) => Number.isInteger(value) && value >= 0 && value <= 255)) {
    return undefined;
  }
  return `rgb(${red}, ${green}, ${blue})`;
}

function applySgr(current: CSSProperties, parameters: string): CSSProperties {
  const style = { ...current };
  const codes = parameters === "" ? [0] : parameters.split(";").map(Number);
  for (let index = 0; index < codes.length; index += 1) {
    const code = codes[index];
    if (code === undefined) continue;
    if (code === 0) {
      for (const key of Object.keys(style) as Array<keyof CSSProperties>) delete style[key];
    } else if (code === 1) style.fontWeight = 700;
    else if (code === 2) style.opacity = 0.65;
    else if (code === 3) style.fontStyle = "italic";
    else if (code === 4) style.textDecorationLine = "underline";
    else if (code === 22) {
      delete style.fontWeight;
      delete style.opacity;
    } else if (code === 23) delete style.fontStyle;
    else if (code === 24) delete style.textDecorationLine;
    else if (code === 39) delete style.color;
    else if (code === 49) delete style.backgroundColor;
    else if (code >= 30 && code <= 37) style.color = xtermColor(code - 30);
    else if (code >= 90 && code <= 97) style.color = xtermColor(code - 90 + 8);
    else if (code >= 40 && code <= 47) style.backgroundColor = xtermColor(code - 40);
    else if (code >= 100 && code <= 107) style.backgroundColor = xtermColor(code - 100 + 8);
    else if ((code === 38 || code === 48) && codes[index + 1] === 5) {
      const color = xtermColor(codes[index + 2] ?? -1);
      if (color) {
        if (code === 38) style.color = color;
        else style.backgroundColor = color;
      }
      index += 2;
    } else if ((code === 38 || code === 48) && codes[index + 1] === 2) {
      const color = rgbColor(codes[index + 2] ?? -1, codes[index + 3] ?? -1, codes[index + 4] ?? -1);
      if (color) {
        if (code === 38) style.color = color;
        else style.backgroundColor = color;
      }
      index += 4;
    }
  }
  return style;
}

function appendText(nodes: ReactNode[], text: string, style: CSSProperties): void {
  const safeText = text
    .replace(OTHER_ESCAPE_SEQUENCES, "")
    .replace(INCOMPLETE_ESCAPE_SEQUENCE, "")
    .replace(/\u001b/g, "");
  if (!safeText) return;
  if (Object.keys(style).length === 0) nodes.push(safeText);
  else nodes.push(<span key={`ansi-${nodes.length}`} style={style}>{safeText}</span>);
}

export function AnsiText({ text }: AnsiTextProps): ReactNode {
  const nodes: ReactNode[] = [];
  let cursor = 0;
  let style: CSSProperties = {};
  for (const match of text.matchAll(SGR)) {
    const start = match.index ?? cursor;
    appendText(nodes, text.slice(cursor, start), style);
    style = applySgr(style, match[1] ?? "");
    cursor = start + match[0].length;
  }
  appendText(nodes, text.slice(cursor), style);
  return nodes.length === 1 ? (nodes[0] ?? "") : nodes;
}