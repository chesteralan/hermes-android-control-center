import { readFileSync } from "node:fs";

const requestedVersion = process.argv[2]?.replace(/^v/, "");
if (!requestedVersion) {
  console.error("Usage: node scripts/release-notes.mjs <version-tag>");
  process.exit(2);
}

const changelog = readFileSync("CHANGELOG.md", "utf8");
const headings = [...changelog.matchAll(/^## \[(.+?)\][^\n]*$/gm)];
const sections = headings.map((heading, index) => {
  const contentStart = heading.index + heading[0].length;
  const contentEnd = headings[index + 1]?.index ?? changelog.length;
  return { version: heading[1], content: changelog.slice(contentStart, contentEnd).trim() };
});
const section =
  sections.find(({ version }) => version === requestedVersion) ??
  sections.find(({ version }) => version === "Unreleased");

if (!section?.content) {
  console.error(`No CHANGELOG.md notes found for ${requestedVersion} or Unreleased.`);
  process.exit(1);
}

process.stdout.write(section.content);
