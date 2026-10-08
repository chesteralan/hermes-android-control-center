import { isMap, isScalar, isSeq, parseDocument } from "yaml";

const MASKED_SECRET = "********";
const SECRET_KEY =
  /password|secret|token|api[-_]?keys?|access[-_]?keys?|private[-_]?keys?|credential/i;

export type MaskedHermesSecrets = Record<string, string>;

function parseHermesDocument(source: string) {
  const document = parseDocument(source);
  if (document.errors.length > 0) {
    throw new Error("Hermes config.yaml contains invalid YAML.");
  }
  if (document.contents !== null && !isMap(document.contents)) {
    throw new Error("Hermes config.yaml must contain a YAML mapping at its root.");
  }
  return document;
}

function visitSecretScalars(
  node: unknown,
  path: Array<string | number>,
  visit: (path: Array<string | number>, value: unknown) => unknown,
  secretAncestor = false,
): void {
  if (isMap(node)) {
    for (const pair of node.items) {
      const key = isScalar(pair.key) ? pair.key.value : null;
      if (typeof key !== "string") continue;
      const childPath = [...path, key];
      const isSecret = secretAncestor || SECRET_KEY.test(key);
      if (isSecret && isScalar(pair.value) && typeof pair.value.value === "string") {
        pair.value.value = visit(childPath, pair.value.value);
      } else {
        visitSecretScalars(pair.value, childPath, visit, isSecret);
      }
    }
  } else if (isSeq(node)) {
    node.items.forEach((item, index) => {
      const itemPath = [...path, index];
      if (secretAncestor && isScalar(item) && typeof item.value === "string") {
        item.value = visit(itemPath, item.value);
      } else {
        visitSecretScalars(item, itemPath, visit, secretAncestor);
      }
    });
  }
}

export function maskHermesConfig(source: string): {
  source: string;
  secrets: MaskedHermesSecrets;
} {
  const document = parseHermesDocument(source);
  const secrets: MaskedHermesSecrets = {};
  visitSecretScalars(document.contents, [], (path, value) => {
    const pathKey = JSON.stringify(path);
    secrets[pathKey] = value as string;
    return MASKED_SECRET;
  });
  return { source: document.toString(), secrets };
}

export function restoreHermesConfigSecrets(
  source: string,
  secrets: MaskedHermesSecrets,
): string {
  const document = parseHermesDocument(source);
  visitSecretScalars(document.contents, [], (path, value) => {
    const original = secrets[JSON.stringify(path)];
    return value === MASKED_SECRET && original !== undefined ? original : value;
  });
  if (document.contents === null) {
    throw new Error("Hermes config.yaml cannot be empty.");
  }
  return document.toString();
}